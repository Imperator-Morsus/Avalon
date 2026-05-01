use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ═════════════════════════════════════════════════════════════════════════════
// Audit Log — Court-Admissible Cryptographic Logging
// ═════════════════════════════════════════════════════════════════════════════

/// Single audit entry with hash chain linkage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub session_id: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub prev_hash: String,
    pub entry_hash: String,
    pub entry_type: String,
    pub actor: String, // user | assistant | system
    pub data: serde_json::Value,
}

/// Per-session manifest written when a session ends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub start_time: chrono::DateTime<chrono::Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub entry_count: u64,
    pub merkle_root: String,
    pub closing_hash: String,
}

/// Archive manifest for warm/cold tiers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub archive_name: String,
    pub sha256: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub signed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct AuditLog {
    session_id: String,
    seq: u64,
    last_hash: String,
    entries: Vec<AuditEntry>,
    hot_dir: PathBuf,
    warm_dir: PathBuf,
    cold_dir: PathBuf,
    debug_dir: PathBuf,
    max_mem_entries: usize,
}

impl AuditLog {
    pub fn new() -> Self {
        let project_dir = std::env::current_exe()
            .ok()
            .and_then(|mut p| {
                p.pop(); p.pop(); p.pop();
                Some(p)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let hot_dir = project_dir.join("logs").join("audit").join("active");
        let warm_dir = project_dir.join("logs").join("audit").join("warm");
        let cold_dir = project_dir.join("archive").join("audit");
        let debug_dir = project_dir.join("logs").join("debug");

        let _ = fs::create_dir_all(&hot_dir);
        let _ = fs::create_dir_all(&warm_dir);
        let _ = fs::create_dir_all(&cold_dir);
        let _ = fs::create_dir_all(&debug_dir);

        let session_id = format!("sess-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs());

        AuditLog {
            session_id,
            seq: 0,
            last_hash: "0".repeat(64),
            entries: Vec::new(),
            hot_dir,
            warm_dir,
            cold_dir,
            debug_dir,
            max_mem_entries: 5000,
        }
    }

    /// Append an entry. Computes hash chain and writes to hot storage immediately.
    pub fn push(&mut self, entry_type: &str, data: serde_json::Value) {
        self.push_with_actor(entry_type, "system", data);
    }

    pub fn push_user(&mut self, entry_type: &str, data: serde_json::Value) {
        self.push_with_actor(entry_type, "user", data);
    }

    pub fn push_assistant(&mut self, entry_type: &str, data: serde_json::Value) {
        self.push_with_actor(entry_type, "assistant", data);
    }

    pub(crate) fn push_with_actor(&mut self, entry_type: &str, actor: &str, data: serde_json::Value) {
        self.seq += 1;
        let timestamp = chrono::Utc::now();

        let entry_hash = Self::compute_hash(&self.last_hash, self.seq, &self.session_id, &timestamp, entry_type, actor, &data);

        let entry = AuditEntry {
            seq: self.seq,
            session_id: self.session_id.clone(),
            timestamp,
            prev_hash: self.last_hash.clone(),
            entry_hash: entry_hash.clone(),
            entry_type: entry_type.to_string(),
            actor: actor.to_string(),
            data,
        };

        self.last_hash = entry_hash;
        self.entries.push(entry.clone());

        // Trim in-memory buffer
        if self.entries.len() > self.max_mem_entries {
            let excess = self.entries.len() - self.max_mem_entries;
            self.entries.drain(0..excess);
        }

        // Write to hot storage immediately (append-only)
        let _ = self.write_hot_entry(&entry);
    }

    fn compute_hash(prev: &str, seq: u64, session: &str, ts: &chrono::DateTime<chrono::Utc>, entry_type: &str, actor: &str, data: &serde_json::Value) -> String {
        let payload = format!(
            "{}|{}|{}|{}|{}|{}",
            prev,
            seq,
            session,
            ts.timestamp_millis(),
            entry_type,
            actor
        );
        let data_str = serde_json::to_string(data).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        hasher.update(data_str.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn write_hot_entry(&self, entry: &AuditEntry) -> io::Result<()> {
        let path = self.hot_dir.join(format!("{}.ndjson", self.session_id));
        let line = serde_json::to_string(entry)?;
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", line)?;
        file.sync_all()?;
        Ok(())
    }

    /// End the session, write manifest, and seal the hot file (read-only)
    pub fn end_session(&mut self) {
        let merkle_root = self.compute_merkle_root();
        let manifest = SessionManifest {
            session_id: self.session_id.clone(),
            start_time: self.entries.first().map(|e| e.timestamp).unwrap_or_else(chrono::Utc::now),
            end_time: chrono::Utc::now(),
            entry_count: self.seq,
            merkle_root,
            closing_hash: self.last_hash.clone(),
        };

        let manifest_path = self.hot_dir.join(format!("{}.manifest.json", self.session_id));
        if let Ok(json) = serde_json::to_string_pretty(&manifest) {
            let _ = fs::write(&manifest_path, json);
        }

        // WORM: set read-only on the ndjson file
        let _hot_path = self.hot_dir.join(format!("{}.ndjson", self.session_id));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&_hot_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o444);
                let _ = fs::set_permissions(&_hot_path, perms);
            }
        }
        #[cfg(windows)]
        {
            // On Windows we can't easily set immutable; rely on append-only design
            // and manifest verification instead.
        }
    }

    fn compute_merkle_root(&self) -> String {
        let mut hashes: Vec<String> = self.entries.iter().map(|e| e.entry_hash.clone()).collect();
        if hashes.is_empty() {
            return "0".repeat(64);
        }
        while hashes.len() > 1 {
            let mut next = Vec::new();
            for pair in hashes.chunks(2) {
                let mut hasher = Sha256::new();
                hasher.update(pair[0].as_bytes());
                if pair.len() > 1 {
                    hasher.update(pair[1].as_bytes());
                }
                next.push(hex::encode(hasher.finalize()));
            }
            hashes = next;
        }
        hashes.into_iter().next().unwrap()
    }

    /// Verify a session's hash chain from hot storage.
    pub fn verify_session(&self, session_id: &str) -> Result<VerificationReport, String> {
        let path = self.hot_dir.join(format!("{}.ndjson", session_id));
        let file = fs::File::open(&path).map_err(|e| format!("Cannot open session file: {}", e))?;
        let reader = io::BufReader::new(file);

        let mut prev_hash = "0".repeat(64);
        let mut entry_count = 0u64;
        let mut broken_at: Option<u64> = None;

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Read error: {}", e))?;
            if line.trim().is_empty() { continue; }
            let entry: AuditEntry = serde_json::from_str(&line)
                .map_err(|e| format!("JSON parse error: {}", e))?;

            if entry.prev_hash != prev_hash {
                broken_at = Some(entry.seq);
                break;
            }

            let recomputed = Self::compute_hash(
                &entry.prev_hash, entry.seq, &entry.session_id,
                &entry.timestamp, &entry.entry_type, &entry.actor, &entry.data
            );
            if recomputed != entry.entry_hash {
                broken_at = Some(entry.seq);
                break;
            }

            prev_hash = entry.entry_hash.clone();
            entry_count = entry.seq;
        }

        let manifest_path = self.hot_dir.join(format!("{}.manifest.json", session_id));
        let manifest: Option<SessionManifest> = fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());

        Ok(VerificationReport {
            session_id: session_id.to_string(),
            entry_count,
            broken_at,
            manifest_valid: manifest.as_ref().map(|m| m.closing_hash == prev_hash).unwrap_or(false),
            manifest,
        })
    }

