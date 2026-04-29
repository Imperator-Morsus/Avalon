use rusqlite::{Connection, Result as SqlResult, Row};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// Avalon Vault Database
// Single SQLite file with FTS5 for text search across documents and images.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultDb {
    conn: Connection,
}

impl VaultDb {
    /// Open (or create) the SQLite database next to the executable / project root.
    pub fn open(project_root: &std::path::Path) -> SqlResult<Self> {
        let db_path = project_root.join(".avalon.db");
        let conn = Connection::open(&db_path)?;
        let db = VaultDb { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            r#"
            -- Documents table (MindVault)
            CREATE TABLE IF NOT EXISTS vault_documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_path TEXT NOT NULL,
                title TEXT,
                content TEXT,
                content_type TEXT NOT NULL, -- 'text', 'pdf', 'markdown', 'code', 'html'
                size_bytes INTEGER,
                ingested_at TEXT NOT NULL,
                hash TEXT NOT NULL
            );

            -- FTS5 virtual table for documents
            CREATE VIRTUAL TABLE IF NOT EXISTS vault_fts USING fts5(
                title, content,
                content_rowid=rowid,
                content='vault_documents'
            );

            -- Images table (VisionVault)
            CREATE TABLE IF NOT EXISTS vision_images (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_path TEXT NOT NULL,
                description TEXT,
                tags TEXT,
                width INTEGER,
                height INTEGER,
                format TEXT,
                ingested_at TEXT NOT NULL,
                hash TEXT NOT NULL,
                confirmed INTEGER DEFAULT 0
            );

            -- FTS5 virtual table for images
            CREATE VIRTUAL TABLE IF NOT EXISTS vision_fts USING fts5(
                description, tags,
                content_rowid=rowid,
                content='vision_images'
            );

            -- Agents table
            CREATE TABLE IF NOT EXISTS agents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                display_name TEXT,
                role TEXT NOT NULL,
                description TEXT,
                system_prompt TEXT,
                allowed_tools TEXT NOT NULL,
                is_builtin INTEGER DEFAULT 0,
                created_at TEXT NOT NULL
            );

