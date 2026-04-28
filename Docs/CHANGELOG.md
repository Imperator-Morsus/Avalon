# Changelog

## 0.2.0 — 2026-04-27

### Web Fetch & Universal Scraping

A major expansion of Avalon's network capabilities, moving from hardcoded GitHub-only support to configurable universal web access with layered security.

#### New Backend Infrastructure

- **`WebFetchConfig`** (`src/main.rs`)
  - Persistent config stored in `.avalon_state.json` under the `web_fetch` key.
  - Fields: `max_depth`, `confirm_domains`, `allowed_domains`, `blocked_domains`, `timeout_secs`, `max_size_mb`, `respect_robots_txt`, `rate_limit_ms`.
  - Defaults to safe values: depth=1, confirm_domains=true, timeout=10s, max_size=5MB.

- **New API Endpoints**
  - `GET /api/web/config` — returns current `WebFetchConfig` as JSON.
  - `POST /api/web/config` — accepts a `WebFetchConfig` body and persists it.

- **`ToolContext` Expansion** (`src/tools/mod.rs`)
  - Added `web_fetch: &WebFetchConfig` so all network tools read live config instead of hardcoded constants.

#### fetch_url Tool Overhaul (`src/tools/fetch_tool.rs`)

- **Removed hardcoded GitHub-only restriction.** Now supports any public `http`/`https` URL subject to domain lists and confirmation settings.
- **Image support.** If the response `Content-Type` starts with `image/`, the tool returns a base64-encoded payload instead of text.
- **PDF support.** If the response `Content-Type` is `application/pdf`, the tool parses the PDF with `lopdf`, extracts plain text from all pages, and returns it. No scripts, forms, launch actions, or embedded files survive — only extracted text.
- **HTML sanitization.** Text responses with `text/html` are run through `sanitize_html()`, which strips `<script>`, `<style>`, `<iframe>`, `<form>`, `<nav>`, `<footer>`, `<header>`, `<aside>`, `<menu>`, `<noscript>`, and all inline event handlers (`on*=`).
- **Dynamic limits.** Reads `timeout_secs`, `max_size_mb`, `allowed_domains`, `blocked_domains`, and `confirm_domains` from `WebFetchConfig`.
- **Enhanced security.** Added URL scheme blocking (`file://`, `javascript:`, `data:`, `ftp://`) in addition to existing SSRF private-IP protection.

#### New web_scrape Tool (`src/tools/web_scrape_tool.rs`)

- **Recursive BFS crawler.** Takes a start URL and `max_depth` (overridable via input, defaults to config). Follows links breadth-first up to the configured depth.
- **Same-domain restriction.** Only crawls pages within the same domain as the start URL to prevent accidental internet-wide crawling.
- **robots.txt respect.** Fetches and parses `robots.txt` before crawling a domain. Honors `Disallow` paths.
- **Rate limiting.** Enforces a per-domain cooldown (`rate_limit_ms`) between requests.
- **Content extraction.** For each page:
  - Extracts `<title>`.
  - Sanitizes HTML and converts it to clean plain text.
  - Extracts image `src` URLs (resolved to absolute).
  - Queues discovered links for the next depth level.
- **Returns structured JSON:** `{ pages: [{ url, title, text, images: [] }] }`.
- **Security:** Same layered protections as `fetch_url` — scheme blocking, SSRF private-IP blocking, DNS rebind checks, domain allow/block lists, content-type guards, size limits, timeouts.

#### Frontend Settings UI (`client/ui/app.js`)

- **New "Web Fetch" expandable section** in the Settings panel with:
  - Number input: Max depth (1–10)
  - Number input: Timeout (1–120 sec)
  - Number input: Max size (1–100 MB)
  - Checkbox: Confirm unknown domains
  - Checkbox: Respect robots.txt
  - Editable list: Allowed domains (with Add/Remove)
  - Editable list: Blocked domains (with Add/Remove)
- All changes save immediately via `POST /api/web/config`.

#### Dependencies (`Cargo.toml`)

- Added `base64 = "0.22"` for image base64 encoding.
- Added `lopdf = "0.34"` for PDF text extraction.

#### Direct Fetch API (`/api/fetch`)

- **New endpoint** `POST /api/fetch` bypasses the AI tool-calling gatekeeping while preserving all backend security.
- Runs the identical safe fetch pipeline (URL validation, SSRF blocking, content sanitization, size limits, domain checks, PDF text extraction, image base64, HTML sanitization).
- Returns sanitized content directly to the frontend for user review before it is sent to the model.
- Useful when models refuse to call `fetch_url` due to baked-in safety training (e.g., PDF fetching on some local models).
- **Frontend UI:** New &#x2193; (download) button in the chat input bar opens a prompt for a URL, fetches directly, and displays the sanitized result in the chat history.

#### Documentation (`Docs/CAPABILITIES.md`)

- Updated tool descriptions for `fetch_url` and added `web_scrape`.
- Added the Web Fetch config reference table.
- Added the full security model table.
- Added `GET/POST /api/web/config` to the API endpoints list.
- Added `POST /api/fetch` to the API endpoints list with request/response examples.
- Added new source files to the Files & Config table.
- Added Settings panel section mentioning Web Fetch controls.

### System Prompt Update (`src/main.rs`)

- Updated the tools description injected into the LLM system prompt to describe the new `fetch_url` behavior (any URL, images, PDFs) and the new `web_scrape` recursive scraper.

### Compilation Status

- All modules compile cleanly under `cargo build --release`.
- One benign warning remains: `set_root` in `mindmap.rs` is declared but unused. It is safe to leave.

---

## Prior Versions

For earlier changes (mindmap graph builder, plugin architecture, Electron frontend, security manager, file system limiter, permission system), see the git history.
