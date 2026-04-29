use serde_json;
use serde_json::json;
use crate::mindmap::MindMapService;
use crate::tools::{Tool, ToolContext};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub struct RemoteMindMapTool;

const MAX_DOWNLOAD_MB: usize = 25;
const TIMEOUT_SECS: u64 = 30;

#[async_trait::async_trait]
impl Tool for RemoteMindMapTool {
    fn name(&self) -> &str {
        "remote_mindmap"
    }

    fn description(&self) -> &str {
        "Downloads a public GitHub repository, builds a mind map from it, and stores it in a quarantined remote graph. The user must review and approve it before it merges into the permanent local mind map. Only github.com repos are supported."
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
            .ok_or("Missing 'url' argument. Provide a GitHub repo URL like https://github.com/user/repo")?;

        let (owner, repo, branch) = parse_github_url(url)?;
        let prefix = format!("github:{}/{}/", owner, repo);

        // Create isolated temp directory
        let temp_dir = std::env::temp_dir()
            .join(format!("avalon_remote_repo_{}_{}_{}", owner, repo, std::process::id()));

        // Ensure cleanup happens even on error
        let _cleanup = TempDirCleanup {
            path: temp_dir.clone(),
        };

        // Download zip
        let zip_url = format!(
            "https://github.com/{}/{}/archive/refs/heads/{}.zip",
            owner, repo, branch
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client
            .get(&zip_url)
            .header("User-Agent", "Avalon-RemoteMindMap/1.0")
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!(
                "GitHub returned HTTP {}. The repo may not exist or the branch '{}' may not be found.",
                status.as_u16(),
                branch
            ));
        }

        // Check content length
        if let Some(len) = resp.content_length() {
            if len > (MAX_DOWNLOAD_MB * 1024 * 1024) as u64 {
                return Err(format!(
                    "Repo archive too large: {} MB (max {} MB)",
                    len / (1024 * 1024),
                    MAX_DOWNLOAD_MB
                ));
            }
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read archive: {}", e))?;

        if bytes.len() > MAX_DOWNLOAD_MB * 1024 * 1024 {
            return Err(format!(
                "Repo archive too large: {} MB (max {} MB)",
                bytes.len() / (1024 * 1024),
                MAX_DOWNLOAD_MB
            ));
        }

        // Save zip to temp
        let zip_path = temp_dir.join("repo.zip");
        std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
        {
            let mut file = File::create(&zip_path).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
        }

        // Extract zip
        extract_zip(&zip_path, &temp_dir)?;

        // Find the extracted directory (it will be named owner-repo-branch)
        let extracted_dir = find_extracted_dir(&temp_dir)?;

        // Build remote mindmap with prefix
        let mut remote_mm = MindMapService::new();
        remote_mm.build_with_prefix(
            &[extracted_dir.to_string_lossy().to_string()],
            3,
            Some(&prefix),
        );

        // Store in quarantine (remote graph) — user must approve before merge
        let remote_graph = remote_mm.graph().clone();
        ctx.mindmap.lock().unwrap().set_remote_graph(remote_graph.clone());

        Ok(serde_json::to_value(json!({
            "stored": true,
            "quarantined": true,
            "source": url,
            "nodes": remote_graph.nodes.len(),
            "edges": remote_graph.edges.len(),
            "message": "Remote mindmap stored in quarantine. User must review and approve before it merges into the permanent local mindmap."
        })).map_err(|e| e.to_string())?)
    }
}

fn parse_github_url(url: &str) -> Result<(String, String, String), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    let host = parsed.host_str().unwrap_or("").to_lowercase();
    if host != "github.com" && !host.ends_with(".github.com") {
        return Err(format!(
            "Only github.com URLs are supported. Got: {}",
            host
        ));
    }

    let path_segments: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.collect())
        .unwrap_or_default();

    if path_segments.len() < 2 {
        return Err(
            "Invalid GitHub URL. Expected format: https://github.com/owner/repo or https://github.com/owner/repo/tree/branch".to_string());
    }

    let owner = path_segments[0].to_string();
    let repo = path_segments[1].to_string();

    let branch = if path_segments.len() >= 4 && path_segments[2] == "tree" {
        path_segments[3].to_string()
    } else {
        "main".to_string()
    };

    Ok((owner, repo, branch))
}

fn extract_zip(zip_path: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
            }
            let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn find_extracted_dir(temp_dir: &std::path::Path) -> Result<PathBuf, String> {
    let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(temp_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.file_name() != "repo.zip")
        .collect();

    if entries.is_empty() {
        return Err("Could not find extracted repo directory".to_string());
    }

    Ok(entries[0].path())
}

struct TempDirCleanup {
    path: PathBuf,
}

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
