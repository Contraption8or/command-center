//! SQLite connection, data location and schema migrations.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Ordered list of migrations. Index + 1 == schema version (`PRAGMA user_version`).
/// Append new files here; never edit a migration that has shipped.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_task_steps.sql"),
    include_str!("migrations/003_event_color.sql"),
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

/// The standard data folder: `%APPDATA%\CommandCenter\data` on Windows.
pub fn standard_data_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "CommandCenter")
        .context("could not determine the user data directory")?;
    Ok(dirs.data_dir().to_path_buf())
}

/// Small text file holding the data folder you picked in Settings (absent = standard).
/// It lives one level above the standard data folder, in `%APPDATA%\CommandCenter`.
pub fn location_file() -> Result<PathBuf> {
    let std_dir = standard_data_dir()?;
    Ok(std_dir.parent().unwrap_or(&std_dir).join("data-folder.txt"))
}

/// Pick the data folder: the `COMMAND_CENTER_DATA` override, else the folder chosen in
/// Settings, else the standard one.
fn resolve_data_dir(env: Option<String>, chosen: Option<String>, standard: PathBuf) -> PathBuf {
    let clean = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    clean(env).or_else(|| clean(chosen)).map(PathBuf::from).unwrap_or(standard)
}

/// The data folder this run uses.
pub fn default_data_dir() -> Result<PathBuf> {
    let chosen = location_file().ok().and_then(|f| std::fs::read_to_string(f).ok());
    Ok(resolve_data_dir(std::env::var(DATA_DIR_ENV).ok(), chosen, standard_data_dir()?))
}

/// Whether a data folder was picked in Settings (instead of the standard one).
pub fn data_dir_is_custom() -> bool {
    location_file().map(|f| f.exists()).unwrap_or(false)
}

/// What [`Db::move_to`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum MoveOutcome {
    /// Your data was copied into the new folder.
    Copied,
    /// The folder already had Command Center data; it will be used as it is.
    UsedExisting,
}

/// Remember `dir` as the data folder for future launches (`None` = back to standard).
pub fn remember_data_dir(dir: Option<&Path>) -> Result<()> {
    let file = location_file()?;
    match dir {
        Some(d) => {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file, d.to_string_lossy().as_bytes()).with_context(|| format!("writing {}", file.display()))?;
        }
        None => {
            if file.exists() {
                std::fs::remove_file(&file)?;
            }
        }
    }
    Ok(())
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

    /// Prepare `dir` as the data folder: copy this database there, unless the folder
    /// already holds Command Center data (then that data is kept and used). The current
    /// folder is left untouched either way. Call [`remember_data_dir`] afterwards.
    pub fn move_to(&self, dir: &Path) -> Result<MoveOutcome> {
        let dest = dir.join(DB_FILE);
        if dest.exists() {
            // Make sure it really is a Command Center database before switching to it.
            Db::open(&dest).with_context(|| format!("{} is not usable", dest.display()))?;
            return Ok(MoveOutcome::UsedExisting);
        }
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        self.export_to(&dest)?;
        Ok(MoveOutcome::Copied)
    }

    fn configure(&self) -> Result<()> {
        self.conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        // Wait for a lock instead of failing at once if another connection (a second copy
        // of the app, a backup) is writing.
        self.conn.busy_timeout(std::time::Duration::from_secs(5))?;
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

    /// Fold the write-ahead log into the main file (on close), so the .db file alone is
    /// complete even if the -wal file is later lost.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
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
    fn data_dir_resolution() {
        let std_dir = PathBuf::from(r"C:\std");
        assert_eq!(resolve_data_dir(None, None, std_dir.clone()), std_dir);
        assert_eq!(resolve_data_dir(None, Some("  E:\\Mine\n".into()), std_dir.clone()), PathBuf::from(r"E:\Mine"));
        assert_eq!(resolve_data_dir(None, Some("   ".into()), std_dir.clone()), std_dir, "blank file = standard");
        assert_eq!(resolve_data_dir(Some(r"D:\test".into()), Some(r"E:\Mine".into()), std_dir), PathBuf::from(r"D:\test"));
    }

    #[test]
    fn move_copies_then_reuses() {
        let tmp = std::env::temp_dir().join(format!("cc-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let src = Db::open(&tmp.join("a").join(DB_FILE).tap_mkdir()).unwrap();
        src.set_setting("marker", "yes").unwrap();
        let target = tmp.join("b");
        assert_eq!(src.move_to(&target).unwrap(), MoveOutcome::Copied);
        let copy = Db::open(&target.join(DB_FILE)).unwrap();
        assert_eq!(copy.setting("marker").as_deref(), Some("yes"));
        drop(copy);
        assert_eq!(src.move_to(&target).unwrap(), MoveOutcome::UsedExisting, "never overwrites existing data");
        drop(src);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    trait TapMkdir {
        fn tap_mkdir(self) -> PathBuf;
    }
    impl TapMkdir for PathBuf {
        fn tap_mkdir(self) -> PathBuf {
            std::fs::create_dir_all(self.parent().unwrap()).unwrap();
            self
        }
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
