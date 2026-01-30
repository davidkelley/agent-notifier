import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  ChevronRight,
  Loader2,
  Network,
  Server,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

type View = "root" | "http";

type HttpSettings = {
  bind_address: string;
  port: number;
};

const DEFAULT_SETTINGS: HttpSettings = {
  bind_address: "127.0.0.1",
  port: 60766,
};

type Status = { type: "success" | "error"; message: string } | null;

function App() {
  const [view, setView] = useState<View>("root");
  const [form, setForm] = useState({
    bind_address: DEFAULT_SETTINGS.bind_address,
    port: DEFAULT_SETTINGS.port.toString(),
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<Status>(null);

  const bindingPreview = useMemo(
    () => `${form.bind_address || "—"}:${form.port || "—"}`,
    [form],
  );

  useEffect(() => {
    loadSettings();
  }, []);

  async function loadSettings() {
    setLoading(true);
    setStatus(null);
    try {
      const result = await invoke<HttpSettings>("get_http_bindings");
      setForm({
        bind_address: result.bind_address,
        port: result.port.toString(),
      });
    } catch (err) {
      console.error(err);
      setStatus({
        type: "error",
        message: "Failed to load settings from the app",
      });
    } finally {
      setLoading(false);
    }
  }

  function updateField(key: "bind_address" | "port", value: string) {
    setForm((prev) => ({ ...prev, [key]: value }));
    setStatus(null);
  }

  async function saveSettings() {
    setStatus(null);
    const trimmedAddress = form.bind_address.trim();
    const parsedPort = Number(form.port);
    if (!trimmedAddress) {
      setStatus({ type: "error", message: "Bind address cannot be empty" });
      return;
    }
    if (!Number.isInteger(parsedPort) || parsedPort < 1 || parsedPort > 65535) {
      setStatus({ type: "error", message: "Port must be between 1 and 65535" });
      return;
    }

    setSaving(true);
    try {
      await invoke("save_http_bindings", {
        settings: { bind_address: trimmedAddress, port: parsedPort },
      });
      setStatus({ type: "success", message: "HTTP bindings saved" });
    } catch (err) {
      console.error(err);
      setStatus({ type: "error", message: "Unable to save settings" });
    } finally {
      setSaving(false);
    }
  }

  function resetDefaults() {
    setForm({
      bind_address: DEFAULT_SETTINGS.bind_address,
      port: DEFAULT_SETTINGS.port.toString(),
    });
    setStatus(null);
  }

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto max-w-3xl px-6 pt-6 pb-6">
        <div className="pt-2">
          {view === "root" ? (
            <div className="space-y-3">
              <p className="text-sm font-semibold text-foreground">General</p>
              <div className="divide-y divide-border overflow-hidden rounded-md border border-border bg-card">
                <button
                  className="flex w-full items-center justify-between px-4 py-3 text-left transition-colors hover:bg-accent hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  onClick={() => setView("http")}
                >
                  <div className="flex items-center gap-3">
                    <Network className="h-4 w-4" />
                    <p className="text-sm font-medium">HTTP Bindings</p>
                  </div>
                  <div className="flex items-center gap-3 text-sm text-muted">
                    <span>{bindingPreview}</span>
                    <ChevronRight className="h-4 w-4" />
                  </div>
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              <div className="flex items-center gap-3">
                <button
                  className="inline-flex items-center gap-2 rounded-full bg-muted px-3 py-1 text-sm text-foreground transition-colors hover:bg-muted/80"
                  onClick={() => setView("root")}
                >
                  <ArrowLeft className="h-4 w-4" />
                  Back
                </button>
                <div className="text-sm text-muted-foreground">
                  General / HTTP Bindings
                </div>
              </div>

              <div className="space-y-1">
                <h2 className="text-xl font-semibold text-foreground">
                  HTTP Bindings
                </h2>
                <p className="text-sm text-muted-foreground">
                  Choose which interface and port the notifier listens on. Use
                  0.0.0.0 to expose it on your network, or 127.0.0.1 to keep it
                  local-only.
                </p>
              </div>

              <div className="divide-y divide-border overflow-hidden rounded-2xl border border-border bg-card">
                <div className="flex flex-wrap items-center justify-between gap-4 px-4 py-4">
                  <div className="space-y-1">
                    <Label htmlFor="bind-address" className="text-foreground">
                      Bind address
                    </Label>
                    <p className="text-xs text-muted-foreground">
                      Default: 127.0.0.1 (localhost only)
                    </p>
                  </div>
                  <div className="flex min-w-[220px] flex-1 items-center gap-3 sm:max-w-sm">
                    <div className="flex h-9 w-9 items-center justify-center rounded-full bg-muted text-foreground">
                      <Server className="h-4 w-4" />
                    </div>
                    <Input
                      id="bind-address"
                      value={form.bind_address}
                      onChange={(e) =>
                        updateField("bind_address", e.currentTarget.value)
                      }
                      placeholder="0.0.0.0"
                      className="flex-1 bg-muted/40 text-foreground placeholder:text-muted-foreground"
                    />
                  </div>
                </div>

                <div className="flex flex-wrap items-center justify-between gap-4 px-4 py-4">
                  <div className="space-y-1">
                    <Label htmlFor="bind-port" className="text-foreground">
                      Port
                    </Label>
                    <p className="text-xs text-muted-foreground">
                      Any free port between 1 and 65535.
                    </p>
                  </div>
                  <div className="flex min-w-[220px] flex-1 items-center gap-3 sm:max-w-sm">
                    <div className="flex h-9 w-9 items-center justify-center rounded-full bg-muted text-foreground">
                      <Server className="h-4 w-4" />
                    </div>
                    <Input
                      id="bind-port"
                      inputMode="numeric"
                      pattern="[0-9]*"
                      value={form.port}
                      onChange={(e) =>
                        updateField("port", e.currentTarget.value)
                      }
                      placeholder="60766"
                      className="flex-1 bg-muted/40 text-foreground placeholder:text-muted-foreground"
                    />
                  </div>
                </div>
              </div>

              {status && (
                <div
                  className={`rounded-xl border px-4 py-3 text-sm ${
                    status.type === "success"
                      ? "border-ring/60 bg-muted/60 text-foreground"
                      : "border-destructive/60 bg-destructive/10 text-destructive-foreground"
                  }`}
                >
                  {status.message}
                </div>
              )}

              <div className="flex flex-wrap gap-3">
                <Button onClick={saveSettings} disabled={saving || loading}>
                  {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  Save
                </Button>
                <Button
                  variant="secondary"
                  onClick={resetDefaults}
                  disabled={saving || loading}
                >
                  Reset to defaults
                </Button>
                <Button
                  variant="ghost"
                  onClick={loadSettings}
                  disabled={saving}
                >
                  Reload
                </Button>
              </div>
            </div>
          )}
        </div>

        {loading && (
          <div className="mt-4 flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            Loading settings…
          </div>
        )}
      </div>
    </main>
  );
}

export default App;
