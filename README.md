<center>
<img src="src-tauri/icons/agent-notifier-tray-icon.png" alt="Agent Notifications Icon" width="256" height="256"/>
</center>

# Agent Notifications

A tiny application that displays desktop notifications for any agent running locally or remotely, once they complete their work.

Download the latest release from the [Releases](https://github.com/davidkelley/agent-notifier/releases) page for your OS.

- **MCP Server**: Add Agent Notifier as an MCP server in your agent framework of choice.
- **Agent Skill 💥**: Optionally, add Agent Notifier as a skill to your agents to get notifications for specific tasks.

## MCP Server

To use Agent Notifier, you need to add it as an MCP server in your agent framework of choice.

### Claude Code

```
claude mcp add --transport http agent-notifier http://localhost:60766/mcp
```

### OpenCode

```json
{
  "mcp": {
    "agent-notifier": {
      "type": "remote",
      "url": "http://localhost:60766/mcp",
      "enabled": true
    }
  }
}
```

### Cursor

```json
{
  "mcpServers": {
    "agent-notifier": {
      "url": "http://localhost:60766/mcp"
    }
  }
}
```

### Codex

```toml
[mcp_servers.agent-notifier]
url = "http://localhost:60766/mcp"
```

### VS Code

```json
"mcp": {
  "servers": {
    "agent-notifier": {
      "type": "http",
      "url": "http://localhost:60766/mcp"
    }
  }
}
```

### Windsurf

```json
{
  "mcpServers": {
    "agent-notifier": {
      "serverUrl": "http://localhost:60766/mcp"
    }
  }
}
```

## Agent Skills

You can also add Agent Notifier as a skill to your agents using the following command:

```
npx skills add https://github.com/davidkelley/agent-notifier --skill notify-on-completion
```
