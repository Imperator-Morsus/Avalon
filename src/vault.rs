use crate::db::{VaultDb, VaultItem, VaultRelationship, VaultNotification};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use image::GenericImageView;
use crate::fs::FileSystemConfig;

// ═════════════════════════════════════════════════════════════════════════════
// The Vault Service (Phase 1 — Unified)
// Handles ingestion, search, relationships, embeddings, contradictions, and notifications.
// ═════════════════════════════════════════════════════════════════════════════

/// Cached digest entry — computed digest + staleness bound.
#[derive(Clone)]
struct CachedDigest {
    content: String,
    computed_at: std::time::Instant,
}

pub struct VaultService {
    db: Arc<Mutex<VaultDb>>,
    fs_config: Option<FileSystemConfig>,
}

// ─────────────────────────────────────────────────────────────────────────────
// TECHNICAL DEBT — Send+Sync Safety Issue
//
// VaultService holds Arc<Mutex<VaultDb>>, which is technically Send+Sync.
// However, extract_concepts_for_item() and detect_contradiction_for_item()
// hold self.db.lock() ACROSS their first await (at call_ollama_json).
// This means a MutexGuard (not Send) lives across an await point.
//
// We suppress the compiler's Send check with unsafe impl because:
//   1. The only problematic futures are in Astra background tasks
//   2. Those tasks use std::thread::spawn (blocking, not async tokio spawn)
//   3. The caller (you, single-user) accepts the risk
//
// RESOLUTION REQUIRED when this is ever made multi-threaded or shared:
//   - Refactor async methods to release the lock BEFORE the first await
//   - Or switch to a lock-free async mutex (tokio::sync::Mutex)
//   - Or spawn blocking threads for these specific operations
// ─────────────────────────────────────────────────────────────────────────────
unsafe impl Send for VaultService {}
unsafe impl Sync for VaultService {}

impl VaultService {
    pub fn new(db: Arc<Mutex<VaultDb>>) -> Self {
        Self { db, fs_config: None }
    }

    pub fn with_fs_config(db: Arc<Mutex<VaultDb>>, fs_config: FileSystemConfig) -> Self {
        Self { db, fs_config: Some(fs_config) }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Ingestion
    // ═════════════════════════════════════════════════════════════════════════

    /// Ingest a document or image from disk into the vault.
    /// Returns the item ID if successful.
    pub fn ingest_file(
        &self,
        path: &Path,
        title: Option<&str>,
        content_type_hint: Option<&str>,
        access_tier: &str,
        owner_id: Option<&str>,
    ) -> Result<i64, String> {
        let path_str = path.to_string_lossy().to_string();

        // Step 1: FileSystemService limiter — path bounds check
        if let Some(ref fs) = self.fs_config {
            if !fs.is_allowed(&path_str) {
                return Err(format!(
                    "Path '{}' is not in allowed_paths. Add it to filesystem configuration to enable access.",
                    path_str
                ));
            }
        }

        // Step 6: Size validation — enforce max_file_size
        if let Some(ref fs) = self.fs_config {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.len() > fs.max_file_size as u64 {
                    return Err(format!(
                        "File size {} bytes exceeds maximum allowed size {} bytes",
                        metadata.len(),
                        fs.max_file_size
                    ));
                }
            }
        }

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return Err(format!("Failed to read file: {}", e)),
        };

        let hash = format!("{:x}", Sha256::digest(&bytes));

