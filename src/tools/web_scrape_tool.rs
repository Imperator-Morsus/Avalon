use serde_json;
use crate::tools::{Tool, ToolContext};
use crate::tools::fetch_tool::sanitize_html;
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};
use regex::Regex;

pub struct WebScrapeTool;

#[derive(Debug)]
struct PageData {
    url: String,
    title: String,
    text: String,
    images: Vec<String>,
}

#[async_trait::async_trait]
impl Tool for WebScrapeTool {
    fn name(&self) -> &str {
        "web_scrape"
    }

    fn description(&self) -> &str {
        "Recursively scrapes a website starting from a URL. Extracts text and image references, follows links up to the configured max depth. Respects robots.txt, rate limits, and domain restrictions."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let start_url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'url' argument")?;
        let max_depth = input
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .map(|d| d as u32)
            .unwrap_or(ctx.web_fetch.max_depth);

        let start_parsed =
            reqwest::Url::parse(start_url).map_err(|e| format!("Invalid URL: {}", e))?;
        let scheme = start_parsed.scheme().to_lowercase();
        if !matches!(scheme.as_str(), "http" | "https") {
            return Err("Only http and https URLs are allowed.".to_string());
        }
        let start_domain = start_parsed.host_str().unwrap_or("").to_lowercase();
        if start_domain.is_empty() {
            return Err("URL must have a host.".to_string());
        }

        // Initial security checks on start URL
        check_url_security(&start_parsed,
            &ctx.web_fetch.blocked_domains,
            ctx.web_fetch.confirm_domains,
            &ctx.web_fetch.allowed_domains,
        )?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(ctx.web_fetch.timeout_secs))
            .build()
            .map_err(|e| e.to_string())?;

        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, u32)> = VecDeque::new();
        let mut pages: Vec<PageData> = Vec::new();
        let mut robots_cache: HashMap<String, Vec<String>> = HashMap::new();
        let mut last_request: HashMap<String, Instant> = HashMap::new();

        queue.push_back((start_url.to_string(), 0));

        while let Some((url, depth)) = queue.pop_front() {
            if visited.contains(&url) || depth > max_depth {
                continue;
            }
            visited.insert(url.clone());

            let parsed = match reqwest::Url::parse(&url) {
                Ok(u) => u,
                Err(_) => continue,
            };
            let host = parsed.host_str().unwrap_or("").to_lowercase();
            if host.is_empty() {
                continue;
            }

            // Domain gate
            if ctx.web_fetch.blocked_domains.iter().any(|d| {
                let d = d.to_lowercase();
                host == d || host.ends_with(&format!(".{}", d))
            }) {
                continue;
            }
            if ctx.web_fetch.confirm_domains
                && !ctx.web_fetch.allowed_domains.iter().any(|d| {
                    let d = d.to_lowercase();
                    host == d || host.ends_with(&format!(".{}", d))
                })
            {
                continue;
            }

            // Same-domain restriction (prevent crawling the entire web)
            if !host.ends_with(&start_domain) && start_domain != host {
                continue;
            }

            // SSRF: block private IPs
            if let Some(ip_str) = parsed.host_str() {
                if let Ok(ip) = IpAddr::from_str(ip_str) {
                    if is_private_ip(ip) {
                        continue;
                    }
                }
            }
            if let Ok(mut addrs) =
                tokio::net::lookup_host(format!("{}:{}", host, parsed.port().unwrap_or(80))).await
            {
                if addrs.any(|addr| is_private_ip(addr.ip())) {
                    continue;
                }
            }

            // robots.txt
            if ctx.web_fetch.respect_robots_txt {
                if !robots_cache.contains_key(&host) {
                    let robots_url = format!("{}://{}/robots.txt", parsed.scheme(), host);
                    let paths = match fetch_robots_txt(&client, &robots_url).await {
                        Ok(p) => p,
                        Err(_) => Vec::new(),
                    };
                    robots_cache.insert(host.clone(), paths);
                }
                if let Some(disallowed) = robots_cache.get(&host) {
                    let path = parsed.path();
                    if disallowed.iter().any(|p| path.starts_with(p)) {
                        continue;
                    }
                }
            }

            // Rate limit
            if let Some(last) = last_request.get(&host) {
                let elapsed = last.elapsed();
                let required = Duration::from_millis(ctx.web_fetch.rate_limit_ms);
                if elapsed < required {
                    tokio::time::sleep(required - elapsed).await;
                }
            }

            // Fetch
            let resp = match client
                .get(&url)
                .header("User-Agent", "Avalon-WebScrape/1.0")
                .send()
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };
            last_request.insert(host, Instant::now());

            if !resp.status().is_success() {
                continue;
            }

            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();

            // Only process text-based responses
            if !content_type.starts_with("text/") && !content_type.is_empty() {
                continue;
            }

            let max_size = (ctx.web_fetch.max_size_mb as usize) * 1024 * 1024;
            if let Some(len) = resp.content_length() {
                if len > max_size as u64 {
                    continue;
                }
            }

            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            if bytes.len() > max_size {
                continue;
            }

            let html = match String::from_utf8(bytes.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let title = extract_title(&html);
            let text = html_to_text(&html);
            let images = extract_images(&html, &parsed);

            pages.push(PageData {
                url: url.clone(),
                title: title.clone(),
                text: text.clone(),
                images,
            });

            // Auto-ingest scraped page text into vault
            let _ = ctx
                .vault
                .lock()
                .unwrap()
                .ingest_text(&url, Some(&title), &text, "text", "Public", None);

            if depth < max_depth {
                let links = extract_links(&html, &parsed);
                for link in links {
                    if !visited.contains(&link) {
                        queue.push_back((link, depth + 1));
                    }
                }
            }
        }

        let result: Vec<serde_json::Value> = pages
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "url": p.url,
                    "title": p.title,
                    "text": p.text,
                    "images": p.images
                })
            })
            .collect();

        Ok(serde_json::json!({ "pages": result }))
    }
}

