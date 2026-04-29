use crate::db::{VaultDb, VisionImage};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// VisionVault Service
// Handles image ingestion, metadata extraction, and retrieval.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VisionService {
    db: Arc<Mutex<VaultDb>>,
}

impl VisionService {
    pub fn new(db: Arc<Mutex<VaultDb>>) -> Self {
        Self { db }
    }

    /// Ingest an image from disk into the vision vault.
    /// Returns the image ID if successful.
    pub fn ingest_image(
        &self,
        path: &Path,
        suggested_description: Option<&str>,
    ) -> Result<i64, String> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return Err(format!("Failed to read image: {}", e)),
        };

        let hash = format!("{:x}", Sha256::digest(&bytes));

        let already_exists = {
            let db = self.db.lock().unwrap();
            db.image_exists_by_hash(&hash).map_err(|e| e.to_string())?
        };
        if already_exists {
            return Err("Image already exists in vision vault".to_string());
        }

        let format = detect_format(&bytes);
        let (width, height) = detect_dimensions(&bytes, format.as_deref());

        let source_path = path.to_string_lossy().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let db = self.db.lock().unwrap();
        let id = db
            .insert_image(
                &source_path,
                suggested_description,
                None,
                width,
                height,
                format.as_deref(),
                &now,
                &hash,
            )
            .map_err(|e| e.to_string())?;

        Ok(id)
    }

    /// Confirm or update an image description (user review step).
    pub fn confirm_description(
        &self,
        id: i64,
        description: &str,
        tags: Vec<String>,
    ) -> Result<(), String> {
        let tags_json = serde_json::to_string(&tags).map_err(|e| e.to_string())?;
        let db = self.db.lock().unwrap();
        db.confirm_image_description(id, description, &tags_json)
            .map_err(|e| e.to_string())
    }

    /// Search images by FTS5 query.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<VisionImage>, String> {
        let db = self.db.lock().unwrap();
        db.search_images(query, limit).map_err(|e| e.to_string())
    }

    /// Retrieve a single image by ID.
    pub fn get(&self, id: i64) -> Result<Option<VisionImage>, String> {
        let db = self.db.lock().unwrap();
        db.get_image(id).map_err(|e| e.to_string())
    }

    /// Delete an image from the vision vault.
    pub fn delete(&self, id: i64) -> Result<bool, String> {
        let db = self.db.lock().unwrap();
        db.delete_image(id).map_err(|e| e.to_string())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Format detection
// ═════════════════════════════════════════════════════════════════════════════

fn detect_format(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 8 {
        return None;
    }
    if &bytes[0..4] == b"\x89PNG" {
        return Some("png".to_string());
    }
    if &bytes[0..3] == b"\xFF\xD8\xFF" {
        return Some("jpeg".to_string());
    }
    if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        return Some("gif".to_string());
    }
    if &bytes[0..4] == b"RIFF" && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return Some("webp".to_string());
    }
    if &bytes[0..2] == b"BM" {
        return Some("bmp".to_string());
    }
    None
}

fn detect_dimensions(bytes: &[u8], format: Option<&str>) -> (Option<i64>, Option<i64>) {
    match format {
        Some("png") => detect_png_dimensions(bytes),
        Some("jpeg") => detect_jpeg_dimensions(bytes),
        Some("gif") => detect_gif_dimensions(bytes),
        Some("bmp") => detect_bmp_dimensions(bytes),
        Some("webp") => detect_webp_dimensions(bytes),
        _ => (None, None),
    }
}

fn detect_png_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    // IHDR chunk starts at offset 16; width/height are 4 bytes each, big-endian
    if bytes.len() >= 24 {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as i64;
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as i64;
        (Some(w), Some(h))
    } else {
        (None, None)
    }
}

fn detect_jpeg_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        // Skip padding and non-SOF markers
        if marker == 0xD9 || marker == 0x00 {
            i += 2;
            continue;
        }
        // SOF0, SOF1, SOF2, SOF3, SOF5, SOF6, SOF7, SOF9, SOF10, SOF11, SOF13, SOF14, SOF15
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as i64;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as i64;
            return (Some(w), Some(h));
        }
        // Segment length includes the length bytes themselves
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        i += 2 + len;
    }
    (None, None)
}

fn detect_gif_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    if bytes.len() >= 10 {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as i64;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as i64;
        (Some(w), Some(h))
    } else {
        (None, None)
    }
}

fn detect_bmp_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    if bytes.len() >= 26 {
        let w = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as i64;
        let h = u32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]) as i64;
        (Some(w), Some(h))
    } else {
        (None, None)
    }
}

fn detect_webp_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    if bytes.len() < 30 {
        return (None, None);
    }
    // VP8 chunk at offset 12: 'VP8 ' (4 bytes) + size (4 bytes) + version (3 bytes) + flag (1 byte) + width (2 bytes) + height (2 bytes)
    for i in 12..bytes.len().saturating_sub(10) {
        if &bytes[i..i + 4] == b"VP8 " {
            let w = u16::from_le_bytes([bytes[i + 8], bytes[i + 9]]) as i64;
            let h = u16::from_le_bytes([bytes[i + 10], bytes[i + 11]]) as i64;
            return (Some(w & 0x3FFF), Some(h & 0x3FFF));
        }
    }
    (None, None)
}