    /// Export a chain-of-custody Markdown report for legal proceedings.
    pub fn export_chain_of_custody(&self, session_id: &str) -> Result<String, String> {
        let report = self.verify_session(session_id)?;
        let path = self.hot_dir.join(format!("{}.ndjson", session_id));
        let file = fs::File::open(&path).map_err(|e| format!("Cannot open: {}", e))?;
        let reader = io::BufReader::new(file);

        let mut md = String::new();
        md.push_str(&format!("# Chain of Custody Report\n\n"));
        md.push_str(&format!("**Session ID:** `{}`\n\n", session_id));
        md.push_str(&format!("**Generated:** {}\n\n", chrono::Utc::now().to_rfc3339()));
        md.push_str(&format!("**Entry Count:** {}\n\n", report.entry_count));
        md.push_str(&format!("**Hash Chain Integrity:** {}\n\n", if report.broken_at.is_none() { "VERIFIED" } else { "BROKEN" }));
        if let Some(seq) = report.broken_at {
            md.push_str(&format!("**Broken At Entry:** {}\n\n", seq));
        }
        if report.manifest_valid {
            md.push_str("**Session Manifest:** VERIFIED\n\n");
        } else {
            md.push_str("**Session Manifest:** MISSING OR MISMATCHED\n\n");
        }
        md.push_str("---\n\n");

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Read error: {}", e))?;
            if line.trim().is_empty() { continue; }
            let entry: AuditEntry = serde_json::from_str(&line)
                .map_err(|e| format!("Parse error: {}", e))?;

            md.push_str(&format!("## Entry {} — `{}`\n\n", entry.seq, entry.entry_type));
            md.push_str(&format!("- **Timestamp:** {}\n", entry.timestamp.to_rfc3339()));
            md.push_str(&format!("- **Actor:** {}\n", entry.actor));
            md.push_str(&format!("- **Previous Hash:** `{}`\n", entry.prev_hash));
            md.push_str(&format!("- **Entry Hash:** `{}`\n", entry.entry_hash));
            md.push_str(&format!("- **Data:**\n```json\n{}\n```\n\n",
                serde_json::to_string_pretty(&entry.data).unwrap_or_default()
            ));
        }