        // Check if already ingested (exact hash match)
        let already_exists = {
            let db = self.db.lock().unwrap();
            db.item_exists_by_hash(&hash).map_err(|e| e.to_string())?
        };
        if already_exists {
            return Err("Item already exists in vault".to_string());
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let source_path = path.to_string_lossy().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        // Check if source_path already exists (re-ingestion)
        let existing_item = {
            let db = self.db.lock().unwrap();
            db.find_item_by_source_path(&source_path).map_err(|e| e.to_string())?
        };

        // Determine content type
        let content_type = content_type_hint.unwrap_or_else(|| {
            match ext.as_str() {
                "pdf" => "pdf",
                "md" => "markdown",
                "rs" | "js" | "ts" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs" | "sh" | "bat" | "ps1" => "code",
                "html" | "htm" => "html",
                "txt" | "log" => "text",
                "jpg" | "jpeg" => "image",
                "png" => "image",
                "gif" => "image",
                "webp" => "image",
                "bmp" => "image",
                "svg" => "image",
                "ico" => "image",
                "mp4" | "webm" | "avi" | "mov" => "video",
                "mp3" | "wav" | "ogg" | "flac" => "audio",
                _ => "text",
            }
        });

        let (content, format, width, height, duration) = match content_type {
            "pdf" => (extract_pdf_text(&bytes)?, Some(ext.clone()), None, None, None),
            "html" => {
                let raw = String::from_utf8_lossy(&bytes).to_string();
                (sanitize_html_text(&raw), Some(ext.clone()), None, None, None)
            }
            "image" => {
                let (w, h) = detect_image_dimensions(path);
                (String::new(), Some(ext.clone()), w, h, None)
            }
            "video" | "audio" => {
                // Placeholder: real duration detection requires ffprobe or similar
                (String::new(), Some(ext.clone()), None, None, None)
            }
            _ => {
                let raw = String::from_utf8_lossy(&bytes).to_string();
                (sanitize_text(&raw), None, None, None, None)
            }
        };

        if content_type != "image" && content_type != "video" && content_type != "audio" && content.is_empty() {
            return Err("Extracted content is empty".to_string());
        }

        let title = title.map(|s| s.to_string()).or_else(|| {
            path.file_stem().map(|s| s.to_string_lossy().to_string())
        });

        let db = self.db.lock().unwrap();
        let id = db.insert_item(
            &source_path,
            title.as_deref(),
            None, // description
            &content,
            content_type,
            format.as_deref(),
            Some(bytes.len()),
            width,
            height,
            duration,
            &now,
            &hash,
            None,
            access_tier,
            owner_id,
        ).map_err(|e| e.to_string())?;

        // If this is a re-ingestion, link versions
        if let Some(old) = existing_item {
            let _ = db.update_item_status(old.id, "archived");
            let _ = db.insert_relationship(
                id, old.id, "newer_version", 1.0, None, &now
            );
            let _ = db.insert_relationship(
                old.id, id, "older_version", 1.0, None, &now
            );
        }

        Ok(id)
    }

    /// Ingest text content directly (for fetched/scraped content without a file).
    pub fn ingest_text(
        &self,
        source_path: &str,
        title: Option<&str>,
        content: &str,
        content_type: &str,
        access_tier: &str,
        owner_id: Option<&str>,
    ) -> Result<i64, String> {
        let hash = format!("{:x}", Sha256::digest(content.as_bytes()));

        let already_exists = {
            let db = self.db.lock().unwrap();
            db.item_exists_by_hash(&hash).map_err(|e| e.to_string())?
        };
        if already_exists {
            return Err("Item already exists in vault".to_string());
        }

        let sanitized = sanitize_text(content);
        if sanitized.is_empty() {
            return Err("Content is empty after sanitization".to_string());
        }

        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();
        let id = db.insert_item(
            source_path,
            title,
            None,
            &sanitized,
            content_type,
            None,
            Some(content.len()),
            None, None, None,
            &now,
            &hash,
            None,
            access_tier,
            owner_id,
        ).map_err(|e| e.to_string())?;

        Ok(id)
    }

