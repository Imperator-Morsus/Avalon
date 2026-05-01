use std::collections::{HashMap, HashSet};
use std::path::Path;
use regex::Regex;
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Mind Map Graph
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapNode {
    pub id: String,
    pub label: String,
    pub node_type: String, // "file" | "dir" | "symbol"
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapEdge {
    pub source: String,
    pub target: String,
    pub relation: String, // "imports" | "references" | "contains" | "depends_on"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMap {
    pub nodes: Vec<MindMapNode>,
    pub edges: Vec<MindMapEdge>,
    pub root: String,
}

impl MindMap {
    /// Return a truncated copy with at most `max_nodes` nodes and only edges between kept nodes.
    pub fn truncated(&self, max_nodes: usize) -> MindMap {
        let mut nodes = self.nodes.clone();
        nodes.truncate(max_nodes);
        let kept: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let edges: Vec<MindMapEdge> = self.edges
            .iter()
            .filter(|e| kept.contains(&e.source) && kept.contains(&e.target))
            .cloned()
            .collect();
        MindMap {
            nodes,
            edges,
            root: self.root.clone(),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Mind Map Service
// ═════════════════════════════════════════════════════════════════════════════

pub struct MindMapService {
    graph: MindMap,
    cached_graph: Option<MindMap>,
    remote_graph: Option<MindMap>,
}

impl MindMapService {
    pub fn new() -> Self {
        MindMapService {
            graph: MindMap {
                nodes: Vec::new(),
                edges: Vec::new(),
                root: String::new(),
            },
            cached_graph: None,
            remote_graph: None,
        }
    }

    pub fn graph(&self) -> &MindMap {
        &self.graph
    }

    pub fn cached(&self) -> Option<&MindMap> {
        self.cached_graph.as_ref()
    }

    pub fn build_and_cache(&mut self, allowed_paths: &[String], max_depth: usize) {
        self.build(allowed_paths, max_depth);
        self.cached_graph = Some(self.graph.clone());
    }

    pub fn remote_graph(&self) -> Option<&MindMap> {
        self.remote_graph.as_ref()
    }

    pub fn set_remote_graph(&mut self, remote: MindMap) {
        self.remote_graph = Some(remote);
    }

    pub fn clear_remote_graph(&mut self) {
        self.remote_graph = None;
    }

    pub fn merge_remote(&mut self) -> bool {
        if let Some(remote) = self.remote_graph.take() {
            self.merge(&remote);
            true
        } else {
            false
        }
    }

    /// Build a mind map from a list of allowed paths.
    /// Only scans files the limiter allows.
    pub fn build(&mut self, allowed_paths: &[String], max_depth: usize) {
        self.build_with_prefix(allowed_paths, max_depth, None);
    }

    pub fn build_with_prefix(
        &mut self,
        allowed_paths: &[String],
        max_depth: usize,
        prefix: Option<&str>,
    ) {
        if prefix.is_none() {
            self.graph.nodes.clear();
            self.graph.edges.clear();
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: Vec<(String, usize)> = Vec::new();

        for path in allowed_paths {
            let abs = std::fs::canonicalize(path)
                .unwrap_or_else(|_| Path::new(path).to_path_buf())
                .to_string_lossy()
                .to_string();
            queue.push((abs, 0));
        }

        let re_rust_use = Regex::new(r#"^\s*use\s+([a-zA-Z_][a-zA-Z0-9_:]*);"#).unwrap();
        let re_rust_mod = Regex::new(r#"^\s*mod\s+([a-zA-Z_][a-zA-Z0-9_]*);"#).unwrap();
        let re_js_import = Regex::new(r#"(?:import\s+.*?from\s+['"](.+?)['"]|require\(['"](.+?)['"]\))"#).unwrap();
        let re_py_import = Regex::new(r#"^(?:from\s+([a-zA-Z_][a-zA-Z0-9_.]*)\s+import|import\s+([a-zA-Z_][a-zA-Z0-9_.]*))"#).unwrap();

        while let Some((path_str, depth)) = queue.pop() {
            if visited.contains(&path_str) || depth > max_depth {
                continue;
            }
            visited.insert(path_str.clone());

            let path = Path::new(&path_str);

            if path.is_dir() {
                self.add_node(&path_str, node_name(path), "dir");

                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let entry_path = entry.path();
                        let entry_str = entry_path.to_string_lossy().to_string();

                        if entry_path.is_dir() {
                            self.add_node(&entry_str, node_name(&entry_path), "dir");
                            if depth < max_depth {
                                queue.push((entry_str.clone(), depth + 1));
                            }
                            self.add_edge(&path_str, &entry_str, "contains");
                        } else if is_source_file(&entry_path) || is_image_file(&entry_path) {
                            if is_image_file(&entry_path) {
                                let mut meta = HashMap::new();
                                meta.insert("image_path".to_string(), entry_str.clone());
                                self.add_node_with_metadata(&entry_str, node_name(&entry_path), "image", meta);
                            } else {
                                self.add_node(&entry_str, node_name(&entry_path), "file");
                            }
                            if depth < max_depth {
                                queue.push((entry_str.clone(), depth + 1));
                            }
                            self.add_edge(&path_str, &entry_str, "contains");
                        }
                    }
                }
            } else if path.is_file() && (is_source_file(path) || is_image_file(path)) {
                if is_image_file(path) {
                    let mut meta = HashMap::new();
                    meta.insert("image_path".to_string(), path_str.clone());
                    self.add_node_with_metadata(&path_str, node_name(path), "image", meta);
                } else {
                    self.add_node(&path_str, node_name(path), "file");
                }

                if !is_image_file(path) {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

                        match ext {
                            "rs" => {
                                for cap in re_rust_use.captures_iter(&content) {
                                    if let Some(m) = cap.get(1) {
                                        let import = m.as_str();
                                        let target = resolve_rust_import(path, import);
                                        if !target.is_empty() && target != path_str {
                                            self.ensure_node(&target, "file");
                                            self.add_edge(&path_str, &target, "imports");
                                        }
                                    }
                                }
                                for cap in re_rust_mod.captures_iter(&content) {
                                    if let Some(m) = cap.get(1) {
                                        let mod_name = m.as_str();
                                        let target = resolve_rust_mod(path, mod_name);
                                        if !target.is_empty() && target != path_str {
                                            self.ensure_node(&target, "file");
                                            self.add_edge(&path_str, &target, "contains");
                                        }
                                    }
                                }
                            }
                            "js" | "ts" | "jsx" | "tsx" | "mjs" => {
                                for cap in re_js_import.captures_iter(&content) {
                                    let import = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str());
                                    if let Some(imp) = import {
                                        let target = resolve_js_import(path, imp);
                                        if !target.is_empty() && target != path_str {
                                            self.ensure_node(&target, "file");
                                            self.add_edge(&path_str, &target, "imports");
                                        }
                                    }
                                }
                            }
                            "py" => {
                                for cap in re_py_import.captures_iter(&content) {
                                    let import = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str());
                                    if let Some(imp) = import {
                                        let target = resolve_py_import(path, imp);
                                        if !target.is_empty() && target != path_str {
                                            self.ensure_node(&target, "file");
                                            self.add_edge(&path_str, &target, "imports");
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if prefix.is_none() {
            if let Some(first) = allowed_paths.first() {
                let root = std::fs::canonicalize(first)
                    .unwrap_or_else(|_| Path::new(first).to_path_buf())
                    .to_string_lossy()
                    .to_string();
                self.graph.root = root;
            }
        }
    }

    /// Build the mindmap from The Vault items instead of filesystem.
    pub fn build_from_vault(
        &mut self,
        items: &[crate::db::VaultItem],
        relationships: &[crate::db::VaultRelationship],
    ) {
        self.graph.nodes.clear();
        self.graph.edges.clear();

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Root node
        self.add_node("vault:root", "The Vault".to_string(), "root");
        seen.insert("vault:root".to_string());
        self.graph.root = "vault:root".to_string();

        for item in items {
            let path = std::path::Path::new(&item.source_path);
            let mut current = "vault:root".to_string();

            // Build directory nodes from path ancestors
            if let Some(parent) = path.parent() {
                for component in parent.components() {
                    let comp_str = component.as_os_str().to_string_lossy().to_string();
                    let node_id = format!("dir:{}", comp_str);
                    if seen.insert(node_id.clone()) {
                        self.add_node(&node_id, comp_str.clone(), "dir");
                    }
                    self.add_edge(&current, &node_id, "contains");
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

            self.add_node_with_metadata(
                &item.source_path, label, node_type, meta
            );
            self.add_edge(&current, &item.source_path, "contains");
        }

        // Add relationship edges
        for rel in relationships {
            let source_id = items.iter().find(|i| i.id == rel.source_id)
                .map(|i| i.source_path.clone())
                .unwrap_or_else(|| format!("item_{}", rel.source_id));
            let target_id = items.iter().find(|i| i.id == rel.target_id)
                .map(|i| i.source_path.clone())
                .unwrap_or_else(|| format!("item_{}", rel.target_id));
            self.add_edge(
                &source_id, &target_id, &rel.relation_type
            );
        }
    }

    pub fn merge(&mut self, other: &MindMap) {
        for node in &other.nodes {
            if !self.graph.nodes.iter().any(|n| n.id == node.id) {
                self.graph.nodes.push(node.clone());
            }
        }
        for edge in &other.edges {
            if !self.graph.edges.iter().any(|e| {
                e.source == edge.source && e.target == edge.target && e.relation == edge.relation
            }) {
                self.graph.edges.push(edge.clone());
            }
        }
    }

    pub fn set_root(&mut self, root: &str) {
        self.graph.root = root.to_string();
    }

    pub fn truncated(&self, max_nodes: usize) -> MindMap {
        self.graph.truncated(max_nodes)
    }

    pub fn add_node(&mut self, id: &str, label: String, node_type: &str) {
        self.add_node_with_metadata(id, label, node_type, HashMap::new());
    }

    pub fn add_node_with_metadata(&mut self,
        id: &str,
        label: String,
        node_type: &str,
        metadata: HashMap<String, String>,
    ) {
        if self.graph.nodes.iter().any(|n| n.id == id) {
            return;
        }
        self.graph.nodes.push(MindMapNode {
            id: id.to_string(),
            label,
            node_type: node_type.to_string(),
            metadata,
        });
    }

    fn ensure_node(&mut self, id: &str, node_type: &str) {
        if !self.graph.nodes.iter().any(|n| n.id == id) {
            let label = Path::new(id)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| id.to_string());
            self.add_node(id, label, node_type);
        }
    }

    pub fn add_edge(&mut self, source: &str, target: &str, relation: &str) {
        if self.graph.edges.iter().any(|e| e.source == source && e.target == target && e.relation == relation) {
            return;
        }
        self.graph.edges.push(MindMapEdge {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
        });
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

fn is_source_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(ext, "rs" | "js" | "ts" | "jsx" | "tsx" | "mjs" | "py" | "java" | "go" | "c" | "cpp" | "h" | "hpp" | "md" | "json" | "toml" | "yaml" | "yml")
}

fn is_image_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "ico")
}

fn node_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn resolve_rust_import(path: &Path, import: &str) -> String {
    // Simple resolution: if import starts with "crate::", try to find the file
    let parent = path.parent().unwrap_or(Path::new(""));

    if import.starts_with("crate::") {
        let rel = import.trim_start_matches("crate::").replace("::", "/");
        let candidate = parent.join(format!("{}.rs", rel));
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
        let candidate_mod = parent.join(&rel).join("mod.rs");
        if candidate_mod.exists() {
            return candidate_mod.to_string_lossy().to_string();
        }
    }

    // Try std/external resolution - just return empty for now
    String::new()
}

fn resolve_rust_mod(path: &Path, mod_name: &str) -> String {
    let parent = path.parent().unwrap_or(Path::new(""));
    let candidate = parent.join(format!("{}.rs", mod_name));
    if candidate.exists() {
        return candidate.to_string_lossy().to_string();
    }
    let candidate_mod = parent.join(mod_name).join("mod.rs");
    if candidate_mod.exists() {
        return candidate_mod.to_string_lossy().to_string();
    }
    String::new()
}

fn resolve_js_import(path: &Path, import: &str) -> String {
    if import.starts_with('.') {
        let parent = path.parent().unwrap_or(Path::new(""));
        let resolved = parent.join(import);
        for ext in &["", ".js", ".ts", ".jsx", ".tsx", ".mjs", "/index.js", "/index.ts"] {
            let candidate = if ext.starts_with('/') {
                resolved.join(&ext[1..])
            } else {
                resolved.with_extension("").with_extension(ext.trim_start_matches('.'))
            };
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    String::new()
}

fn resolve_py_import(path: &Path, import: &str) -> String {
    let parent = path.parent().unwrap_or(Path::new(""));
    let parts: Vec<&str> = import.split('.').collect();

    // Try relative to current dir
    let mut candidate = parent.to_path_buf();
    for part in &parts {
        candidate = candidate.join(part);
    }

    let py_file = candidate.with_extension("py");
    if py_file.exists() {
        return py_file.to_string_lossy().to_string();
    }

    let init_file = candidate.join("__init__.py");
    if init_file.exists() {
        return init_file.to_string_lossy().to_string();
    }

    String::new()
}
