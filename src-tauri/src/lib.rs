use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use std::convert::Infallible;
use std::io::BufReader;
use std::time::Duration;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::post,
    Json, Router,
};
#[cfg(target_os = "macos")]
use cocoa::appkit::{NSColor, NSWindow};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil, YES};
use rodio::{Decoder, OutputStream, Sink};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(desktop)]
use tauri::{image::Image, menu::MenuBuilder, menu::MenuItem, tray::TrayIconBuilder};
use tauri::{
    plugin::PermissionState, webview::Color, Manager, TitleBarStyle, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_store::StoreExt;
use tokio::sync::{Mutex, RwLock};
use tokio::{task, time};
use tokio_stream::{wrappers::IntervalStream, StreamExt};

// Keep the default notification sound embedded so it ships with the app.
const DEFAULT_SOUND: &[u8] = include_bytes!("../sounds/Ping.wav");
const DISABLE_SOUND_ENV: &str = "AGENT_NOTIFIER_DISABLE_SOUND";
// MCP HTTP Stream transport as of the 2025-11-25 specification.
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const STORE_FILE: &str = "settings.json";
const HTTP_SETTINGS_KEY: &str = "httpBindings";
// Theme background: oklch(0.1649 0.0352 281.8285) ≈ #0c0c1d.
const THEME_BACKGROUND_COLOR: Color = Color(12, 12, 29, 255);

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn get_http_bindings(state: tauri::State<'_, ManagedState>) -> Result<HttpSettings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
async fn save_http_bindings(
    settings: HttpSettings,
    app: tauri::AppHandle,
    state: tauri::State<'_, ManagedState>,
) -> Result<(), String> {
    validate_http_settings(&settings)?;

    {
        let mut guard = state.settings.write().await;
        *guard = settings.clone();
    }

    persist_http_settings(&app, &settings)?;
    restart_http_server(&app, &state).await
}

#[derive(Clone)]
struct AppState {
    app: tauri::AppHandle,
    listening: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct NotifyRequest {
    title: String,
    content: String,
    agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpSettings {
    bind_address: String,
    port: u16,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".into(),
            port: 60766,
        }
    }
}

struct ManagedState {
    listening: Arc<AtomicBool>,
    server_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    settings: RwLock<HttpSettings>,
}

// Windows toast text blocks cap at 1024 chars; keep a conservative ceiling to avoid truncation.
const MAX_NOTIFICATION_BODY_CHARS: usize = 1000;
// Soft limit to match the SKILL guidance; keeps space for the agent prefix.
const SOFT_CONTENT_LIMIT_CHARS: usize = 950;

fn play_notification_sound() {
    // Allow opting out (useful for CI or silent environments).
    if std::env::var_os(DISABLE_SOUND_ENV).is_some() {
        return;
    }

    // Spawn onto Tokio so we don't block the HTTP handler while audio initializes/plays.
    task::spawn_blocking(|| {
        let Ok((stream, handle)) = OutputStream::try_default() else {
            eprintln!("Audio output init failed");
            return;
        };

        let cursor = std::io::Cursor::new(DEFAULT_SOUND);
        let reader = BufReader::new(cursor);
        let decoder = match Decoder::new(reader) {
            Ok(decoder) => decoder,
            Err(err) => {
                eprintln!("Failed to decode notification sound: {err}");
                return;
            }
        };

        let sink = match Sink::try_new(&handle) {
            Ok(sink) => sink,
            Err(err) => {
                eprintln!("Failed to create audio sink: {err}");
                return;
            }
        };

        sink.append(decoder);
        // Block this worker thread until playback finishes so the stream stays alive.
        sink.sleep_until_end();

        // Keep the stream alive until sink finishes.
        drop(stream);
    });
}

fn ensure_notification_permission(app: &tauri::AppHandle) {
    // Best-effort permission check and request so macOS users get the system prompt up front.
    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => {}
        Ok(PermissionState::Prompt | PermissionState::PromptWithRationale) => {
            if let Err(err) = app.notification().request_permission() {
                eprintln!("Notification permission request failed: {err}");
            }
        }
        Ok(PermissionState::Denied) => {
            eprintln!("Notification permission is denied for this app.");
        }
        Err(err) => eprintln!("Unable to read notification permission state: {err}"),
    }

    #[cfg(target_os = "macos")]
    if tauri::is_dev() {
        // In dev builds the notification bundle id maps to Terminal; users may need to allow it.
        eprintln!(
            "macOS dev note: notifications appear as 'Terminal'. \
             Enable Terminal notifications in System Settings > Notifications to see them."
        );
    }
}

fn validate_notification_fields(
    title: &str,
    content: &str,
    agent: &str,
) -> Result<(String, String, String), String> {
    let title = title.trim();
    let content = content.trim();
    let agent = agent.trim();

    if title.is_empty() || content.is_empty() || agent.is_empty() {
        return Err("'title', 'content', and 'agent' are required".into());
    }

    let content_len = content.chars().count();
    if content_len > SOFT_CONTENT_LIMIT_CHARS {
        return Err(format!(
            "'content' is too long ({content_len} chars); keep it under {SOFT_CONTENT_LIMIT_CHARS}"
        ));
    }

    Ok((title.to_owned(), content.to_owned(), agent.to_owned()))
}

fn dispatch_notification(
    state: &AppState,
    title: &str,
    content: &str,
    agent: &str,
) -> Result<(), String> {
    let body = format!("{agent}: {content}");
    let limited_content: String = body.chars().take(MAX_NOTIFICATION_BODY_CHARS).collect();

    state
        .app
        .notification()
        .builder()
        .title(title)
        .body(&limited_content)
        .show()
        .map_err(|err| format!("Failed to dispatch notification: {err}"))?;

    play_notification_sound();
    Ok(())
}

fn notify_tool_descriptor() -> Value {
    json!({
        "name": "notify",
        "description": "Send a desktop notification via the Agent Notifications app with title, content, and agent label.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "title": { "type": "string", "minLength": 1 },
                "content": { "type": "string", "minLength": 1, "maxLength": SOFT_CONTENT_LIMIT_CHARS as i64 },
                "agent": { "type": "string", "minLength": 1 }
            },
            "required": ["title", "content", "agent"],
            "additionalProperties": false
        }
    })
}

