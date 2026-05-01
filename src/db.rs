use rusqlite::{Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// Avalon Vault Database — Phase 1: Unified Knowledge Graph
// Replaces vault_documents + vision_images with vault_items + relationships.
// ═════════════════════════════════════════════════════════════════════════════

// ═════════════════════════════════════════════════════════════════════════════
// Access Tier — permission level for vault items
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessTier {
    Public,
    Restricted,
    Confidential,
    Secret,
}

impl AccessTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessTier::Public => "Public",
            AccessTier::Restricted => "Restricted",
            AccessTier::Confidential => "Confidential",
            AccessTier::Secret => "Secret",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Public" => Some(AccessTier::Public),
            "Restricted" => Some(AccessTier::Restricted),
            "Confidential" => Some(AccessTier::Confidential),
            "Secret" => Some(AccessTier::Secret),
            _ => None,
        }
    }

    /// Returns true if actor_level can see items at or below required_level.
    /// Hierarchy: Secret > Confidential > Restricted > Public
    pub fn can_see(&self, required: &AccessTier) -> bool {
        self.rank() >= required.rank()
    }

    fn rank(&self) -> u8 {
        match self {
            AccessTier::Public => 1,
            AccessTier::Restricted => 2,
            AccessTier::Confidential => 3,
            AccessTier::Secret => 4,
        }
    }
}

pub struct VaultDb {
    pub(crate) conn: Connection,
}

impl VaultDb {
    /// Open (or create) the SQLite database next to the executable / project root.
    pub fn open(project_root: &std::path::Path) -> SqlResult<Self> {
        let db_path = project_root.join(".avalon.db");
        let conn = Connection::open(&db_path)?;
        let db = VaultDb { conn };
        db.init_schema()?;
        db.migrate_from_legacy()?;
        db.migrate_access_tier()?;
        db.migrate_astrar_tracking()?;
        db.migrate_auth()?;
        Ok(db)
    }

    fn init_schema(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            r#"
            -- Unified vault items table
            CREATE TABLE IF NOT EXISTS vault_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_path TEXT NOT NULL,
                title TEXT,
                description TEXT,
                content TEXT,
                content_type TEXT NOT NULL,
                format TEXT,
                size_bytes INTEGER,
                width INTEGER,
                height INTEGER,
                duration_seconds INTEGER,
                ingested_at TEXT NOT NULL,
                hash TEXT NOT NULL,
                embedding_synced INTEGER DEFAULT 0,
                has_contradictions INTEGER DEFAULT 0,
                contradiction_summary TEXT,
                status TEXT DEFAULT 'current',
                version INTEGER DEFAULT 1,
                replaces_id INTEGER,
                metadata TEXT,
                access_tier TEXT NOT NULL DEFAULT 'Public',
                owner_id TEXT,
                FOREIGN KEY(replaces_id) REFERENCES vault_items(id) ON DELETE SET NULL
            );
            CREATE INDEX IF NOT EXISTS idx_items_source ON vault_items(source_path);
            CREATE INDEX IF NOT EXISTS idx_items_status ON vault_items(status);
            CREATE INDEX IF NOT EXISTS idx_items_type ON vault_items(content_type);

            -- FTS5 virtual table for unified search
            CREATE VIRTUAL TABLE IF NOT EXISTS vault_fts USING fts5(
                title, content, description,
                content_rowid=rowid,
                content='vault_items'
            );

