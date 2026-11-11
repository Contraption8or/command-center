//! SQLite connection, data location and schema migrations.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Ordered list of migrations. Index + 1 == schema version (`PRAGMA user_version`).
/// Append new files here; never edit a migration that has shipped.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_task_steps.sql"),
];

/// Environment variable that overrides the data directory (handy for testing).
pub const DATA_DIR_ENV: &str = "COMMAND_CENTER_DATA";
pub const DB_FILE: &str = "command-center.db";

/// `%text%` for a `LIKE … ESCAPE '\'` clause, with `%`, `_` and `\` in `text` matched literally.
pub fn like_pattern(text: &str) -> String {
    let escaped = text.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

pub struct Db {
    pub conn: Connection,
    path: PathBuf,
}

/// `%APPDATA%\CommandCenter\data` on Windows, unless overridden by `COMMAND_CENTER_DATA`.
pub fn default_data_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var(DATA_DIR_ENV) {
        return Ok(PathBuf::from(dir));
    }
    let dirs = directories::ProjectDirs::from("", "", "CommandCenter")
        .context("could not determine the user data directory")?;
    Ok(dirs.data_dir().to_path_buf())
}

impl Db {
    pub fn open_default() -> Result<Self> {
        let dir = default_data_dir()?;
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Self::open(&dir.join(DB_FILE))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut db = Db { conn, path: path.to_path_buf() };
        db.configure()?;
        db.migrate()?;
        Ok(db)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self> {
        let mut db = Db { conn: Connection::open_in_memory()?, path: PathBuf::new() };
        db.configure()?;
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn configure(&self) -> Result<()> {
        self.conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<usize> {
        let v: i64 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        Ok(v as usize)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()?;
        if current > MIGRATIONS.len() {
            anyhow::bail!(
                "database schema v{current} is newer than this app (v{}); update the app",
                MIGRATIONS.len()
            );
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    // ---- settings -------------------------------------------------------

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }

    pub fn count(&self, sql: &str) -> i64 {
        self.conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_fresh_db() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len());
    }

    #[test]
    fn settings_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting("theme"), None);
        db.set_setting("theme", "dark").unwrap();
        db.set_setting("theme", "light").unwrap();
        assert_eq!(db.setting("theme").as_deref(), Some("light"));
    }
}
