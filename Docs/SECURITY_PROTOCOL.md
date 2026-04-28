# Avalon Security Protocol

**Last Updated:** 2026-04-28
**Status:** Active
**Classification:** Public

---

## 1. Threat Model

Avalon is designed to run on a developer's local machine with direct file system and network access. The primary threats are:

- **Unauthorized file access** — an AI tool reads or writes files outside allowed paths
- **SSRF / DNS rebinding** — a malicious URL causes the backend to access internal services
- **Remote code execution** — downloaded content (HTML, PDF, scripts) executes on the host
- **Data exfiltration** — a tool sends local data to an unexpected remote destination
- **Session hijacking** — another process on the machine exploits the local API

---

## 2. Security Principles

1. **Default Deny.** All file and network operations are blocked unless explicitly allowed.
2. **User in the Loop.** Destructive operations require explicit user approval.
3. **Never Execute.** Downloaded content is parsed, not executed. No subprocess spawning for user data.
4. **Local Only.** The backend binds to `127.0.0.1:8080` and does not expose itself to the network.
5. **Layered Defense.** Multiple independent checks must pass for any sensitive operation.

---

## 3. File System Security

### 3.1 Limiter Config (`FileSystemConfig`)
Stored in `.avalon_fs.json`:

```json
{
  "default_policy": "deny",
  "allowed_paths": ["D:/Projects"],
  "denied_paths": ["C:/", "D:/Secrets"],
  "max_file_size": 10485760
}
```

### 3.2 Evaluation Rules
1. **Deny list wins.** If a path matches `denied_paths`, it is always blocked.
2. **Allow list gates.** If `allowed_paths` is non-empty, only matched paths are permitted.
3. **Default policy.** Used when `allowed_paths` is empty.
4. **Size limit.** Files exceeding `max_file_size` are blocked.

### 3.3 Transparency
`.avalon_fs.json` is always readable so the AI can explain current rules to the user.

---

## 4. Network Security

### 4.1 Web Fetch Config (`WebFetchConfig`)
Stored in `.avalon_state.json` under `web_fetch`:

| Field | Default | Purpose |
|-------|---------|---------|
| `max_depth` | 1 | How deep `web_scrape` follows links |
| `confirm_domains` | true | Require explicit approval for unknown domains |
| `allowed_domains` | github.com, raw.githubusercontent.com, gist.github.com, api.github.com | Always permitted |
| `blocked_domains` | [] | Always denied |
| `timeout_secs` | 10 | Request timeout |
| `max_size_mb` | 5 | Max response size |
| `respect_robots_txt` | true | Honor `robots.txt` |
| `rate_limit_ms` | 1000 | Min delay between requests to same domain |

### 4.2 Layered Protections

| Layer | Implementation |
|-------|----------------|
| URL scheme | Block `file://`, `javascript:`, `data:`, `ftp://` |
| SSRF | Block private IPs (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, 169.254.0.0/16, ::1, fc00::/7, fe80::/10) |
| DNS rebind | Resolve hostname, check all returned IPs before request |
| Domain gate | Allow-list + block-list + confirmation dialog |
| Content | Whitelist: `text/*`, `image/*`, `application/pdf`, `application/json`, `application/xml` |
| HTML | Sanitize: remove scripts, styles, iframes, forms, event handlers |
| Size | Enforce `max_size_mb` per response |
| Timeout | Enforce `timeout_secs` per request |
| Rate | Per-domain cooldown (`rate_limit_ms`) |
| Robots | Fetch and respect `robots.txt` |
| No execution | Never run downloaded content; only parse as text/image |

---

## 5. Permission System

### 5.1 Session Permissions
- Tools that write or delete trigger a user approval dialog.
- Approval grants `ReadWrite` for the current session only.
- Revocation removes the grant immediately.
- Core tools (`read_file`, `write_file`, `list_dir`, `delete_file`, `get_fs_config`) are always available but still subject to the File System Limiter.

### 5.2 Permission Flow
1. AI emits a tool call in its response.
2. Backend detects that the tool requires user approval.
3. SSE `permission` event is sent to frontend.
4. Frontend renders dialog with tool description and arguments.
5. User clicks **Approve** or **Deny**.
6. Backend records decision and either executes or rejects.

---

## 6. AI Safety

- **System prompt** explicitly instructs the AI to use tools only when needed and to respect file system limits.
- **Tool descriptions** are dynamically injected so the AI knows exactly what each tool does.
- **No prompt injection protection** is implemented; users are assumed to be the owner of the system.
- **Model output is not sandboxed.** The AI can request any allowed operation. This is by design for a local assistant.

---

## 7. Audit & Logging

Every significant event is logged to the debug log:
- Session start/end
- LLM requests and responses
- Tool calls, results, and errors
- Permission requests, approvals, denials, revocations
- Mind map builds
- File system operations

Logs can be saved to `logs/avalon-debug-{timestamp}.md` for external review.

---

## 8. Reporting Issues

Security concerns should be reported to `legal@imperatormorsus.com`.
