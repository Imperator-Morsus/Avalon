use serde_json;
use crate::tools::{Tool, ToolContext};
use std::net::IpAddr;
use std::str::FromStr;
use base64::Engine;

pub struct FetchUrlTool;

#[async_trait::async_trait]
impl Tool for FetchUrlTool {
    fn name(&self) -> &str {
        "fetch_url"
    }

    fn description(&self) -> &str {
        "Downloads content from a public URL. Supports text, images, and PDFs (text extracted). Respects the Web Fetch config for domains, size limits, and timeouts."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'url' argument")?;

        let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

        // Block dangerous schemes
        let scheme = parsed.scheme().to_lowercase();
        if !matches!(scheme.as_str(), "http" | "https") {
            return Err(format!(
                "URL scheme '{}' is not allowed. Only http and https are permitted.",
                scheme
            ));
        }

        let host = parsed.host_str().unwrap_or("").to_lowercase();

        // Blocked domains
        if ctx.web_fetch.blocked_domains.iter().any(|d| {
            let d = d.to_lowercase();
            host == d || host.ends_with(&format!(".{}", d))
        }) {
            return Err(format!("Domain '{}' is blocked.", host));
        }

        // Domain allow-list / confirmation
        let is_allowed = ctx.web_fetch.allowed_domains.iter().any(|d| {
            let d = d.to_lowercase();
            host == d || host.ends_with(&format!(".{}", d))
        });
        if ctx.web_fetch.confirm_domains && !is_allowed {
            return Err(format!(
                "Domain '{}' is not in the allowed list. Add it to Settings > Web Fetch > Allowed domains to proceed.",
                host
            ));
        }

        // SSRF protection: block private IPs
        if ctx.security.block_private_ips {
            if let Some(ip_str) = parsed.host_str() {
                if let Ok(ip) = IpAddr::from_str(ip_str) {
                    if is_private_ip(ip) {
                        return Err("Private IP addresses are not allowed.".to_string());
                    }
                }
            }
            if let Ok(addrs) = tokio::net::lookup_host(format!("{}:{}", host, parsed.port().unwrap_or(80))).await {
                for addr in addrs {
                    if is_private_ip(addr.ip()) {
                        return Err("Private IP addresses are not allowed.".to_string());
                    }
                }
            }
        }

        let timeout = std::time::Duration::from_secs(ctx.web_fetch.timeout_secs);
        let max_size = (ctx.web_fetch.max_size_mb as usize) * 1024 * 1024;

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client
            .get(url)
            .header("User-Agent", "Avalon-FetchUrl/1.0")
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            ));
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Content-type guard
        let is_allowed_type = content_type.starts_with("text/")
            || content_type.starts_with("image/")
            || content_type.contains("application/pdf")
            || content_type.contains("json")
            || content_type.contains("xml")
            || content_type.is_empty();
        if !is_allowed_type {
            return Err(format!(
                "Content type '{}' is not allowed. Only text, image, and JSON/XML are permitted.",
                content_type
            ));
        }

        if let Some(len) = resp.content_length() {
            if len > max_size as u64 {
                return Err(format!(
                    "File too large: {} bytes (max {} MB)",
                    len, ctx.web_fetch.max_size_mb
                ));
            }
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if bytes.len() > max_size {
            return Err(format!(
                "File too large: {} bytes (max {} MB)",
                bytes.len(),
                ctx.web_fetch.max_size_mb
            ));
        }

        if content_type.starts_with("image/") {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            return Ok(serde_json::json!({
                "url": url,
                "type": "image",
                "mime_type": content_type,
                "size": bytes.len(),
                "base64": b64
            }));
        }

        if content_type.contains("application/pdf") {
            let result = extract_pdf_text(&bytes, url)?;
            // Auto-ingest PDF text into vault
            if let Ok(content) = result.get("content").and_then(|v| v.as_str()).ok_or("") {
                let _ = ctx.vault.lock().unwrap().ingest_text(url, None, content, "pdf");
            }
            return Ok(result);
        }

        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| "Response is not valid UTF-8 text.".to_string())?;

        let text = if content_type.contains("html") && ctx.security.enforce_html_sanitize {
            sanitize_html(&text)
        } else {
            text
        };

        // Auto-ingest fetched text into vault
        let content_type_label = if content_type.contains("html") { "html" } else { "text" };
        let _ = ctx.vault.lock().unwrap().ingest_text(url, None, &text, content_type_label);

        Ok(serde_json::json!({
            "url": url,
            "type": "text",
            "size": bytes.len(),
            "content": text
        }))
    }
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            if octets[0] == 10 {
                return true;
            }
            if octets[0] == 172 && (octets[1] >= 16 && octets[1] <= 31) {
                return true;
            }
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            if octets[0] == 127 {
                return true;
            }
            if octets[0] == 169 && octets[1] == 254 {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            if segments == [0, 0, 0, 0, 0, 0, 0, 1] {
                return true;
            }
            if (segments[0] & 0xFE00) == 0xFC00 {
                return true;
            }
            if (segments[0] & 0xFFC0) == 0xFE80 {
                return true;
            }
            false
        }
    }
}

fn extract_pdf_text(bytes: &[ u8 ], url: &str) -> Result<serde_json::Value, String> {
    let doc = lopdf::Document::load_mem(bytes)
        .map_err(|e| format!("Failed to parse PDF: {}", e))?;
    let pages: Vec<u32> = doc.get_pages().into_keys().collect();
    let text = doc.extract_text(&pages)
        .map_err(|e| format!("Failed to extract PDF text: {}", e))?;

    let trimmed = text.trim().to_string();

    Ok(serde_json::json!({
        "url": url,
        "type": "pdf",
        "mime_type": "application/pdf",
        "size": bytes.len(),
        "content": trimmed
    }))
}

pub fn sanitize_html(html: &str) -> String {
    let mut result = html.to_string();

    let re_script = regex::Regex::new(r"(?i)<script[\s\S]*?</script>").unwrap();
    result = re_script.replace_all(&result, "").to_string();

    let re_style = regex::Regex::new(r"(?i)<style[\s\S]*?</style>").unwrap();
    result = re_style.replace_all(&result, "").to_string();

    let re_iframe = regex::Regex::new(r"(?i)<iframe[\s\S]*?</iframe>").unwrap();
    result = re_iframe.replace_all(&result, "").to_string();

    let re_form = regex::Regex::new(r"(?i)<form[\s\S]*?</form>").unwrap();
    result = re_form.replace_all(&result, "").to_string();

    for tag in [&"nav", &"footer", &"header", &"aside", &"menu", &"noscript",
    ] {
        let pattern = format!(r"(?i)<{}[\s\S]*?</{}>", regex::escape(tag), regex::escape(tag));
        if let Ok(re) = regex::Regex::new(&pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }

    let re_events = regex::Regex::new(r#"(?i)\s+on\w+\s*=\s*(?:\"[^\"]*\"|'[^']*'|[^\s>]+)"#).unwrap();
    result = re_events.replace_all(&result, "").to_string();

    result
}
