//! Single-file export / restore. A backup is a complete, standalone SQLite database.

use crate::db::Db;
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags, MAIN_DB};
use std::path::Path;

/// Suggested file name, e.g. `command-center-2026-11-11.ccbackup`.
pub fn suggested_name(today: chrono::NaiveDate) -> String {
    format!("command-center-{today}.ccbackup")
}

const AUTO_PREFIX: &str = "auto-";
const AUTO_KEY: &str = "backup.last_auto";
/// How many daily automatic backups to keep.
pub const AUTO_KEEP: usize = 7;

/// Once per day: write `<dir>/auto-YYYY-MM-DD.ccbackup` and delete automatic backups beyond
/// the newest [`AUTO_KEEP`]. Manual exports and pre-restore copies are never touched.
/// Returns the new file, or `None` if today's backup already exists.
pub fn auto_backup(db: &Db, dir: &Path, today: chrono::NaiveDate) -> Result<Option<std::path::PathBuf>> {
    let stamp = today.to_string();
    if db.setting(AUTO_KEY).as_deref() == Some(stamp.as_str()) {
        return Ok(None);
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = dir.join(format!("{AUTO_PREFIX}{stamp}.ccbackup"));
    db.export_to(&file)?;
    db.set_setting(AUTO_KEY, &stamp)?;

    let mut autos: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(AUTO_PREFIX) && n.ends_with(".ccbackup"))
        })
        .collect();
    autos.sort(); // ISO dates sort chronologically
    let excess = autos.len().saturating_sub(AUTO_KEEP);
    for old in &autos[..excess] {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(file))
}

/// Folder for automatic and safety backups, next to the live database.
pub fn backup_dir(db: &Db) -> std::path::PathBuf {
    db.path().parent().map(|p| p.join("backups")).unwrap_or_default()
}

impl Db {
    /// Write a consistent snapshot of the whole database to `dest` (overwrites).
    pub fn export_to(&self, dest: &Path) -> Result<()> {
        if dest == self.path() {
            bail!("cannot export over the live database");
        }
        if dest.exists() {
            std::fs::remove_file(dest).with_context(|| format!("replacing {}", dest.display()))?;
        }
        let dest_str = dest.to_str().context("backup path is not valid UTF-8")?;
        self.conn
            .execute("VACUUM INTO ?1", [dest_str])
            .with_context(|| format!("writing backup to {}", dest.display()))?;
        Ok(())
    }

    /// Replace all current data with the contents of a backup file.
    /// The backup is validated first, then migrated forward if it came from an older version.
    pub fn restore_from(&mut self, src: &Path) -> Result<()> {
        {
            let probe = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .with_context(|| format!("opening {}", src.display()))?;
            let ok: i64 = probe
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('tasks','projects','settings')",
                    [],
                    |r| r.get(0),
                )
                .context("file is not a Command Center backup")?;
            if ok != 3 {
                bail!("file is not a Command Center backup");
            }
        }
        self.conn
            .restore(MAIN_DB, src, None::<fn(rusqlite::backup::Progress)>)
            .context("restoring backup")?;
        self.reopen_after_restore()
    }

    fn reopen_after_restore(&mut self) -> Result<()> {
        let path = self.path().to_path_buf();
        if path.as_os_str().is_empty() {
            return Ok(()); // in-memory (tests)
        }
        *self = Db::open(&path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Db;

    #[test]
    fn export_and_restore_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cc-backup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let live = dir.join("live.db");
        let backup = dir.join("snap.ccbackup");
        let _ = std::fs::remove_file(&live);

        let mut db = Db::open(&live).unwrap();
        db.set_setting("marker", "before").unwrap();
        db.export_to(&backup).unwrap();
        db.set_setting("marker", "after").unwrap();
        db.restore_from(&backup).unwrap();
        assert_eq!(db.setting("marker").as_deref(), Some("before"));

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_backup_once_a_day_and_prunes() {
        use super::{auto_backup, AUTO_KEEP};
        use chrono::{Duration, NaiveDate};
        let dir = std::env::temp_dir().join(format!("cc-auto-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(&dir.join("live.db")).unwrap();
        let backups = dir.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(backups.join("pre-restore-x.ccbackup"), b"keep me").unwrap();

        let day0 = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert!(auto_backup(&db, &backups, day0).unwrap().is_some());
        assert!(auto_backup(&db, &backups, day0).unwrap().is_none(), "once per day");
        for i in 1..10 {
            auto_backup(&db, &backups, day0 + Duration::days(i)).unwrap();
        }
        let names: Vec<String> = std::fs::read_dir(&backups).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.iter().filter(|n| n.starts_with("auto-")).count(), AUTO_KEEP);
        assert!(names.contains(&"auto-2026-11-10.ccbackup".to_string()));
        assert!(!names.contains(&"auto-2026-11-01.ccbackup".to_string()));
        assert!(names.contains(&"pre-restore-x.ccbackup".to_string()));
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