fn check_url_security(
    parsed: &reqwest::Url,
    blocked_domains: &[ String ],
    confirm_domains: bool,
    allowed_domains: &[ String ],
) -> Result<(), String> {
    let scheme = parsed.scheme().to_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err(format!(
            "URL scheme '{}' is not allowed. Only http and https are permitted.",
            scheme
        ));
    }

    let host = parsed.host_str().unwrap_or("").to_lowercase();
    if blocked_domains.iter().any(|d| {
        let d = d.to_lowercase();
        host == d || host.ends_with(&format!(".{}", d))
    }) {
        return Err(format!("Domain '{}' is blocked.", host));
    }
    if confirm_domains
        && !allowed_domains.iter().any(|d| {
            let d = d.to_lowercase();
            host == d || host.ends_with(&format!(".{}", d))
        })
    {
        return Err(format!(
            "Domain '{}' is not in the allowed list. Add it to Settings > Web Fetch > Allowed domains to proceed.",
            host
        ));
    }

    if let Some(ip_str) = parsed.host_str() {
        if let Ok(ip) = IpAddr::from_str(ip_str) {
            if is_private_ip(ip) {
                return Err("Private IP addresses are not allowed.".to_string());
            }
        }
    }

    Ok(())
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

async fn fetch_robots_txt(client: &reqwest::Client, url: &str) -> Result<Vec<String>, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "Avalon-WebScrape/1.0")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let mut disallowed = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.to_lowercase().starts_with("disallow:") {
            let path = line["disallow:".len()..].trim();
            if !path.is_empty() {
                disallowed.push(path.to_string());
            }
        }
    }
    Ok(disallowed)
}

fn extract_title(html: &str) -> String {
    let re = Regex::new(r"(?i)<title>(.*?)</title>").unwrap();
    re.captures(html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default()
}

fn html_to_text(html: &str) -> String {
    let mut text = sanitize_html(html);

    let re_br = Regex::new(r"(?i)<br\s*/?>").unwrap();
    text = re_br.replace_all(&text, "\n").to_string();

    let re_p = Regex::new(r"(?i)</p>\s*<p").unwrap();
    text = re_p.replace_all(&text, "\n\n<").to_string();

    let re_tags = Regex::new(r"<[^>]+>").unwrap();
    text = re_tags.replace_all(&text, "").to_string();

    let re_ws = Regex::new(r"[ \t]+").unwrap();
    text = re_ws.replace_all(&text, " ").to_string();

    let re_lines = Regex::new(r"\n{3,}").unwrap();
    text = re_lines.replace_all(&text, "\n\n").to_string();

    text.trim().to_string()
}

fn extract_images(html: &str, base: &reqwest::Url) -> Vec<String> {
    let re = Regex::new(r#"(?i)<img[^>]+src\s*=\s*["']([^"']+)["']"#).unwrap();
    re.captures_iter(html)
        .filter_map(|cap| cap.get(1))
        .map(|m| resolve_url(base, m.as_str()))
        .collect()
}

fn extract_links(html: &str, base: &reqwest::Url) -> Vec<String> {
    let re = Regex::new(r#"(?i)<a[^>]+href\s*=\s*["']([^"']+)["']"#).unwrap();
    re.captures_iter(html)
        .filter_map(|cap| cap.get(1))
        .map(|m| resolve_url(base, m.as_str()))
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .collect()
}

fn resolve_url(base: &reqwest::Url, href: &str) -> String {
    base.join(href)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| href.to_string())
}