            -- Agent dispatches
            CREATE TABLE IF NOT EXISTS agent_dispatches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id INTEGER NOT NULL,
                task TEXT NOT NULL,
                status TEXT NOT NULL,
                result TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                FOREIGN KEY(agent_id) REFERENCES agents(id)
            );

            -- Agent board (inter-agent messaging)
            CREATE TABLE IF NOT EXISTS agent_board (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                dispatch_id INTEGER NOT NULL,
                author TEXT NOT NULL,
                channel TEXT NOT NULL DEFAULT 'general',
                content TEXT NOT NULL,
                posted_at TEXT NOT NULL,
                FOREIGN KEY(dispatch_id) REFERENCES agent_dispatches(id)
            );

            -- Agent memory (session summaries)
            CREATE TABLE IF NOT EXISTS agent_memory (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id INTEGER NOT NULL UNIQUE,
                summary TEXT NOT NULL DEFAULT '',
                session_count INTEGER DEFAULT 0,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(agent_id) REFERENCES agents(id)
            );

            -- Ensure FTS5 triggers exist for automatic index maintenance
            CREATE TRIGGER IF NOT EXISTS vault_fts_insert AFTER INSERT ON vault_documents BEGIN
                INSERT INTO vault_fts(rowid, title, content)
                VALUES (NEW.id, NEW.title, NEW.content);
            END;
            CREATE TRIGGER IF NOT EXISTS vault_fts_delete AFTER DELETE ON vault_documents BEGIN
                INSERT INTO vault_fts(vault_fts, rowid, title, content)
                VALUES ('delete', OLD.id, OLD.title, OLD.content);
            END;
            CREATE TRIGGER IF NOT EXISTS vault_fts_update AFTER UPDATE ON vault_documents BEGIN
                INSERT INTO vault_fts(vault_fts, rowid, title, content)
                VALUES ('delete', OLD.id, OLD.title, OLD.content);
                INSERT INTO vault_fts(rowid, title, content)
                VALUES (NEW.id, NEW.title, NEW.content);
            END;

            CREATE TRIGGER IF NOT EXISTS vision_fts_insert AFTER INSERT ON vision_images BEGIN
                INSERT INTO vision_fts(rowid, description, tags)
                VALUES (NEW.id, NEW.description, NEW.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS vision_fts_delete AFTER DELETE ON vision_images BEGIN
                INSERT INTO vision_fts(vision_fts, rowid, description, tags)
                VALUES ('delete', OLD.id, OLD.description, OLD.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS vision_fts_update AFTER UPDATE ON vision_images BEGIN
                INSERT INTO vision_fts(vision_fts, rowid, description, tags)
                VALUES ('delete', OLD.id, OLD.description, OLD.tags);
                INSERT INTO vision_fts(rowid, description, tags)
                VALUES (NEW.id, NEW.description, NEW.tags);
            END;
            "#,
        )
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Document (MindVault) Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_document(
        &self,
        source_path: &str,
        title: Option<&str>,
        content: &str,
        content_type: &str,
        size_bytes: usize,
        ingested_at: &str,
        hash: &str,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO vault_documents
             (source_path, title, content, content_type, size_bytes, ingested_at, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (source_path, title, content, content_type, size_bytes as i64, ingested_at, hash),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn search_documents(&self, query: &str, limit: usize) -> SqlResult<Vec<VaultDoc>> {
        let sql = "SELECT vd.* FROM vault_documents vd
                   JOIN vault_fts ON vd.id = vault_fts.rowid
                   WHERE vault_fts MATCH ?1
                   ORDER BY rank
                   LIMIT ?2";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map((query, limit as i64), |row| {
            Ok(VaultDoc {
                id: row.get(0)?,
                source_path: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                size_bytes: row.get::<_, i64>(5)? as usize,
                ingested_at: row.get(6)?,
                hash: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_document(&self, id: i64) -> SqlResult<Option<VaultDoc>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_path, title, content, content_type, size_bytes, ingested_at, hash
             FROM vault_documents WHERE id = ?1"
        )?;
        let row = stmt.query_row((id,), |row| {
            Ok(VaultDoc {
                id: row.get(0)?,
                source_path: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                size_bytes: row.get::<_, i64>(5)? as usize,
                ingested_at: row.get(6)?,
                hash: row.get(7)?,
            })
        });
        match row {
            Ok(doc) => Ok(Some(doc)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete_document(&self, id: i64) -> SqlResult<bool> {
        let changes = self.conn.execute("DELETE FROM vault_documents WHERE id = ?1", (id,))?;
        Ok(changes > 0)
    }

    pub fn document_exists_by_hash(&self, hash: &str) -> SqlResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM vault_documents WHERE hash = ?1",
            (hash,),
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Image (VisionVault) Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_image(
        &self,
        source_path: &str,
        description: Option<&str>,
        tags: Option<&str>,
        width: Option<i64>,
        height: Option<i64>,
        format: Option<&str>,
        ingested_at: &str,
        hash: &str,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO vision_images
             (source_path, description, tags, width, height, format, ingested_at, hash, confirmed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            (source_path, description, tags, width, height, format, ingested_at, hash),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn confirm_image_description(
        &self,
        id: i64,
        description: &str,
        tags: &str,
    ) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE vision_images SET description = ?1, tags = ?2, confirmed = 1 WHERE id = ?3",
            (description, tags, id),
        )?;
        Ok(())
    }

    pub fn search_images(&self, query: &str, limit: usize) -> SqlResult<Vec<VisionImage>> {
        let sql = "SELECT vi.* FROM vision_images vi
                   JOIN vision_fts ON vi.id = vision_fts.rowid
                   WHERE vision_fts MATCH ?1
                   ORDER BY rank
                   LIMIT ?2";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map((query, limit as i64), |row| {
            Ok(VisionImage {
                id: row.get(0)?,
                source_path: row.get(1)?,
                description: row.get(2)?,
                tags: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                format: row.get(6)?,
                ingested_at: row.get(7)?,
                hash: row.get(8)?,
                confirmed: row.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_image(&self, id: i64) -> SqlResult<Option<VisionImage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_path, description, tags, width, height, format, ingested_at, hash, confirmed
             FROM vision_images WHERE id = ?1"
        )?;
        let row = stmt.query_row((id,), |row| {
            Ok(VisionImage {
                id: row.get(0)?,
                source_path: row.get(1)?,
                description: row.get(2)?,
                tags: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                format: row.get(6)?,
                ingested_at: row.get(7)?,
                hash: row.get(8)?,
                confirmed: row.get(9)?,
            })
        });
        match row {
            Ok(img) => Ok(Some(img)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete_image(&self, id: i64) -> SqlResult<bool> {
        let changes = self.conn.execute("DELETE FROM vision_images WHERE id = ?1", (id,))?;
        Ok(changes > 0)
    }

    pub fn image_exists_by_hash(&self, hash: &str) -> SqlResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM vision_images WHERE hash = ?1",
            (hash,),
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Agent Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_agent(
        &self,
        name: &str,
        display_name: Option<&str>,
        role: &str,
        description: Option<&str>,
        system_prompt: Option<&str>,
        allowed_tools: &str,
        is_builtin: bool,
        created_at: &str,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO agents
             (name, display_name, role, description, system_prompt, allowed_tools, is_builtin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (name, display_name, role, description, system_prompt, allowed_tools, is_builtin as i64, created_at),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_agents(&self) -> SqlResult<Vec<AgentRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, display_name, role, description, system_prompt, allowed_tools, is_builtin, created_at
             FROM agents ORDER BY name"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                role: row.get(3)?,
                description: row.get(4)?,
                system_prompt: row.get(5)?,
                allowed_tools: row.get(6)?,
                is_builtin: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_agent_by_name(&self, name: &str) -> SqlResult<Option<AgentRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, display_name, role, description, system_prompt, allowed_tools, is_builtin, created_at
             FROM agents WHERE name = ?1"
        )?;
        let row = stmt.query_row((name,), |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                role: row.get(3)?,
                description: row.get(4)?,
                system_prompt: row.get(5)?,
                allowed_tools: row.get(6)?,
                is_builtin: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        });
        match row {
            Ok(a) => Ok(Some(a)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn update_agent(&self, name: &str, display_name: Option<&str>, role: Option<&str>,
                        description: Option<&str>, system_prompt: Option<&str>, allowed_tools: Option<&str>) -> SqlResult<bool> {
        let mut sets = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(v) = display_name {
            sets.push("display_name = ?".to_string());
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = role {
            sets.push("role = ?".to_string());
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = description {
            sets.push("description = ?".to_string());
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = system_prompt {
            sets.push("system_prompt = ?".to_string());
            params.push(Box::new(v.to_string()));
        }
        if let Some(v) = allowed_tools {
            sets.push("allowed_tools = ?".to_string());
            params.push(Box::new(v.to_string()));
        }
        if sets.is_empty() {
            return Ok(false);
        }
        params.push(Box::new(name.to_string()));
        let sql = format!("UPDATE agents SET {} WHERE name = ?", sets.join(", "));
        let changes = self.conn.execute(&sql, rusqlite::params_from_iter(params.iter()))?;
        Ok(changes > 0)
    }

    pub fn delete_agent(&self, name: &str) -> SqlResult<bool> {
        let changes = self.conn.execute(
            "DELETE FROM agents WHERE name = ?1 AND is_builtin = 0",
            (name,),
        )?;
        Ok(changes > 0)
    }

    pub fn insert_dispatch(&self, agent_id: i64, task: &str, status: &str, created_at: &str) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO agent_dispatches (agent_id, task, status, created_at) VALUES (?1, ?2, ?3, ?4)",
            (agent_id, task, status, created_at),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_dispatch(&self, id: i64) -> SqlResult<Option<DispatchRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent_id, task, status, result, error, created_at, completed_at
             FROM agent_dispatches WHERE id = ?1"
        )?;
        let row = stmt.query_row((id,), |row| {
            Ok(DispatchRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                task: row.get(2)?,
                status: row.get(3)?,
                result: row.get(4)?,
                error: row.get(5)?,
                created_at: row.get(6)?,
                completed_at: row.get(7)?,
            })
        });
        match row {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn update_dispatch_status(&self, id: i64, status: &str, result: Option<&str>, error: Option<&str>, completed_at: Option<&str>) -> SqlResult<bool> {
        let changes = self.conn.execute(
            "UPDATE agent_dispatches SET status = ?1, result = ?2, error = ?3, completed_at = ?4 WHERE id = ?5",
            (status, result, error, completed_at, id),
        )?;
        Ok(changes > 0)
    }

    pub fn insert_board_post(&self, dispatch_id: i64, author: &str, channel: &str, content: &str, posted_at: &str) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO agent_board (dispatch_id, author, channel, content, posted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            (dispatch_id, author, channel, content, posted_at),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_board_posts(&self, dispatch_id: i64, channel: Option<&str>, since: Option<&str>) -> SqlResult<Vec<BoardPost>> {
        let since_str = since.unwrap_or("1970-01-01T00:00:00Z");
        if let Some(ch) = channel {
            let mut stmt = self.conn.prepare(
                "SELECT id, dispatch_id, author, channel, content, posted_at
                 FROM agent_board WHERE dispatch_id = ?1 AND channel = ?2 AND posted_at > ?3 ORDER BY posted_at"
            )?;
            let rows = stmt.query_map((dispatch_id, ch, since_str), |row| {
                Ok(BoardPost {
                    id: row.get(0)?,
                    dispatch_id: row.get(1)?,
                    author: row.get(2)?,
                    channel: row.get(3)?,
                    content: row.get(4)?,
                    posted_at: row.get(5)?,
                })
            })?;
            rows.collect()
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, dispatch_id, author, channel, content, posted_at
                 FROM agent_board WHERE dispatch_id = ?1 AND posted_at > ?2 ORDER BY posted_at"
            )?;
            let rows = stmt.query_map((dispatch_id, since_str), |row| {
                Ok(BoardPost {
                    id: row.get(0)?,
                    dispatch_id: row.get(1)?,
                    author: row.get(2)?,
                    channel: row.get(3)?,
                    content: row.get(4)?,
                    posted_at: row.get(5)?,
                })
            })?;
            rows.collect()
        }
    }

    pub fn upsert_agent_memory(&self, agent_id: i64, summary: &str, session_count: i64, updated_at: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO agent_memory (agent_id, summary, session_count, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent_id) DO UPDATE SET
                 summary = excluded.summary,
                 session_count = excluded.session_count,
                 updated_at = excluded.updated_at",
            (agent_id, summary, session_count, updated_at),
        )?;
        Ok(())
    }

    pub fn get_agent_memory(&self, agent_id: i64) -> SqlResult<Option<AgentMemory>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent_id, summary, session_count, updated_at FROM agent_memory WHERE agent_id = ?1"
        )?;
        let row = stmt.query_row((agent_id,), |row| {
            Ok(AgentMemory {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                summary: row.get(2)?,
                session_count: row.get(3)?,
                updated_at: row.get(4)?,
            })
        });
        match row {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Data structs for query results
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultDoc {
    pub id: i64,
    pub source_path: String,
    pub title: Option<String>,
    pub content: String,
    pub content_type: String,
    pub size_bytes: usize,
    pub ingested_at: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionImage {
    pub id: i64,
    pub source_path: String,
    pub description: Option<String>,
    pub tags: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub format: Option<String>,
    pub ingested_at: String,
    pub hash: String,
    pub confirmed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub role: String,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub allowed_tools: String,
    pub is_builtin: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchRecord {
    pub id: i64,
    pub agent_id: i64,
    pub task: String,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardPost {
    pub id: i64,
    pub dispatch_id: i64,
    pub author: String,
    pub channel: String,
    pub content: String,
    pub posted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub id: i64,
    pub agent_id: i64,
    pub summary: String,
    pub session_count: i64,
    pub updated_at: String,
}

// Thread-safe shared handle
pub type SharedVaultDb = Arc<Mutex<VaultDb>>;
