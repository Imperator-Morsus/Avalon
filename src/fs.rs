use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ═════════════════════════════════════════════════════════════════════════════
// File System Configuration (Limiter)
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileSystemConfig {
    #[serde(default = "default_policy_deny")]
    pub default_policy: String,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub denied_paths: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
}

fn default_policy_deny() -> String { "deny".to_string() }
fn default_max_file_size() -> usize { 10 * 1024 * 1024 } // 10 MB

impl Default for FileSystemConfig {
    fn default() -> Self {
        FileSystemConfig {
            default_policy: "deny".to_string(),
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            max_file_size: default_max_file_size(),
        }
    }
}

impl FileSystemConfig {
    pub fn load() -> Self {
        let config_path = std::env::current_exe()
            .ok()
            .and_then(|p| {
                let mut path = p;
                path.pop(); path.pop(); path.pop();
                Some(path)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(".avalon_fs.json");

        if !config_path.exists() {
            return FileSystemConfig::default();
        }
        match std::fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => FileSystemConfig::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let config_path = std::env::current_exe()
            .ok()
            .and_then(|p| {
                let mut path = p;
                path.pop(); path.pop(); path.pop();
                Some(path)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(".avalon_fs.json");

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(&config_path, content)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        Ok(())
    }

/// Check if a path is allowed by the limiter rules.
    /// Returns true if the operation is permitted.
    pub fn is_allowed(&self, path: &str) -> bool {
        let path = normalize_path(path);
        let sep = std::path::MAIN_SEPARATOR;

        // 1. Config file is always readable for transparency
        if path.ends_with(".avalon_fs.json") {
            return true;
        }

        // 2. Check deny list first (deny always wins)
        for denied in &self.denied_paths {
            let denied_norm = clean_path_prefix(normalize_path(denied));
            if path == denied_norm || path.starts_with(&(denied_norm.clone() + &sep.to_string())) {
                return false;
            }
        }

        // 3. Check allow list
        if self.allowed_paths.is_empty() {
            // No explicit allow list => fall back to default policy
            return self.default_policy == "allow";
        }

        for allowed in &self.allowed_paths {
            let allowed_norm = clean_path_prefix(normalize_path(allowed));
            if path == allowed_norm || path.starts_with(&(allowed_norm.clone() + &sep.to_string())) {
                return true;
            }
        }

        // Path didn't match any allowed prefix
        false
    }
}

fn clean_path_prefix(s: String) -> String {
    let mut s = s;
    while s.ends_with('*') || s.ends_with(std::path::MAIN_SEPARATOR) {
        s.pop();
    }
    s
}

fn strip_unc_prefix(s: String) -> String {
    s.strip_prefix("\\\\?\\").map(String::from).unwrap_or(s)
}

fn normalize_path(p: &str) -> String {
    let path = PathBuf::from(p);

    // Try canonicalize first for existing paths (resolves symlinks)
    if let Ok(canonical) = path.canonicalize() {
        let s = canonical.to_string_lossy().to_string().to_lowercase();
        return strip_unc_prefix(s);
    }

    // Fallback for non-existent paths: resolve to absolute and clean components
    let abs = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };

    let mut cleaned = PathBuf::new();
    for component in abs.components() {
        cleaned.push(component.as_os_str());
    }

    let mut s = cleaned.to_string_lossy().to_string().to_lowercase();
    while s.ends_with(std::path::MAIN_SEPARATOR) {
        s.pop();
    }
    strip_unc_prefix(s)
}

// ═════════════════════════════════════════════════════════════════════════════
// File System Service
// ═════════════════════════════════════════════════════════════════════════════

pub struct FileSystemService {
    config: FileSystemConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileOperationResult {
    pub success: bool,
    pub path: String,
    pub content: Option<String>,
    pub error: Option<String>,
    pub entries: Option<Vec<String>>,
}

impl FileSystemService {
    pub fn new() -> Self {
        FileSystemService {
            config: FileSystemConfig::load(),
        }
    }

    pub fn config(&self) -> &FileSystemConfig {
        &self.config
    }

    pub fn reload_config(&mut self) {
        self.config = FileSystemConfig::load();
    }

    fn check_limiter(&self, path: &str) -> Result<(), String> {
        if !self.config.is_allowed(path) {
            return Err(format!(
                "Path '{}' is not allowed by the file system limiter. \
                 Add it to .avalon_fs.json -> allowed_paths to enable access.",
                path
            ));
        }
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> FileOperationResult {
        if let Err(e) = self.check_limiter(path) {
            return FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(e),
                entries: None,
            };
        }

        match std::fs::metadata(path) {
            Ok(meta) => {
                if meta.len() > self.config.max_file_size as u64 {
                    return FileOperationResult {
                        success: false,
                        path: path.to_string(),
                        content: None,
                        error: Some(format!(
                            "File too large: {} MB (max {} MB)",
                            meta.len() / (1024 * 1024),
                            self.config.max_file_size / (1024 * 1024)
                        )),
                        entries: None,
                    };
                }
            }
            Err(e) => {
                return FileOperationResult {
                    success: false,
                    path: path.to_string(),
                    content: None,
                    error: Some(format!("Failed to read metadata: {}", e)),
                    entries: None,
                };
            }
        }

        match std::fs::read_to_string(path) {
            Ok(content) => FileOperationResult {
                success: true,
                path: path.to_string(),
                content: Some(content),
                error: None,
                entries: None,
            },
            Err(e) => FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(format!("Failed to read file: {}", e)),
                entries: None,
            },
        }
    }

    pub fn write_file(&self, path: &str, content: &str) -> FileOperationResult {
        if let Err(e) = self.check_limiter(path) {
            return FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(e),
                entries: None,
            };
        }

        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return FileOperationResult {
                    success: false,
                    path: path.to_string(),
                    content: None,
                    error: Some(format!("Failed to create parent directory: {}", e)),
                    entries: None,
                };
            }
        }

        match std::fs::write(path, content) {
            Ok(()) => FileOperationResult {
                success: true,
                path: path.to_string(),
                content: None,
                error: None,
                entries: None,
            },
            Err(e) => FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(format!("Failed to write file: {}", e)),
                entries: None,
            },
        }
    }

    pub fn list_dir(&self, path: &str) -> FileOperationResult {
        if let Err(e) = self.check_limiter(path) {
            return FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(e),
                entries: None,
            };
        }

        match std::fs::read_dir(path) {
            Ok(entries) => {
                let names: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        let meta = e.metadata().ok();
                        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                        if is_dir { format!("{}/", name) } else { name }
                    })
                    .collect();
                FileOperationResult {
                    success: true,
                    path: path.to_string(),
                    content: None,
                    error: None,
                    entries: Some(names),
                }
            }
            Err(e) => FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(format!("Failed to list directory: {}", e)),
                entries: None,
            },
        }
    }

    pub fn delete_file(&self, path: &str) -> FileOperationResult {
        if let Err(e) = self.check_limiter(path) {
            return FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(e),
                entries: None,
            };
        }

        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                return FileOperationResult {
                    success: false,
                    path: path.to_string(),
                    content: None,
                    error: Some(format!("Failed to read metadata: {}", e)),
                    entries: None,
                };
            }
        };

        let result = if meta.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };

        match result {
            Ok(()) => FileOperationResult {
                success: true,
                path: path.to_string(),
                content: None,
                error: None,
                entries: None,
            },
            Err(e) => FileOperationResult {
                success: false,
                path: path.to_string(),
                content: None,
                error: Some(format!("Failed to delete: {}", e)),
                entries: None,
            },
        }
    }
}