fn jsonrpc_success(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn jsonrpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}

async fn notify_handler(
    State(state): State<AppState>,
    Json(payload): Json<NotifyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !state.listening.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "message": "Server is not listening" })),
        );
    }

    let title = payload.title.trim();
    let content = payload.content.trim();
    let agent = payload.agent.trim();

    if title.is_empty() || content.is_empty() || agent.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "message": "'title', 'content', and 'agent' are required" })),
        );
    }

    if let Err(err) = dispatch_notification(&state, title, content, agent) {
        eprintln!("{err}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "message": "Failed to dispatch notification" })),
        );
    }

    (
        StatusCode::OK,
        Json(json!({ "message": "Notification dispatched" })),
    )
}

async fn mcp_post_handler(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if !state.listening.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "message": "Server is not listening" })),
        )
            .into_response();
    }

    // If this is a response or notification (no method), acknowledge and stop.
    if body.get("method").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    // Notifications with a method but no id: accept and do nothing.
    if body.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    let Some(method) = body.get("method").and_then(Value::as_str) else {
        return (
            StatusCode::OK,
            Json(jsonrpc_error(
                None,
                -32600,
                "Invalid request: method must be a string",
            )),
        )
            .into_response();
    };

    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let params = body.get("params");

    match method {
        "initialize" => {
            let result = json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": {
                    "name": "agent-notifications",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "tools": { "listChanged": false }
                }
            });
            (StatusCode::OK, Json(jsonrpc_success(id, result))).into_response()
        }
        "tools/list" => {
            let result = json!({
                "tools": [notify_tool_descriptor()],
                "nextCursor": Value::Null
            });
            (StatusCode::OK, Json(jsonrpc_success(id, result))).into_response()
        }
        "tools/call" => {
            let Some(param_obj) = params.and_then(Value::as_object) else {
                return (
                    StatusCode::OK,
                    Json(jsonrpc_error(
                        Some(id),
                        -32602,
                        "Invalid params: expected object",
                    )),
                )
                    .into_response();
            };

            let Some(tool_name) = param_obj.get("name").and_then(Value::as_str) else {
                return (
                    StatusCode::OK,
                    Json(jsonrpc_error(
                        Some(id),
                        -32602,
                        "Invalid params: missing tool name",
                    )),
                )
                    .into_response();
            };

            if tool_name != "notify" {
                return (
                    StatusCode::OK,
                    Json(jsonrpc_error(Some(id), -32601, "Tool not found")),
                )
                    .into_response();
            }

            let Some(arguments) = param_obj.get("arguments").and_then(Value::as_object) else {
                return (
                    StatusCode::OK,
                    Json(jsonrpc_error(
                        Some(id),
                        -32602,
                        "Invalid params: 'arguments' must be an object",
                    )),
                )
                    .into_response();
            };

            let title = arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let content = arguments
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let agent = arguments
                .get("agent")
                .and_then(Value::as_str)
                .unwrap_or_default();

            let Ok((title, content, agent)) = validate_notification_fields(title, content, agent)
            else {
                return (
                    StatusCode::OK,
                    Json(jsonrpc_error(
                        Some(id),
                        -32602,
                        "Invalid params: 'title', 'content', and 'agent' are required and must be within limits",
                    )),
                )
                    .into_response();
            };

            if let Err(err) = dispatch_notification(&state, &title, &content, &agent) {
                eprintln!("{err}");
                return (
                    StatusCode::OK,
                    Json(jsonrpc_error(
                        Some(id),
                        -32000,
                        "Failed to dispatch notification",
                    )),
                )
                    .into_response();
            }

            let result = json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Notification sent: {title}")
                    }
                ],
                "isError": false
            });

            (StatusCode::OK, Json(jsonrpc_success(id, result))).into_response()
        }
        _ => (
            StatusCode::OK,
            Json(jsonrpc_error(Some(id), -32601, "Method not found")),
        )
            .into_response(),
    }
}