            -- FTS5 triggers for automatic index maintenance
            CREATE TRIGGER IF NOT EXISTS vault_fts_insert AFTER INSERT ON vault_items BEGIN
                INSERT INTO vault_fts(rowid, title, content, description)
                VALUES (NEW.id, NEW.title, NEW.content, NEW.description);
            END;
            CREATE TRIGGER IF NOT EXISTS vault_fts_delete AFTER DELETE ON vault_items BEGIN
                INSERT INTO vault_fts(vault_fts, rowid, title, content, description)
                VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.description);
            END;
            CREATE TRIGGER IF NOT EXISTS vault_fts_update AFTER UPDATE ON vault_items BEGIN
                INSERT INTO vault_fts(vault_fts, rowid, title, content, description)
                VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.description);
                INSERT INTO vault_fts(rowid, title, content, description)
                VALUES (NEW.id, NEW.title, NEW.content, NEW.description);
            END;

            -- Relationships (graph edges)
            CREATE TABLE IF NOT EXISTS vault_relationships (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id INTEGER NOT NULL,
                target_id INTEGER NOT NULL,
                relation_type TEXT NOT NULL,
                confidence REAL DEFAULT 1.0,
                reason TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(source_id) REFERENCES vault_items(id) ON DELETE CASCADE,
                FOREIGN KEY(target_id) REFERENCES vault_items(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_rel_source ON vault_relationships(source_id);
            CREATE INDEX IF NOT EXISTS idx_rel_target ON vault_relationships(target_id);
            CREATE INDEX IF NOT EXISTS idx_rel_type ON vault_relationships(relation_type);

            -- Embeddings (semantic vectors)
            CREATE TABLE IF NOT EXISTS vault_embeddings (
                item_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(item_id) REFERENCES vault_items(id) ON DELETE CASCADE
            );

            -- Notifications
            CREATE TABLE IF NOT EXISTS vault_notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id INTEGER NOT NULL,
                notification_type TEXT NOT NULL,
                message TEXT NOT NULL,
                read INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                FOREIGN KEY(item_id) REFERENCES vault_items(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_notif_unread ON vault_notifications(read, created_at);

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
            "#,
        )
    }

    fn migrate_from_legacy(&self) -> SqlResult<()> {
        let old_docs: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vault_documents'",
            [], |r| r.get(0),
        ).unwrap_or(0);
        if old_docs == 0 {
            return Ok(());
        }

        // Migrate vault_documents -> vault_items
        let _ = self.conn.execute(
            "INSERT INTO vault_items (source_path, title, content, content_type, size_bytes, ingested_at, hash, status, version, metadata)
             SELECT source_path, title, content, content_type, size_bytes, ingested_at, hash, 'current', 1, NULL FROM vault_documents",
            [],
        );

        // Migrate vision_images -> vault_items
        let _ = self.conn.execute(
            "INSERT INTO vault_items (source_path, description, content_type, format, width, height, ingested_at, hash, status, version, metadata)
             SELECT source_path, description, 'image', format, width, height, ingested_at, hash, 'current', 1, json_object('confirmed', confirmed, 'tags', tags) FROM vision_images",
            [],
        );

        // Drop old tables
        let _ = self.conn.execute_batch(
            "DROP TABLE IF EXISTS vault_documents;
             DROP TABLE IF EXISTS vision_images;
             DROP TABLE IF EXISTS vault_fts;
             DROP TABLE IF EXISTS vision_fts;"
        );

        Ok(())
    }

    fn migrate_access_tier(&self) -> SqlResult<()> {
        // Check if access_tier column already exists
        let has_column: i64 = self.conn.query_row(
            "SELECT count(*) FROM pragma_table_info('vault_items') WHERE name='access_tier'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);
        if has_column > 0 {
            return Ok(()); // already migrated
        }

        // Add access_tier and owner_id columns
        let _ = self.conn.execute(
            "ALTER TABLE vault_items ADD COLUMN access_tier TEXT NOT NULL DEFAULT 'Public'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE vault_items ADD COLUMN owner_id TEXT",
            [],
        );

        // Add indexes for new columns
        let _ = self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_items_access_tier ON vault_items(access_tier)",
            [],
        );
        let _ = self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_items_owner ON vault_items(owner_id)",
            [],
        );

        Ok(())
    }

    fn migrate_astrar_tracking(&self) -> SqlResult<()> {
        // Add Astra tracking columns: concept extraction and contradiction check timestamps
        for col_info in [
            ("concept_extracted_at", "TEXT"),
            ("contradiction_checked_at", "TEXT"),
            ("last_processed_by", "TEXT"),
        ] {
            let has_col: i64 = self.conn.query_row(
                &format!("SELECT count(*) FROM pragma_table_info('vault_items') WHERE name='{}'", col_info.0),
                [],
                |r| r.get(0),
            ).unwrap_or(0);
            if has_col == 0 {
                let _ = self.conn.execute(
                    &format!("ALTER TABLE vault_items ADD COLUMN {} {}", col_info.0, col_info.1),
                    [],
                );
            }
        }
        Ok(())
    }

    fn migrate_auth(&self) -> SqlResult<()> {
        // --- users table ---
        let has_users_table: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='users'",
            [], |r| r.get(0)
        ).unwrap_or(0);

        if has_users_table == 0 {
            self.conn.execute_batch(r#"
            CREATE TABLE users (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                username     TEXT NOT NULL UNIQUE,
                display_name TEXT,
                password_hash TEXT NOT NULL,
                role         TEXT NOT NULL DEFAULT 'user',
                is_active    INTEGER NOT NULL DEFAULT 1,
                created_at   TEXT NOT NULL,
                last_login   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
            "#)?;
        }

        // --- sessions table ---
        let has_sessions_table: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
            [], |r| r.get(0)
        ).unwrap_or(0);

        if has_sessions_table == 0 {
            self.conn.execute_batch(r#"
            CREATE TABLE sessions (
                id          BLOB PRIMARY KEY,
                user_id     INTEGER NOT NULL,
                created_at  TEXT NOT NULL,
                expires_at  TEXT NOT NULL,
                ip_address  TEXT,
                user_agent  TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
            "#)?;
        }

        // --- login_attempts table (for rate limiting) ---
        let has_login_attempts: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='login_attempts'",
            [], |r| r.get(0)
        ).unwrap_or(0);

        if has_login_attempts == 0 {
            self.conn.execute_batch(r#"
            CREATE TABLE login_attempts (
                ip_address   TEXT NOT NULL,
                attempted_at TEXT NOT NULL,
                username     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_attempts_ip ON login_attempts(ip_address);
            CREATE INDEX IF NOT EXISTS idx_attempts_time ON login_attempts(attempted_at);
            "#)?;
        }

        Ok(())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Vault Item Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_item(
        &self,
        source_path: &str,
        title: Option<&str>,
        description: Option<&str>,
        content: &str,
        content_type: &str,
        format: Option<&str>,
        size_bytes: Option<usize>,
        width: Option<i64>,
        height: Option<i64>,
        duration_seconds: Option<i64>,
        ingested_at: &str,
        hash: &str,
        metadata: Option<&str>,
        access_tier: &str,
        owner_id: Option<&str>,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO vault_items
             (source_path, title, description, content, content_type, format, size_bytes, width, height, duration_seconds, ingested_at, hash, metadata, access_tier, owner_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            (source_path, title, description, content, content_type, format, size_bytes.map(|v| v as i64), width, height, duration_seconds, ingested_at, hash, metadata, access_tier, owner_id),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn search_items(&self, query: &str, limit: usize) -> SqlResult<Vec<VaultItem>> {
        let sql = "SELECT vi.* FROM vault_items vi
                   JOIN vault_fts ON vi.id = vault_fts.rowid
                   WHERE vault_fts MATCH ?1
                   ORDER BY rank
                   LIMIT ?2";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map((query, limit as i64), |row| Ok(row_to_vault_item(row)))?;
        rows.collect()
    }

    pub fn get_item(&self, id: i64) -> SqlResult<Option<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM vault_items WHERE id = ?1"
        )?;
        let row = stmt.query_row((id,), |row| Ok(row_to_vault_item(row)));
        match row {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn find_item_by_source_path(&self, source_path: &str) -> SqlResult<Option<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM vault_items WHERE source_path = ?1 ORDER BY version DESC LIMIT 1"
        )?;
        let row = stmt.query_row((source_path,), |row| Ok(row_to_vault_item(row)));
        match row {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn list_items_by_type(&self, content_type: &str) -> SqlResult<Vec<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM vault_items WHERE content_type = ?1 ORDER BY ingested_at DESC"
        )?;
        let rows = stmt.query_map((content_type,), |row| Ok(row_to_vault_item(row)))?;
        rows.collect()
    }

    pub fn list_items_needing_embeddings(&self, limit: usize) -> SqlResult<Vec<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM vault_items WHERE embedding_synced = 0 LIMIT ?1"
        )?;
        let rows = stmt.query_map((limit as i64,), |row| Ok(row_to_vault_item(row)))?;
        rows.collect()
    }

    /// List items that need concept extraction (never been processed).
    pub fn list_items_needing_concepts(&self, limit: usize) -> SqlResult<Vec<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM vault_items
             WHERE concept_extracted_at IS NULL
               AND content_type NOT IN ('concept', 'image', 'video', 'audio')
               AND (content IS NOT NULL AND length(content) > 50)
             ORDER BY ingested_at DESC
             LIMIT ?1"
        )?;
        let rows = stmt.query_map((limit as i64,), |row| Ok(row_to_vault_item(row)))?;
        rows.collect()
    }

    /// List items that have a newer version but haven't been checked for contradictions.
    pub fn list_versioned_items_unchecked(&self, limit: usize) -> SqlResult<Vec<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT vi.* FROM vault_items vi
             JOIN vault_relationships vr ON vi.id = vr.source_id AND vr.relation_type = 'newer_version'
             WHERE vi.contradiction_checked_at IS NULL
             ORDER BY vi.ingested_at DESC
             LIMIT ?1"
        )?;
        let rows = stmt.query_map((limit as i64,), |row| Ok(row_to_vault_item(row)))?;
        rows.collect()
    }

    /// Update the concept_extracted_at timestamp for an item.
    pub fn mark_concepts_extracted(&self, id: i64, processed_by: &str) -> SqlResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE vault_items SET concept_extracted_at = ?1, last_processed_by = ?2 WHERE id = ?3",
            (&now, processed_by, id),
        )?;
        Ok(())
    }

    /// Update the contradiction_checked_at timestamp for an item.
    pub fn mark_contradiction_checked(&self, id: i64, processed_by: &str) -> SqlResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE vault_items SET contradiction_checked_at = ?1, last_processed_by = ?2 WHERE id = ?3",
            (&now, processed_by, id),
        )?;
        Ok(())
    }

    pub fn list_all_items(&self) -> SqlResult<Vec<VaultItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM vault_items ORDER BY ingested_at DESC"
        )?;
        let rows = stmt.query_map([], |row| Ok(row_to_vault_item(row)))?;
        rows.collect()
    }

    pub fn delete_item(&self, id: i64) -> SqlResult<bool> {
        let changes = self.conn.execute("DELETE FROM vault_items WHERE id = ?1", (id,))?;
        Ok(changes > 0)
    }

    pub fn item_exists_by_hash(&self, hash: &str) -> SqlResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM vault_items WHERE hash = ?1",
            (hash,),
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn update_item_embedding_synced(&self, id: i64, synced: bool) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE vault_items SET embedding_synced = ?1 WHERE id = ?2",
            (synced as i64, id),
        )?;
        Ok(())
    }

    pub fn update_item_status(&self, id: i64, status: &str) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE vault_items SET status = ?1 WHERE id = ?2",
            (status, id),
        )?;
        Ok(())
    }

    pub fn update_item_contradiction(&self, id: i64, has: bool, summary: Option<&str>) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE vault_items SET has_contradictions = ?1, contradiction_summary = ?2 WHERE id = ?3",
            (has as i64, summary, id),
        )?;
        Ok(())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Relationship Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_relationship(
        &self,
        source_id: i64,
        target_id: i64,
        relation_type: &str,
        confidence: f64,
        reason: Option<&str>,
        created_at: &str,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO vault_relationships (source_id, target_id, relation_type, confidence, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (source_id, target_id, relation_type, confidence, reason, created_at),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_relationships_for_item(&self, id: i64, rel_type: Option<&str>) -> SqlResult<Vec<VaultRelationship>> {
        if let Some(rt) = rel_type {
            let mut stmt = self.conn.prepare(
                "SELECT id, source_id, target_id, relation_type, confidence, reason, created_at
                 FROM vault_relationships WHERE source_id = ?1 AND relation_type = ?2 ORDER BY created_at DESC"
            )?;
            let rows = stmt.query_map((id, rt), |row| Ok(row_to_relationship(row)))?;
            rows.collect()
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, source_id, target_id, relation_type, confidence, reason, created_at
                 FROM vault_relationships WHERE source_id = ?1 OR target_id = ?1 ORDER BY created_at DESC"
            )?;
            let rows = stmt.query_map((id, id), |row| Ok(row_to_relationship(row)))?;
            rows.collect()
        }
    }

    pub fn delete_relationships_for_item(&self, id: i64) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM vault_relationships WHERE source_id = ?1 OR target_id = ?1",
            (id,),
        )?;
        Ok(())
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Embedding Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_embedding(&self, item_id: i64, embedding: &[u8], model: &str, created_at: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO vault_embeddings (item_id, embedding, model, created_at) VALUES (?1, ?2, ?3, ?4)",
            (item_id, embedding, model, created_at),
        )?;
        Ok(())
    }

    pub fn get_embedding(&self, item_id: i64) -> SqlResult<Option<VaultEmbedding>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_id, embedding, model, created_at FROM vault_embeddings WHERE item_id = ?1"
        )?;
        let row = stmt.query_row((item_id,), |row| {
            Ok(VaultEmbedding {
                item_id: row.get(0)?,
                embedding: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
            })
        });
        match row {
            Ok(e) => Ok(Some(e)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete_embedding(&self, item_id: i64) -> SqlResult<()> {
        self.conn.execute("DELETE FROM vault_embeddings WHERE item_id = ?1", (item_id,))?;
        Ok(())
    }

    pub fn all_embeddings(&self) -> SqlResult<Vec<VaultEmbedding>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_id, embedding, model, created_at FROM vault_embeddings"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(VaultEmbedding {
                item_id: row.get(0)?,
                embedding: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Notification Operations
    // ═════════════════════════════════════════════════════════════════════════

    pub fn insert_notification(
        &self,
        item_id: i64,
        notification_type: &str,
        message: &str,
        created_at: &str,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO vault_notifications (item_id, notification_type, message, created_at) VALUES (?1, ?2, ?3, ?4)",
            (item_id, notification_type, message, created_at),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_unread_notifications(&self, limit: usize) -> SqlResult<Vec<VaultNotification>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_id, notification_type, message, read, created_at
             FROM vault_notifications WHERE read = 0 ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map((limit as i64,), |row| {
            Ok(VaultNotification {
                id: row.get(0)?,
                item_id: row.get(1)?,
                notification_type: row.get(2)?,
                message: row.get(3)?,
                read: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn mark_notification_read(&self, id: i64) -> SqlResult<bool> {
        let changes = self.conn.execute(
            "UPDATE vault_notifications SET read = 1 WHERE id = ?1",
            (id,),
        )?;
        Ok(changes > 0)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Agent Operations (unchanged from legacy)
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
// Row mappers
// ═════════════════════════════════════════════════════════════════════════════

pub(crate) fn row_to_vault_item(row: &rusqlite::Row) -> VaultItem {
    VaultItem {
        id: row.get(0).unwrap_or_default(),
        source_path: row.get(1).unwrap_or_default(),
        title: row.get(2).ok(),
        description: row.get(3).ok(),
        content: row.get(4).unwrap_or_default(),
        content_type: row.get(5).unwrap_or_default(),
        format: row.get(6).ok(),
        size_bytes: row.get::<_, Option<i64>>(7).unwrap_or(None).map(|v| v as usize),
        width: row.get(8).ok(),
        height: row.get(9).ok(),
        duration_seconds: row.get(10).ok(),
        ingested_at: row.get(11).unwrap_or_default(),
        hash: row.get(12).unwrap_or_default(),
        embedding_synced: row.get::<_, i64>(13).unwrap_or(0) != 0,
        has_contradictions: row.get::<_, i64>(14).unwrap_or(0) != 0,
        contradiction_summary: row.get(15).ok(),
        status: row.get(16).unwrap_or_else(|_| "current".to_string()),
        version: row.get(17).unwrap_or(1),
        replaces_id: row.get(18).ok(),
        metadata: row.get(19).ok(),
        access_tier: row.get(20).unwrap_or_else(|_| "Public".to_string()),
        owner_id: row.get(21).ok(),
        concept_extracted_at: row.get(22).ok(),
        contradiction_checked_at: row.get(23).ok(),
        last_processed_by: row.get(24).ok(),
    }
}

fn row_to_relationship(row: &rusqlite::Row) -> VaultRelationship {
    VaultRelationship {
        id: row.get(0).unwrap_or_default(),
        source_id: row.get(1).unwrap_or_default(),
        target_id: row.get(2).unwrap_or_default(),
        relation_type: row.get(3).unwrap_or_default(),
        confidence: row.get(4).unwrap_or(1.0),
        reason: row.get(5).ok(),
        created_at: row.get(6).unwrap_or_default(),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Data structs for query results
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultItem {
    pub id: i64,
    pub source_path: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content: String,
    pub content_type: String,
    pub format: Option<String>,
    pub size_bytes: Option<usize>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_seconds: Option<i64>,
    pub ingested_at: String,
    pub hash: String,
    pub embedding_synced: bool,
    pub has_contradictions: bool,
    pub contradiction_summary: Option<String>,
    pub status: String,
    pub version: i64,
    pub replaces_id: Option<i64>,
    pub metadata: Option<String>,
    pub access_tier: String,
    pub owner_id: Option<String>,
    pub concept_extracted_at: Option<String>,
    pub contradiction_checked_at: Option<String>,
    pub last_processed_by: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════
// User and Session Records (Auth Phase A)
// ═════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: String,
    pub last_login: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: Vec<u8>,       // 32-byte token hash
    pub user_id: i64,
    pub created_at: String,
    pub expires_at: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

pub(crate) fn row_to_user(row: &rusqlite::Row) -> UserRecord {
    UserRecord {
        id: row.get(0).unwrap_or_default(),
        username: row.get(1).unwrap_or_default(),
        display_name: row.get(2).ok(),
        password_hash: row.get(3).unwrap_or_default(),
        role: row.get(4).unwrap_or_else(|_| "user".to_string()),
        is_active: row.get::<_, i64>(5).unwrap_or(1) != 0,
        created_at: row.get(6).unwrap_or_default(),
        last_login: row.get(7).ok(),
    }
}

fn row_to_session(row: &rusqlite::Row) -> SessionRecord {
    SessionRecord {
        id: row.get(0).unwrap_or_default(),
        user_id: row.get(1).unwrap_or_default(),
        created_at: row.get(2).unwrap_or_default(),
        expires_at: row.get(3).unwrap_or_default(),
        ip_address: row.get(4).ok(),
        user_agent: row.get(5).ok(),
    }
}

// ═════════════════════════════════════════════════════════════════════════
// User CRUD
// ═════════════════════════════════════════════════════════════════════════

impl VaultDb {
    pub fn insert_user(
    &self,
    username: &str,
    display_name: Option<&str>,
    password_hash: &str,
    role: &str,
    created_at: &str,
) -> SqlResult<i64> {
    self.conn.execute(
        "INSERT INTO users (username, display_name, password_hash, role, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        (username, display_name, password_hash, role, created_at),
    )?;
    Ok(self.conn.last_insert_rowid())
}

    pub fn get_user_by_username(&self, username: &str) -> SqlResult<Option<UserRecord>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, username, display_name, password_hash, role, is_active, created_at, last_login
         FROM users WHERE username = ?1"
    )?;
    let row = stmt.query_row((username,), |row| Ok(row_to_user(row)));
    match row {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

    pub fn get_user_by_id(&self, id: i64) -> SqlResult<Option<UserRecord>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, username, display_name, password_hash, role, is_active, created_at, last_login
         FROM users WHERE id = ?1"
    )?;
    let row = stmt.query_row((id,), |row| Ok(row_to_user(row)));
    match row {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

    pub fn update_last_login(&self, user_id: i64, login_at: &str) -> SqlResult<()> {
    self.conn.execute(
        "UPDATE users SET last_login = ?1 WHERE id = ?2",
        (login_at, user_id),
    )?;
    Ok(())
}

    pub fn list_users(&self) -> SqlResult<Vec<UserRecord>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, username, display_name, password_hash, role, is_active, created_at, last_login
         FROM users ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map([], |row| Ok(row_to_user(row)))?;
    rows.collect()
}

// ═════════════════════════════════════════════════════════════════════════
// Session CRUD
// ═════════════════════════════════════════════════════════════════════════

    pub fn create_session(
    &self,
    token_hash: &[u8],
    user_id: i64,
    created_at: &str,
    expires_at: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> SqlResult<()> {
    self.conn.execute(
        "INSERT INTO sessions (id, user_id, created_at, expires_at, ip_address, user_agent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (token_hash, user_id, created_at, expires_at, ip_address, user_agent),
    )?;
    Ok(())
}

    pub fn get_valid_session(&self, token_hash: &[u8], now: &str) -> SqlResult<Option<SessionRecord>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, user_id, created_at, expires_at, ip_address, user_agent
         FROM sessions WHERE id = ?1 AND expires_at > ?2"
    )?;
    let row = stmt.query_row((token_hash, now), |row| Ok(row_to_session(row)));
    match row {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

    pub fn delete_session(&self, token_hash: &[u8]) -> SqlResult<bool> {
    let changes = self.conn.execute("DELETE FROM sessions WHERE id = ?1", (token_hash,))?;
    Ok(changes > 0)
}

    pub fn delete_user_sessions(&self, user_id: i64) -> SqlResult<()> {
    self.conn.execute("DELETE FROM sessions WHERE user_id = ?1", (user_id,))?;
    Ok(())
}

    pub fn touch_session(&self, token_hash: &[u8], new_expires_at: &str) -> SqlResult<bool> {
    let changes = self.conn.execute(
        "UPDATE sessions SET expires_at = ?1 WHERE id = ?2",
        (new_expires_at, token_hash),
    )?;
    Ok(changes > 0)
}

    pub fn purge_expired_sessions(&self, now: &str) -> SqlResult<u64> {
    let changes = self.conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", (now,))?;
    Ok(changes as u64)
}

// ═════════════════════════════════════════════════════════════════════════
// Rate Limiting
// ═════════════════════════════════════════════════════════════════════════

    pub fn record_login_attempt(
    &self,
    ip_address: &str,
    attempted_at: &str,
    username: Option<&str>,
) -> SqlResult<()> {
    self.conn.execute(
        "INSERT INTO login_attempts (ip_address, attempted_at, username) VALUES (?1, ?2, ?3)",
        (ip_address, attempted_at, username),
    )?;
    Ok(())
}

    pub fn count_recent_login_attempts(&self, ip_address: &str, since: &str) -> SqlResult<i64> {
    let count: i64 = self.conn.query_row(
        "SELECT count(*) FROM login_attempts WHERE ip_address = ?1 AND attempted_at >= ?2",
        (ip_address, since),
        |r| r.get(0),
    )?;
    Ok(count)
    }

    pub fn purge_old_login_attempts(&self, before: &str) -> SqlResult<u64> {
    let changes = self.conn.execute("DELETE FROM login_attempts WHERE attempted_at < ?1", (before,))?;
    Ok(changes as u64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRelationship {
    pub id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub relation_type: String,
    pub confidence: f64,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEmbedding {
    pub item_id: i64,
    pub embedding: Vec<u8>,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNotification {
    pub id: i64,
    pub item_id: i64,
    pub notification_type: String,
    pub message: String,
    pub read: bool,
    pub created_at: String,
}

// Legacy structs (kept for agent/dispatch compatibility)
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
