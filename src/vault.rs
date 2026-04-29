use crate::db::VaultDb;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// MindVault Service
// Handles document ingestion, sanitization, and retrieval.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultService {
    db: Arc<Mutex<VaultDb>>,
}

impl VaultService {
    pub fn new(db: Arc<Mutex<VaultDb>>) -> Self {
        Self { db }
    }

    /// Ingest a document from disk into the vault.
    /// Returns the document ID if successful.
    pub fn ingest_file(&self,
        path: &Path,
        title: Option<&str>,
        content_type_hint: Option<&str>,
    ) -> Result<i64, String> {
        // Read file bytes for hashing
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return Err(format!("Failed to read file: {}", e)),
        };

        // Compute hash to deduplicate
        let hash = format!("{:x}", Sha256::digest(&bytes));

        // Check if already ingested
        let already_exists = {
            let db = self.db.lock().unwrap();
            db.document_exists_by_hash(&hash).map_err(|e| e.to_string())?
        };
        if already_exists {
            return Err("Document already exists in vault".to_string());
        }

        // Determine content type
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let content_type = content_type_hint.unwrap_or_else(|| {
            match ext.as_str() {
                "pdf" => "pdf",
                "md" => "markdown",
                "rs" | "js" | "ts" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs" | "sh" | "bat" | "ps1" => "code",
                "html" | "htm" => "html",
                "txt" | "log" => "text",
                _ => "text",
            }
        });

        // Extract and sanitize content
        let content = match content_type {
            "pdf" => extract_pdf_text(&bytes)?,
            "html" => {
                let raw = String::from_utf8_lossy(&bytes).to_string();
                sanitize_html_text(&raw)
            }
            _ => {
                let raw = String::from_utf8_lossy(&bytes).to_string();
                sanitize_text(&raw)
            }
        };

        if content.is_empty() {
            return Err("Extracted content is empty".to_string());
        }

        let title = title.map(|s| s.to_string()).or_else(|| {
            path.file_stem().map(|s| s.to_string_lossy().to_string())
        });

        let now = chrono::Utc::now().to_rfc3339();
        let source_path = path.to_string_lossy().to_string();

        let db = self.db.lock().unwrap();
        let id = db.insert_document(
            &source_path,
            title.as_deref(),
            &content,
            content_type,
            bytes.len(),
            &now,
            &hash,
        ).map_err(|e| e.to_string())?;

        Ok(id)
    }

    /// Ingest text content directly (for fetched/scraped content without a file).
    pub fn ingest_text(
        &self,
        source_path: &str,
        title: Option<&str>,
        content: &str,
        content_type: &str,
    ) -> Result<i64, String> {
        let hash = format!("{:x}", Sha256::digest(content.as_bytes()));

        let already_exists = {
            let db = self.db.lock().unwrap();
            db.document_exists_by_hash(&hash).map_err(|e| e.to_string())?
        };
        if already_exists {
            return Err("Document already exists in vault".to_string());
        }

        let sanitized = sanitize_text(content);
        if sanitized.is_empty() {
            return Err("Content is empty after sanitization".to_string());
        }

        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();
        let id = db.insert_document(
            source_path,
            title,
            &sanitized,
            content_type,
            content.len(),
            &now,
            &hash,
        ).map_err(|e| e.to_string())?;

        Ok(id)
    }

    /// Search documents by FTS5 query.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<crate::db::VaultDoc>, String> {
        let db = self.db.lock().unwrap();
        db.search_documents(query, limit).map_err(|e| e.to_string())
    }

    /// Retrieve a single document by ID.
    pub fn get(&self, id: i64) -> Result<Option<crate::db::VaultDoc>, String> {
        let db = self.db.lock().unwrap();
        db.get_document(id).map_err(|e| e.to_string())
    }

    /// Delete a document from the vault.
    pub fn delete(&self, id: i64) -> Result<bool, String> {
        let db = self.db.lock().unwrap();
        db.delete_document(id).map_err(|e| e.to_string())
    }

    /// Re-ingest an already-vaulted file (useful after edits).
    pub fn reingest_file(&self, path: &Path) -> Result<i64, String> {
        // Find existing doc by path and delete it, then re-ingest
        let existing_id = {
            let db = self.db.lock().unwrap();
            // Search for exact path match
            let docs = db.search_documents(path.to_string_lossy().as_ref(), 100)
                .map_err(|e| e.to_string())?;
            docs.into_iter().find(|d| d.source_path == path.to_string_lossy())
                .map(|d| d.id)
        };

        if let Some(id) = existing_id {
            let db = self.db.lock().unwrap();
            let _ = db.delete_document(id);
        }

        self.ingest_file(path, None, None)
    }
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
    for (page_num, page_id) in doc.get_pages() {
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
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        // Remove null bytes and control characters except newlines/tabs
        match ch {
            '\0' => continue,
            '\u{0001}'..='\u{0008}' | '\u{000b}'..='\u{000c}' | '\u{000e}'..='\u{001f}' => continue,
            _ => output.push(ch),
        }
    }
    // Normalize whitespace
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_html_text(input: &str) -> String {
    // Strip tags, extract text content
    let tag_re = regex::Regex::new(r"<[^>]*>").unwrap();
    let stripped = tag_re.replace_all(input, " ");

    // Decode common HTML entities
    let mut text = stripped.to_string();
    text = text.replace("&amp;", "&");
    text = text.replace("&lt;", "<");
    text = text.replace("&gt;", ">");
    text = text.replace("&quot;", "\"");
    text = text.replace("&apos;", "'");
    text = text.replace("&nbsp;", " ");

    sanitize_text(&text)
}