async fn mcp_get_handler(State(state): State<AppState>) -> impl IntoResponse {
    if !state.listening.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "message": "Server is not listening" })),
        )
            .into_response();
    }

    let stream = IntervalStream::new(time::interval(Duration::from_secs(25)))
        .map(|_| Ok::<Event, Infallible>(Event::default().comment("keep-alive")));

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(25)))
        .into_response()
}
fn validate_http_settings(settings: &HttpSettings) -> Result<(), String> {
    if settings.bind_address.trim().is_empty() {
        return Err("Bind address cannot be empty".into());
    }
    if settings.port == 0 {
        return Err("Port must be between 1 and 65535".into());
    }
    Ok(())
}

fn load_http_settings(app: &tauri::AppHandle) -> HttpSettings {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("Failed to open settings store: {err}");
            return HttpSettings::default();
        }
    };

    match store.get(HTTP_SETTINGS_KEY) {
        Some(value) => match serde_json::from_value::<HttpSettings>(value.clone()) {
            Ok(settings) => settings,
            Err(err) => {
                eprintln!("Failed to parse stored HTTP settings: {err}");
                HttpSettings::default()
            }
        },
        None => HttpSettings::default(),
    }
}

fn persist_http_settings(app: &tauri::AppHandle, settings: &HttpSettings) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|err| format!("Failed to open settings store: {err}"))?;
    store.set(
        HTTP_SETTINGS_KEY,
        serde_json::to_value(settings)
            .map_err(|err| format!("Failed to serialize HTTP settings: {err}"))?,
    );
    store
        .save()
        .map_err(|err| format!("Failed to save HTTP settings: {err}"))
}

fn spawn_http_server(
    app: tauri::AppHandle,
    listening: Arc<AtomicBool>,
    settings: HttpSettings,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let state = AppState { app, listening };
        let router = Router::new()
            .route("/agent/notify", post(notify_handler))
            .route("/mcp", post(mcp_post_handler).get(mcp_get_handler))
            .with_state(state);

        let bind_addr = format!("{}:{}", settings.bind_address, settings.port);
        let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("HTTP server failed to bind {bind_addr}: {err}");
                return;
            }
        };

        if let Err(err) = axum::serve(listener, router).await {
            eprintln!("HTTP server error: {err}");
        }
    })
}

async fn restart_http_server(app: &tauri::AppHandle, managed: &ManagedState) -> Result<(), String> {
    let settings = { managed.settings.read().await.clone() };

    let mut guard = managed.server_task.lock().await;
    if let Some(handle) = guard.take() {
        handle.abort();
    }
    *guard = Some(spawn_http_server(
        app.clone(),
        managed.listening.clone(),
        settings,
    ));
    Ok(())
}