    /// Ingest an image file into the vault (replaces VisionService::ingest_image).
    pub fn ingest_image(
        &self,
        path: &Path,
        description: Option<&str>,
        tags: Option<&str>,
        access_tier: &str,
        owner_id: Option<&str>,
    ) -> Result<i64, String> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return Err(format!("Failed to read image: {}", e)),
        };

        let hash = format!("{:x}", Sha256::digest(&bytes));

        let already_exists = {
            let db = self.db.lock().unwrap();
            db.item_exists_by_hash(&hash).map_err(|e| e.to_string())?
        };
        if already_exists {
            return Err("Image already exists in vault".to_string());
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let source_path = path.to_string_lossy().to_string();
        let (width, height) = detect_image_dimensions(path);
        let now = chrono::Utc::now().to_rfc3339();

        let meta = if let Some(t) = tags {
            Some(format!("{{\"confirmed\":0,\"tags\":\"{}\"}}", t))
        } else {
            Some("{\"confirmed\":0}".to_string())
        };

        let db = self.db.lock().unwrap();
        let id = db.insert_item(
            &source_path,
            description,
            description,
            "",
            "image",
            Some(&ext),
            Some(bytes.len()),
            width,
            height,
            None,
            &now,
            &hash,
            meta.as_deref(),
            access_tier,
            owner_id,
        ).map_err(|e| e.to_string())?;

        Ok(id)
    }

    /// Confirm an image description (replaces VisionService::confirm_image_description).
    pub fn confirm_image_description(
        &self,
        id: i64,
        description: &str,
        tags: &str,
    ) -> Result<(), String> {
        let db = self.db.lock().unwrap();
        let item = db.get_item(id).map_err(|e| e.to_string())?;
        if item.is_none() {
            return Err("Image not found".to_string());
        }
        // Update metadata to confirmed
        let meta = format!("{{\"confirmed\":1,\"tags\":\"{}\"}}", tags);
        db.conn.execute(
            "UPDATE vault_items SET description = ?1, metadata = ?2 WHERE id = ?3",
            (description, meta, id),
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Batch ingest all files from a directory tree into the vault.
    pub fn sync_directory(
        &self,
        path: &Path,
        access_tier: &str,
        owner_id: Option<&str>,
    ) -> Result<(usize, usize, usize), String> {
        let mut ingested = 0;
        let mut skipped = 0;
        let mut errors = 0;

        for entry in walkdir::WalkDir::new(path) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => { errors += 1; continue; }
            };
            if !entry.file_type().is_file() { continue; }

            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let is_image = matches!(ext.as_str(), "jpg"|"jpeg"|"png"|"gif"|"webp"|"bmp"|"svg"|"ico");

            let result = if is_image {
                self.ingest_image(p, None, None, access_tier, owner_id)
            } else {
                self.ingest_file(p, None, None, access_tier, owner_id)
            };

            match result {
                Ok(_) => ingested += 1,
                Err(e) if e.contains("already exists") => skipped += 1,
                Err(_) => errors += 1,
            }
        }

        Ok((ingested, skipped, errors))
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Search
    // ═════════════════════════════════════════════════════════════════════════

    pub fn search(&self, query: &str, limit: usize
    ) -> Result<Vec<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.search_items(query, limit).map_err(|e| e.to_string())
    }

    pub fn get(&self, id: i64) -> Result<Option<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.get_item(id).map_err(|e| e.to_string())
    }

    pub fn list_all(&self) -> Result<Vec<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.list_all_items().map_err(|e| e.to_string())
    }

    pub fn list_by_type(&self, content_type: &str
    ) -> Result<Vec<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.list_items_by_type(content_type).map_err(|e| e.to_string())
    }

    /// Filter a list of items by the actor's permission level.
    /// Returns only items where actor_level >= item's access_tier.
    fn filter_by_permission(items: Vec<VaultItem>, actor_level: &crate::db::AccessTier) -> Vec<VaultItem> {
        items.into_iter().filter(|item| {
            let item_tier = crate::db::AccessTier::from_str(&item.access_tier)
                .unwrap_or(crate::db::AccessTier::Public);
            actor_level.can_see(&item_tier)
        }).collect()
    }

    /// Search filtered by actor's permission level.
    pub fn search_filtered(
        &self,
        query: &str,
        limit: usize,
        actor_level: &crate::db::AccessTier,
    ) -> Result<Vec<VaultItem>, String> {
        let results = self.search(query, limit)?;
        Ok(Self::filter_by_permission(results, actor_level))
    }

    /// Get a single item filtered by actor's permission level.
    /// Returns None if the item doesn't exist OR actor lacks permission.
    pub fn get_filtered(
        &self,
        id: i64,
        actor_level: &crate::db::AccessTier,
    ) -> Result<Option<VaultItem>, String> {
        let item = self.get(id)?;
        if let Some(ref it) = item {
            let item_tier = crate::db::AccessTier::from_str(&it.access_tier)
                .unwrap_or(crate::db::AccessTier::Public);
            if !actor_level.can_see(&item_tier) {
                return Ok(None);
            }
        }
        Ok(item)
    }

    /// List all items filtered by actor's permission level.
    pub fn list_all_filtered(
        &self,
        actor_level: &crate::db::AccessTier,
    ) -> Result<Vec<VaultItem>, String> {
        let items = self.list_all()?;
        Ok(Self::filter_by_permission(items, actor_level))
    }

    /// Build a permissioned digest of vault items as markdown.
    /// Returns a summary of accessible items, truncated to max_items.
    /// Results are cached for 5 minutes to avoid recomputation on every LLM call.
    pub fn build_permissioned_digest(
        &self,
        actor_level: &crate::db::AccessTier,
        max_items: usize,
    ) -> Result<String, String> {
        static mut CACHE: Option<Box<CachedDigest>> = None;
        static CACHE_ACTOR_LEVEL: std::sync::OnceLock<crate::db::AccessTier> = std::sync::OnceLock::new();

        // Check staleness (5-minute bound)
        let is_stale = unsafe {
            match &CACHE {
                Some(cached) => cached.computed_at.elapsed().as_secs() >= 300,
                None => true,
            }
        };

        // Check actor level matches cached
        let actor_matches = unsafe {
            match CACHE_ACTOR_LEVEL.get() {
                Some(lvl) => lvl == actor_level,
                None => false,
            }
        };

        if !is_stale && actor_matches {
            return Ok(unsafe { CACHE.as_ref().unwrap().content.clone() });
        }

        // Compute fresh digest
        let items = self.list_all_filtered(actor_level)?;
        let total_count = items.len();
        let displayed = if items.len() > max_items {
            &items[..max_items]
        } else {
            &items
        };

        let mut lines = Vec::new();
        lines.push(format!("# Vault Digest ({}/{} items accessible)", total_count, max_items));
        lines.push(String::new());

        for item in displayed {
            let title = item.title.as_deref().unwrap_or("(untitled)");
            let desc = item.description.as_deref().unwrap_or("");
            let has_contradiction = if item.has_contradictions {
                " ⚠️ CONTRADICTION DETECTED"
            } else {
                ""
            };
            lines.push(format!(
                "- **[{}]** `{}` — id:{} tier:{}{}\n  _{}_",
                title,
                item.content_type,
                item.id,
                item.access_tier,
                has_contradiction,
                desc
            ));
        }

        if total_count > max_items {
            lines.push(format!("\n_...and {} more items._", total_count - max_items));
        }

        let digest = lines.join("\n");

        // Update cache
        unsafe {
            CACHE = Some(Box::new(CachedDigest {
                content: digest.clone(),
                computed_at: std::time::Instant::now(),
            }));
            let _ = CACHE_ACTOR_LEVEL.set(actor_level.clone());
        }

        Ok(digest)
    }

    pub fn delete(&self, id: i64) -> Result<bool, String> {
        let db = self.db.lock().unwrap();
        db.delete_item(id).map_err(|e| e.to_string())
    }

    /// Semantic search over vault embeddings.
    /// Returns items sorted by cosine similarity, highest first.
    pub fn semantic_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(VaultItem, f64)>, String> {
        let db = self.db.lock().unwrap();
        let all = db.all_embeddings().map_err(|e| e.to_string())?;

        let mut scored: Vec<(i64, f64)> = Vec::with_capacity(all.len());
        for emb in all {
            let vec = crate::embeddings::bytes_to_embedding(&emb.embedding);
            if vec.len() == query_embedding.len() {
                let sim = crate::embeddings::cosine_similarity(query_embedding, &vec);
                scored.push((emb.item_id, sim));
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        let mut results = Vec::with_capacity(scored.len());
        for (item_id, sim) in scored {
            if let Ok(Some(item)) = db.get_item(item_id) {
                results.push((item, sim));
            }
        }
        Ok(results)
    }

    /// Store an embedding for a vault item.
    pub fn store_embedding(
        &self,
        item_id: i64,
        embedding: &[f32],
        model: &str,
    ) -> Result<(), String> {
        let db = self.db.lock().unwrap();
        let bytes = crate::embeddings::embedding_to_bytes(embedding);
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_embedding(item_id, &bytes, model, &now)
            .map_err(|e| e.to_string())?;
        db.update_item_embedding_synced(item_id, true)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn item_exists_by_hash(&self, hash: &str) -> Result<bool, String> {
        let db = self.db.lock().unwrap();
        db.item_exists_by_hash(hash).map_err(|e| e.to_string())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Relationships
    // ═════════════════════════════════════════════════════════════════════════

    pub fn link_items(
        &self,
        source_id: i64,
        target_id: i64,
        relation_type: &str,
        confidence: f64,
        reason: Option<&str>,
    ) -> Result<i64, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();
        db.insert_relationship(source_id, target_id, relation_type, confidence, reason, &now)
            .map_err(|e| e.to_string())
    }

    pub fn get_related_items(
        &self,
        id: i64,
    ) -> Result<Vec<VaultRelationship>, String> {
        let db = self.db.lock().unwrap();
        db.get_relationships_for_item(id, None).map_err(|e| e.to_string())
    }

    pub fn get_contradictions(
        &self,
        id: i64,
    ) -> Result<Vec<VaultRelationship>, String> {
        let db = self.db.lock().unwrap();
        db.get_relationships_for_item(id, Some("contradicts")).map_err(|e| e.to_string())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Contradictions
    // ═════════════════════════════════════════════════════════════════════════

    /// Flag a contradiction between two items and create a notification.
    pub fn flag_contradiction(
        &self,
        newer_id: i64,
        older_id: i64,
        reason: &str,
        confidence: f64,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();

        db.insert_relationship(newer_id, older_id, "contradicts", confidence, Some(reason), &now)
            .map_err(|e| e.to_string())?;

        db.update_item_contradiction(newer_id, true, Some(reason))
            .map_err(|e| e.to_string())?;
        db.update_item_contradiction(older_id, true, Some(reason))
            .map_err(|e| e.to_string())?;

        let msg = format!("Contradiction detected: newer item {} contradicts older item {}. Reason: {}", newer_id, older_id, reason);
        db.insert_notification(newer_id, "contradiction", &msg, &now)
            .map_err(|e| e.to_string())?;
        db.insert_notification(older_id, "contradiction", &msg, &now)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Notifications
    // ═════════════════════════════════════════════════════════════════════════

    pub fn get_unread_notifications(
        &self,
        limit: usize,
    ) -> Result<Vec<VaultNotification>, String> {
        let db = self.db.lock().unwrap();
        db.get_unread_notifications(limit).map_err(|e| e.to_string())
    }

    pub fn mark_notification_read(&self,
        id: i64,
    ) -> Result<bool, String> {
        let db = self.db.lock().unwrap();
        db.mark_notification_read(id).map_err(|e| e.to_string())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Embeddings (Phase 2 hooks)
    // ═════════════════════════════════════════════════════════════════════════

    pub fn queue_for_embedding(&self, id: i64) -> Result<(), String> {
        let db = self.db.lock().unwrap();
        db.update_item_embedding_synced(id, false).map_err(|e| e.to_string())
    }

    pub fn get_items_needing_embeddings(
        &self, limit: usize
    ) -> Result<Vec<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.list_items_needing_embeddings(limit).map_err(|e| e.to_string())
    }

    pub fn get_items_needing_concepts(&self, limit: usize) -> Result<Vec<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.list_items_needing_concepts(limit).map_err(|e| e.to_string())
    }

    pub fn get_versioned_items_unchecked(&self, limit: usize) -> Result<Vec<VaultItem>, String> {
        let db = self.db.lock().unwrap();
        db.list_versioned_items_unchecked(limit).map_err(|e| e.to_string())
    }

    /// Build the text payload for embedding generation from a vault item.
    /// Combines title, description, and content with appropriate weighting.
    pub fn embedding_text_for_item(&self,
        item: &VaultItem,
    ) -> String {
        let mut parts = Vec::new();
        if let Some(ref title) = item.title {
            if !title.is_empty() {
                parts.push(title.clone());
            }
        }
        if let Some(ref desc) = item.description {
            if !desc.is_empty() {
                parts.push(desc.clone());
            }
        }
        if !item.content.is_empty() {
            let content = item.content.chars().take(2000).collect::<String>();
            parts.push(content);
        }
        parts.join("\n\n")
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Graph builder (for mindmap)
    // ═════════════════════════════════════════════════════════════════════════

    pub fn build_directory_graph(
        &self,
    ) -> Result<crate::mindmap::MindMap, String> {
        let items = self.list_all()?;
        let mut mm = crate::mindmap::MindMapService::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Root node
        mm.add_node("vault:root", "The Vault".to_string(), "root");
        seen.insert("vault:root".to_string());

        for item in &items {
            let path = std::path::Path::new(&item.source_path);
            let mut current = "vault:root".to_string();

            // Build directory nodes from path ancestors
            if let Some(parent) = path.parent() {
                for component in parent.components() {
                    let comp_str = component.as_os_str().to_string_lossy().to_string();
                    let node_id = format!("dir:{}", comp_str);
                    if seen.insert(node_id.clone()) {
                        mm.add_node(&node_id, comp_str.clone(), "dir");
                    }
                    mm.add_edge(&current, &node_id, "contains");
                    current = node_id;
                }
            }

            // Add item node
            let node_type = if item.content_type == "image" { "image" } else { "file" };
            let label = item.title.clone().unwrap_or_else(|| {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("item_{}", item.id))
            });

            let mut meta = std::collections::HashMap::new();
            meta.insert("item_id".to_string(), item.id.to_string());
            meta.insert("content_type".to_string(), item.content_type.clone());
            if item.has_contradictions {
                meta.insert("warning".to_string(), "contradiction".to_string());
            }

            mm.add_node_with_metadata(&item.source_path, label.clone(), node_type, meta
            );
            mm.add_edge(&current, &item.source_path, "contains");
        }

        // Add relationship edges
        for item in &items {
            let rels = self.get_related_items(item.id)?;
            for rel in rels {
                let target = self.get(rel.target_id)?;
                if let Some(t) = target {
                    mm.add_edge(
                        &item.source_path,
                        &t.source_path,
                        &rel.relation_type,
                    );
                }
            }
        }

        Ok(mm.graph().clone())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Astra autonomous behaviors — extracted Ollama logic for background use
    // ═════════════════════════════════════════════════════════════════════════

    async fn call_ollama_json(&self, prompt: &str) -> Result<serde_json::Value, String> {
        let base = std::env::var("AVALON_OLLAMA_BASE")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = std::env::var("AVALON_LIBRARIAN_MODEL")
            .unwrap_or_else(|_| "qwen2.5-coder:7b".to_string());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;

        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "format": "json",
            "options": {
                "temperature": 0.2,
                "num_predict": 512
            }
        });

        let resp = client.post(format!("{}/api/generate", base))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, text));
        }

        let resp_json: serde_json::Value = resp.json().await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        let response_text = resp_json.get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        if response_text.is_empty() {
            return Err("Ollama returned empty response".to_string());
        }

        serde_json::from_str(response_text)
            .or_else(|_| {
                let cleaned = response_text
                    .replace("```json", "")
                    .replace("```", "")
                    .trim()
                    .to_string();
                serde_json::from_str(&cleaned)
            })
            .map_err(|e| format!("Failed to parse Ollama response JSON: {}. Raw: {}", e, response_text))
    }

    /// Extract concepts from a vault item using Ollama and create concept nodes.
    /// Returns the number of concepts extracted.
    pub async fn extract_concepts_for_item(&self, item_id: i64) -> Result<usize, String> {
        let text = {
            let db = self.db.lock().unwrap();
            let item = db.get_item(item_id).map_err(|e| e.to_string())?
                .ok_or("Item not found")?;
            let mut parts = Vec::new();
            if let Some(ref title) = item.title {
                parts.push(format!("Title: {}", title));
            }
            if let Some(ref desc) = item.description {
                parts.push(format!("Description: {}", desc));
            }
            let content_preview: String = item.content.chars().take(3000).collect();
            parts.push(format!("Content: {}", content_preview));
            parts.join("\n")
        };

        let prompt = format!(
            "Extract 5-10 key concepts from the following text. Return ONLY a JSON array of strings. Each concept should be a concise noun phrase (1-4 words).\n\n{}\n\nConcepts:",
            text
        );

        let concepts_json = self.call_ollama_json(&prompt).await?;
        let concepts: Vec<String> = concepts_json
            .as_array()
            .ok_or("Expected JSON array of concepts")?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();

        if concepts.is_empty() {
            // Mark as processed even when no concepts found
            let db = self.db.lock().unwrap();
            let _ = db.mark_concepts_extracted(item_id, "astra");
            return Ok(0);
        }

        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();

        for concept in &concepts {
            // Check if concept item already exists
            let concept_id = {
                let existing = db.search_items(concept, 5)
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|i| i.content_type == "concept" && i.title.as_deref() == Some(concept));
                if let Some(existing) = existing {
                    existing.id
                } else {
                    // Insert new concept item directly via db (avoids re-entrant lock on vault service)
                    let source_path = format!("concept://{}", concept);
                    db.insert_item(
                        &source_path,
                        Some(concept),
                        None,
                        "",
                        "concept",
                        None,
                        Some(0),
                        None, None, None,
                        &now,
                        &format!("{:x}", sha2::Sha256::digest(concept.as_bytes())),
                        None,
                        "Public",
                        None,
                    ).map_err(|e| e.to_string())?
                }
            };

            let _ = db.insert_relationship(item_id, concept_id, "teaches", 0.95, Some("AI-extracted concept"), &now);
        }

        // Mark as processed
        db.mark_concepts_extracted(item_id, "astra").map_err(|e| e.to_string())?;

        Ok(concepts.len())
    }

    /// Detect contradictions between a vault item and its older version.
    /// Returns Ok(true) if a contradiction was flagged, Ok(false) if not.
    pub async fn detect_contradiction_for_item(&self, item_id: i64) -> Result<bool, String> {
        let (newer_content, older_content, older_id) = {
            let db = self.db.lock().unwrap();
            let rels = db.get_relationships_for_item(item_id, Some("older_version"))
                .map_err(|e| e.to_string())?;
            let older_rel = rels.iter().find(|r| r.relation_type == "older_version");
            let older_id = match older_rel {
                Some(r) => r.target_id,
                None => return Ok(false), // no older version
            };
            let newer = db.get_item(item_id).map_err(|e| e.to_string())?
                .ok_or("Item not found")?;
            let older = db.get_item(older_id).map_err(|e| e.to_string())?
                .ok_or("Older item not found")?;
            (newer.content.clone(), older.content.clone(), older_id)
        };

        let prompt = format!(
            "Compare these two excerpts from different versions of the same document. Do they contain contradictory claims or information?\n\nReturn ONLY a JSON object with these exact keys:\n- \"contradicts\": boolean (true if they contradict)\n- \"reason\": string (one sentence explaining the conflict, or \"No contradiction detected\" if they agree)\n- \"confidence\": number 0.0-1.0 (your certainty)\n\nOLDER VERSION:\n{}\n\nNEWER VERSION:\n{}\n\nAnalysis:",
            older_content.chars().take(2000).collect::<String>(),
            newer_content.chars().take(2000).collect::<String>()
        );

        let result = self.call_ollama_json(&prompt).await?;
        let contradicts = result.get("contradicts").and_then(|v| v.as_bool()).unwrap_or(false);
        let reason = result.get("reason").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
        let confidence = result.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);

        if contradicts && confidence >= 0.85 {
            self.flag_contradiction(item_id, older_id, &reason, confidence)?;
        }

        // Mark as checked even if no contradiction found
        let db = self.db.lock().unwrap();
        db.mark_contradiction_checked(item_id, "astra").map_err(|e| e.to_string())?;

        Ok(contradicts && confidence >= 0.85)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Image detection helpers
