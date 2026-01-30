---
name: notify-on-permission-request
description: Ask the user for approval before running a risky command by sending a desktop notification via the Agent Notifications MCP tool `notify_permission_request`; include command, reason, agent, risk level, and optional timeout or context URL.
---

# Notify on Permission Request

Use when an agent needs explicit user consent (e.g., modifying files, running scripts, network calls, package installs, deployments).

## MCP tool

- Tool name: `notify_permission_request`
- Arguments (all trimmed):
  - `command` (string, required): the command or action being requested.
  - `reason` (string, required): why this is needed.
  - `agent` (string, required): short id of the asking agent/workflow.
  - `risk` ("low" | "medium" | "high", required): risk level.
  - `timeoutSeconds` (int, optional): seconds to wait before timing out/auto-deny.
  - `contextUrl` (string, optional): link to diff/log/ticket for review.

## Message rules

1. Be explicit that approval is needed; the app shows title `Permission needed` and body lines for command, reason, risk, optional timeout/context.
2. Keep it short; stay under the existing ~950 character soft limit.
3. Include the most specific command form (with flags/paths) so the user understands the exact action.
4. Risk should reflect impact scope (file writes, network, privilege escalation, data exfil, deploys).
5. If timeout matters, set `timeoutSeconds`; otherwise omit.

## Example

```json
{
  "name": "notify_permission_request",
  "arguments": {
    "command": "npm install && npm test",
    "reason": "Need dependencies to run the test suite",
    "agent": "codex",
    "risk": "medium",
    "timeoutSeconds": 600,
    "contextUrl": "http://localhost:4173/logs/test-run"
  }
}
```

## When to skip

- If the platform already enforced the permission and user confirmed.
- For read-only or obviously safe actions (e.g., `ls`, `cat`).

## Tips

- Pair with your normal approval loop (chat prompt, CLI confirm); this notification is a heads-up, not an implicit yes.
- Use `agent` consistently so users can filter/recognize the requester.