#[cfg(desktop)]
fn setup_tray(app: &tauri::AppHandle, listening: Arc<AtomicBool>) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open_window", "Settings", true, None::<&str>)?;
    let start_item = MenuItem::with_id(
        app,
        "start_listening",
        "Start listening",
        false,
        None::<&str>,
    )?;
    let stop_item = MenuItem::with_id(app, "stop_listening", "Stop listening", true, None::<&str>)?;

    if !listening.load(Ordering::SeqCst) {
        // Ensure menu reflects the actual state if we ever start with listening disabled.
        start_item.set_enabled(true)?;
        stop_item.set_enabled(false)?;
    }

    let menu = MenuBuilder::new(app)
        .item(&open_item)
        .separator()
        .item(&start_item)
        .item(&stop_item)
        .separator()
        .text("quit", "Quit")
        .build()?;

    let tray_icon = match Image::from_bytes(include_bytes!("../icons/agent-notifier-tray-icon.png"))
    {
        Ok(icon) => Some(icon),
        Err(err) => {
            eprintln!("Failed to decode tray icon, falling back to window icon: {err}");
            None
        }
    };

    // Prefer the app's default icon when available so the tray icon matches the window.
    let mut tray_builder = TrayIconBuilder::new().menu(&menu).on_menu_event({
        let start_item = start_item.clone();
        let stop_item = stop_item.clone();
        move |app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "open_window" => {
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(err) = window.show() {
                        eprintln!("Failed to show main window: {err}");
                    }
                    if let Err(err) = window.unminimize() {
                        eprintln!("Failed to unminimize main window: {err}");
                    }
                    if let Err(err) = window.set_focus() {
                        eprintln!("Failed to focus main window: {err}");
                    }
                } else {
                    eprintln!("Main window not found when handling tray 'Settings'");
                }
            }
            "stop_listening" => {
                listening.store(false, Ordering::SeqCst);
                if let Err(err) = stop_item.set_enabled(false) {
                    eprintln!("Failed to disable 'Stop listening' menu item: {err}");
                }
                if let Err(err) = start_item.set_enabled(true) {
                    eprintln!("Failed to enable 'Start listening' menu item: {err}");
                }
            }
            "start_listening" => {
                listening.store(true, Ordering::SeqCst);
                if let Err(err) = start_item.set_enabled(false) {
                    eprintln!("Failed to disable 'Start listening' menu item: {err}");
                }
                if let Err(err) = stop_item.set_enabled(true) {
                    eprintln!("Failed to enable 'Stop listening' menu item: {err}");
                }
            }
            _ => {}
        }
    });

    if let Some(icon) = tray_icon {
        tray_builder = tray_builder.icon(icon);
    } else if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }

    tray_builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            let app_handle = app.handle();
            let mut window_builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Settings")
                .inner_size(860.0, 680.0)
                .background_color(THEME_BACKGROUND_COLOR)
                .resizable(true)
                // Start hidden; the user opens it from the tray.
                .visible(false);

            #[cfg(target_os = "macos")]
            {
                window_builder = window_builder.title_bar_style(TitleBarStyle::Transparent);
            }

            let window = window_builder.build()?;

            // Keep the app alive when the user clicks the window close button.
            let window_for_close = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(err) = window_for_close.hide() {
                        eprintln!("Failed to hide main window on close: {err}");
                    }
                }
            });

            #[cfg(target_os = "macos")]
            {
                if let Ok(ns_window) = window.ns_window() {
                    let ns_window = ns_window as id;
                    unsafe {
                        let (r, g, b) = (
                            THEME_BACKGROUND_COLOR.0 as f64 / 255.0,
                            THEME_BACKGROUND_COLOR.1 as f64 / 255.0,
                            THEME_BACKGROUND_COLOR.2 as f64 / 255.0,
                        );
                        let bg = NSColor::colorWithRed_green_blue_alpha_(nil, r, g, b, 1.0);
                        ns_window.setTitlebarAppearsTransparent_(YES);
                        ns_window.setBackgroundColor_(bg);
                    }
                }
            }

            // Fire and forget: the task stops automatically when the process exits.
            let listening = Arc::new(AtomicBool::new(true));
            ensure_notification_permission(&app_handle);
            let initial_settings = load_http_settings(&app_handle);
            let managed_state = ManagedState {
                listening: listening.clone(),
                server_task: Mutex::new(None),
                settings: RwLock::new(initial_settings.clone()),
            };

            tauri::async_runtime::block_on(async {
                let handle =
                    spawn_http_server(app_handle.clone(), listening.clone(), initial_settings);
                *managed_state.server_task.lock().await = Some(handle);
            });

            app.manage(managed_state);
            #[cfg(desktop)]
            setup_tray(&app_handle, listening)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_http_bindings,
            save_http_bindings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