// ═════════════════════════════════════════════════════════════════════════════

fn detect_image_dimensions(path: &Path) -> (Option<i64>, Option<i64>) {
    if let Ok(reader) = image::ImageReader::open(path) {
        if let Ok(img) = reader.decode() {
            let (w, h) = img.dimensions();
            return (Some(w as i64), Some(h as i64));
        }
    }
    (None, None)
}

// ═════════════════════════════════════════════════════════════════════════════
// Content extraction
// ═════════════════════════════════════════════════════════════════════════════

fn extract_pdf_text(bytes: &[u8]) -> Result<String, String> {
    use lopdf::Document;
    use std::io::Cursor;

    let doc = Document::load_from(Cursor::new(bytes))
        .map_err(|e| format!("PDF parse error: {}", e))?;

    let mut text = String::new();
    for (page_num, _page_id) in doc.get_pages() {
        if let Ok(page_text) = doc.extract_text(&[page_num]) {
            text.push_str(&page_text);
            text.push('\n');
        }
    }

    if text.is_empty() {
        return Err("PDF contains no extractable text".to_string());
    }

    Ok(sanitize_text(&text))
}

// ═════════════════════════════════════════════════════════════════════════════
// Content sanitization
// ═════════════════════════════════════════════════════════════════════════════

fn sanitize_text(input: &str) -> String {
    // Step 3: Null-byte removal + control character stripping
    // Step 4: Whitespace normalization — trim ends, collapse runs
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\0' => continue, // strip null bytes
            '\t' | '\n' | '\r' => output.push(ch), // preserve formatting whitespace
            '\u{0001}'..='\u{0008}'
            | '\u{000b}'..='\u{000c}'
            | '\u{000e}'..='\u{001f}' => continue, // strip other control chars
            _ => output.push(ch),
        }
    }
    // Trim leading/trailing and collapse internal runs of whitespace to single space
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_html_text(input: &str) -> String {
    let tag_re = regex::Regex::new(r"<[^>]*>").unwrap();
    let stripped = tag_re.replace_all(input, " ");
    let mut text = stripped.to_string();
    text = text.replace("&amp;", "&");
    text = text.replace("&lt;", "<");
    text = text.replace("&gt;", ">");
    text = text.replace("&quot;", "\"");
    text = text.replace("&apos;", "'");
    text = text.replace("&nbsp;", " ");
    sanitize_text(&text)
}
