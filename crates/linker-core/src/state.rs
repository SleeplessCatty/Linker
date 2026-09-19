use std::fs::File;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{LinkerError, Result};

#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub local_path: String,
    pub cloud_path: String,
    pub status: String,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredFileState {
    pub relative_path: String,
    pub local_hash: Option<String>,
    pub local_mtime: Option<i64>,
    pub local_size: Option<i64>,
    pub cloud_hash: Option<String>,
    pub cloud_mtime: Option<i64>,
    pub cloud_size: Option<i64>,
    pub last_synced_hash: Option<String>,
    pub deleted: bool,
}

#[derive(Debug, Clone)]
pub struct FileStateUpdate {
    pub relative_path: String,
    pub local_hash: Option<String>,
    pub local_mtime: Option<i64>,
    pub local_size: Option<i64>,
    pub cloud_hash: Option<String>,
    pub cloud_mtime: Option<i64>,
    pub cloud_size: Option<i64>,
    pub last_synced_hash: Option<String>,
    pub deleted: bool,
}

pub struct NewItem<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub item_type: &'a str,
    pub local_path: &'a str,
    pub cloud_path: &'a str,
}

pub struct StateDb {
    conn: Connection,
    directory: PathBuf,
}

impl StateDb {
    /// Preview must not initialize or migrate state. Only synchronization lock
    /// files may be created when a caller subsequently previews an item.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(std::time::Duration::from_secs(30))?;
        let db = Self {
            conn,
            directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        };
        if !db.column_exists("items", "id")? || db.column_exists("items", "rule_path")? {
            return Err(std::io::Error::other("this command requires upgraded Linker state; back up and complete the schema-2 upgrade first (see INSTALL.md)").into());
        }
        Ok(db)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let _migration_lock = crate::lock::acquire(&directory.join("locks"), "migration")?;
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(30))?;
        crate::migration::upgrade(&mut conn, &directory)?;
        crate::migration::upgrade_shared_sources(&mut conn, &directory)?;
        let db = Self { conn, directory };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                item_type TEXT NOT NULL DEFAULT 'directory',
                local_path TEXT NOT NULL,
                cloud_path TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_sync_at INTEGER,
                last_error TEXT,
                UNIQUE(local_path, cloud_path)
            );

            CREATE INDEX IF NOT EXISTS items_source_path ON items(local_path);

            CREATE TABLE IF NOT EXISTS file_states (
                id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                local_hash TEXT,
                local_mtime INTEGER,
                local_size INTEGER,
                cloud_hash TEXT,
                cloud_mtime INTEGER,
                cloud_size INTEGER,
                last_synced_hash TEXT,
                last_synced_at INTEGER,
                deleted INTEGER NOT NULL DEFAULT 0,
                UNIQUE(item_id, relative_path),
                FOREIGN KEY (item_id) REFERENCES items(id)
            );
            "#,
        )?;
        if !self.column_exists("items", "item_type")? {
            self.conn.execute(
                "ALTER TABLE items ADD COLUMN item_type TEXT NOT NULL DEFAULT 'directory'",
                [],
            )?;
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (2, ?1)",
            [Utc::now().timestamp()],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (3, ?1)",
            [Utc::now().timestamp()],
        )?;
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for name in columns {
            if name? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn name_exists(&self, name: &str) -> Result<bool> {
        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM items WHERE name = ?1 LIMIT 1",
                params![name],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(exists)
    }

    pub fn insert_item(&mut self, item: NewItem<'_>) -> Result<()> {
        if self.name_exists(item.name)? {
            return Err(LinkerError::ItemExists(item.name.to_string()));
        }

        let now = Utc::now().timestamp();
        let tx = self.conn.transaction()?;
        tx.execute(
            r#"
            INSERT INTO items (
                id, name, item_type, local_path, cloud_path, status,
                created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)
            "#,
            params![
                item.id,
                item.name,
                item.item_type,
                item.local_path,
                item.cloud_path,
                now
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn list_items(&self) -> Result<Vec<Item>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                i.id,
                i.name,
                i.item_type,
                i.local_path,
                i.cloud_path,
                i.status,
                i.last_sync_at,
                i.last_error
            FROM items i
            ORDER BY i.name
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Item {
                id: row.get(0)?,
                name: row.get(1)?,
                item_type: row.get(2)?,
                local_path: row.get(3)?,
                cloud_path: row.get(4)?,
                status: row.get(5)?,
                last_sync_at: row.get(6)?,
                last_error: row.get(7)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn get_item(&self, name: &str) -> Result<Item> {
        self.list_items()?
            .into_iter()
            .find(|item| item.name == name || item.id == name)
            .ok_or_else(|| LinkerError::ItemNotFound(name.to_string()))
    }

    pub fn remove_item(&self, name: &str) -> Result<Item> {
        let item = self.get_item(name)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM file_states WHERE item_id = ?1",
            params![&item.id],
        )?;
        tx.execute("DELETE FROM items WHERE id = ?1", params![&item.id])?;
        tx.commit()?;
        Ok(item)
    }

    pub fn lock_item(&self, id: &str) -> Result<File> {
        crate::lock::acquire(&self.directory.join("locks"), &format!("item:{id}"))
    }

    /// Call after the item lock. All writers/previews sharing the canonical
    /// stored source path serialize, while unrelated sources remain independent.
    pub fn lock_source(&self, source: &str) -> Result<File> {
        crate::lock::acquire(&self.directory.join("locks"), &format!("source:{source}"))
    }

    pub(crate) fn lock_add(&self) -> Result<File> {
        crate::lock::acquire(&self.directory.join("locks"), "add-registry")
    }

    pub(crate) fn rollback_add(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM file_states WHERE item_id = ?1", [id])?;
        tx.execute("DELETE FROM items WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn forget_file_state(&self, item_id: &str, relative_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM file_states WHERE item_id = ?1 AND relative_path = ?2",
            params![item_id, relative_path],
        )?;
        Ok(())
    }

    pub fn list_file_states(&self, item_id: &str) -> Result<Vec<StoredFileState>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                relative_path,
                local_hash,
                local_mtime,
                local_size,
                cloud_hash,
                cloud_mtime,
                cloud_size,
                last_synced_hash,
                deleted
            FROM file_states
            WHERE item_id = ?1
            "#,
        )?;

        let rows = stmt.query_map(params![item_id], |row| {
            let deleted: i64 = row.get(8)?;
            Ok(StoredFileState {
                relative_path: row.get(0)?,
                local_hash: row.get(1)?,
                local_mtime: row.get(2)?,
                local_size: row.get(3)?,
                cloud_hash: row.get(4)?,
                cloud_mtime: row.get(5)?,
                cloud_size: row.get(6)?,
                last_synced_hash: row.get(7)?,
                deleted: deleted != 0,
            })
        })?;

        let mut states = Vec::new();
        for row in rows {
            states.push(row?);
        }
        Ok(states)
    }

    pub fn upsert_file_state(&self, item_id: &str, update: &FileStateUpdate) -> Result<()> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            r#"
            INSERT INTO file_states (
                id,
                item_id,
                relative_path,
                local_hash,
                local_mtime,
                local_size,
                cloud_hash,
                cloud_mtime,
                cloud_size,
                last_synced_hash,
                last_synced_at,
                deleted
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(item_id, relative_path) DO UPDATE SET
                local_hash = excluded.local_hash,
                local_mtime = excluded.local_mtime,
                local_size = excluded.local_size,
                cloud_hash = excluded.cloud_hash,
                cloud_mtime = excluded.cloud_mtime,
                cloud_size = excluded.cloud_size,
                last_synced_hash = excluded.last_synced_hash,
                last_synced_at = excluded.last_synced_at,
                deleted = excluded.deleted
            "#,
            params![
                uuid::Uuid::new_v4().to_string(),
                item_id,
                &update.relative_path,
                &update.local_hash,
                update.local_mtime,
                update.local_size,
                &update.cloud_hash,
                update.cloud_mtime,
                update.cloud_size,
                &update.last_synced_hash,
                now,
                if update.deleted { 1 } else { 0 }
            ],
        )?;
        Ok(())
    }

    pub fn mark_item_synced(&self, item_id: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            r#"
            UPDATE items
            SET status = 'active',
                updated_at = ?1,
                last_sync_at = ?1,
                last_error = NULL
            WHERE id = ?2
            "#,
            params![now, item_id],
        )?;
        Ok(())
    }

    pub fn mark_item_error(&self, item_id: &str, error: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            r#"
            UPDATE items
            SET status = 'error',
                updated_at = ?1,
                last_error = ?2
            WHERE id = ?3
            "#,
            params![now, error, item_id],
        )?;
        Ok(())
    }
}