        md.push_str("---\n\n");
        md.push_str("## Verification Steps\n\n");
        md.push_str("1. Each entry's `entry_hash` is SHA-256 of (`prev_hash` + `seq` + `session_id` + `timestamp_ms` + `entry_type` + `actor` + serialized `data`).\n");
        md.push_str("2. Entry N's `prev_hash` must equal Entry N-1's `entry_hash`.\n");
        md.push_str("3. The session manifest's `closing_hash` must equal the final entry's `entry_hash`.\n");
        md.push_str("4. The Merkle root in the manifest is the recursive pairwise hash of all entry hashes.\n");
        md.push_str("\nTo re-verify programmatically, use `/api/audit/verify/{session_id}`.\n");

        let out_path = self.hot_dir.join(format!("{}_report.md", session_id));
        fs::write(&out_path, &md)
            .map_err(|e| format!("Cannot write report: {}", e))?;
        Ok(out_path.to_string_lossy().to_string())
    }

    /// Archive old sessions into warm tier (daily tar.gz)
    pub fn archive_daily(&self, date: &str, warm_enabled: bool) -> Result<(), String> {
        if !warm_enabled {
            return Ok(());
        }

        let day_prefix = format!("{}", date);
        let entries: Vec<_> = fs::read_dir(&self.hot_dir)
            .map_err(|e| format!("Cannot read hot dir: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("sess-") && !name.ends_with(".manifest.json")
            })
            .collect();

        if entries.is_empty() {
            return Ok(());
        }

        let tar_path = self.warm_dir.join(format!("{}.tar.gz", day_prefix));
        let tar_gz = fs::File::create(&tar_path).map_err(|e| format!("Create tar: {}", e))?;
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        for entry in entries {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default();
            tar.append_path_with_name(&path, name).map_err(|e| format!("Append tar: {}", e))?;
        }

        tar.finish().map_err(|e| format!("Finish tar: {}", e))?;

        // Write archive manifest
        let hash = Self::file_sha256(&tar_path).map_err(|e| format!("Hash archive: {}", e))?;
        let manifest = ArchiveManifest {
            archive_name: tar_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            sha256: hash,
            signed_at: chrono::Utc::now(),
        };
        let manifest_path = self.warm_dir.join(format!("{}.manifest.json", day_prefix));
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap_or_default())
            .map_err(|e| format!("Write manifest: {}", e))?;

        // WORM: set read-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&tar_path).map_err(|e| e.to_string())?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o444);
            fs::set_permissions(&tar_path, perms).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn file_sha256(path: &Path) -> Result<String, io::Error> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        io::copy(&mut file, &mut hasher)?;
        Ok(hex::encode(hasher.finalize()))
    }

    /// List all sessions in hot storage
    pub fn list_sessions(&self) -> Vec<String> {
        fs::read_dir(&self.hot_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".ndjson") {
                    Some(name.trim_end_matches(".ndjson").to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Save debug-style Markdown to the manual debug directory (logs/debug/)
    pub fn save_to_file(&self) -> Result<String, String> {
        let out_path = self.debug_dir.join(format!("{}_debug.md", self.session_id));
        let report = self.verify_session(&self.session_id)?;
        let path = self.hot_dir.join(format!("{}.ndjson", self.session_id));
        let file = fs::File::open(&path).map_err(|e| format!("Cannot open: {}", e))?;
        let reader = io::BufReader::new(file);

        let mut md = String::new();
        md.push_str(&format!("# Debug Report\n\n"));
        md.push_str(&format!("**Session ID:** `{}`\n\n", self.session_id));
        md.push_str(&format!("**Generated:** {}\n\n", chrono::Utc::now().to_rfc3339()));
        md.push_str(&format!("**Entry Count:** {}\n\n", report.entry_count));
        md.push_str(&format!("**Hash Chain Integrity:** {}\n\n", if report.broken_at.is_none() { "VERIFIED" } else { "BROKEN" }));
        if let Some(seq) = report.broken_at {
            md.push_str(&format!("**Broken At Entry:** {}\n\n", seq));
        }
        md.push_str("---\n\n");

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Read error: {}", e))?;
            if line.trim().is_empty() { continue; }
            let entry: AuditEntry = serde_json::from_str(&line)
                .map_err(|e| format!("Parse error: {}", e))?;
            md.push_str(&format!("## Entry {} — `{}`\n\n", entry.seq, entry.entry_type));
            md.push_str(&format!("- **Timestamp:** {}\n", entry.timestamp.to_rfc3339()));
            md.push_str(&format!("- **Actor:** {}\n", entry.actor));
            md.push_str(&format!("- **Data:**\n```json\n{}\n```\n\n",
                serde_json::to_string_pretty(&entry.data).unwrap_or_default()
            ));
        }

        fs::write(&out_path, &md)
            .map_err(|e| format!("Cannot write report: {}", e))?;
        Ok(out_path.to_string_lossy().to_string())
    }

    /// Backward-compatible: clear in-memory buffer
    pub fn clear(&mut self) {
        self.entries.clear();
        self.seq = 0;
        self.last_hash = "0".repeat(64);
    }

    /// Backward-compatible: get all in-memory entries
    pub fn get_all(&self) -> &Vec<AuditEntry> {
        &self.entries
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn debug_dir(&self) -> &PathBuf {
        &self.debug_dir
    }
}

/// Result of verifying a session
#[derive(Debug, Clone, Serialize)]
pub struct VerificationReport {
    pub session_id: String,
    pub entry_count: u64,
    pub broken_at: Option<u64>,
    pub manifest_valid: bool,
    pub manifest: Option<SessionManifest>,
}
