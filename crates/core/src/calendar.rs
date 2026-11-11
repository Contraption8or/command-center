//! Calendar sources. Manual events live only in the database; external calendars implement
//! [`EventSource`] and are mirrored into the `events` table by [`sync`].
//!
//! Adding Google Calendar later means: an `EventSource` whose `key()` is `"google"` and whose
//! `fetch` calls the Calendar API (OAuth token stored locally), plus a "Sync now" action and a
//! periodic timer in the app. Nothing in the schema or the UI needs to change: synced rows are
//! shown alongside manual ones and are read-only in the editor.

use crate::db::Db;
use crate::model::events::ts;
use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::params;
use std::collections::HashSet;

/// An event as delivered by an external calendar.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteEvent {
    /// Stable id in the remote system; used to update in place on the next sync.
    pub external_id: String,
    pub calendar_id: String,
    pub title: String,
    pub starts_at: NaiveDateTime,
    pub ends_at: Option<NaiveDateTime>,
    pub all_day: bool,
    pub location: String,
    pub notes: String,
}

pub trait EventSource {
    /// Short key stored in `events.source`, e.g. `"google"`. Must not be `"manual"`.
    fn key(&self) -> &'static str;
    /// Events overlapping `from..=to` (local dates).
    fn fetch(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<RemoteEvent>>;
}

#[derive(Debug, Default, PartialEq)]
pub struct SyncReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

/// Mirror `source` into the database for the window `from..=to`: upsert everything it
/// returns, and remove that source's rows in the window that it no longer returns.
pub fn sync(db: &mut Db, source: &dyn EventSource, from: NaiveDate, to: NaiveDate) -> Result<SyncReport> {
    let key = source.key();
    anyhow::ensure!(key != crate::model::events::MANUAL, "source key `manual` is reserved");
    let remote = source.fetch(from, to)?;
    let tx = db.conn.transaction()?;
    let mut report = SyncReport::default();
    let mut seen = HashSet::new();
    for r in &remote {
        seen.insert(r.external_id.clone());
        let changed = tx.execute(
            "UPDATE events SET title=?1, starts_at=?2, ends_at=?3, all_day=?4, location=?5, notes=?6, calendar_id=?7
             WHERE source=?8 AND external_id=?9",
            params![r.title, ts(r.starts_at), r.ends_at.map(ts), r.all_day, r.location, r.notes, r.calendar_id, key, r.external_id],
        )?;
        if changed > 0 {
            report.updated += 1;
        } else {
            tx.execute(
                "INSERT INTO events(title,starts_at,ends_at,all_day,location,notes,source,external_id,calendar_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![r.title, ts(r.starts_at), r.ends_at.map(ts), r.all_day, r.location, r.notes, key, r.external_id, r.calendar_id],
            )?;
            report.added += 1;
        }
    }
    let stale: Vec<(i64, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, external_id FROM events WHERE source=?1 AND starts_at >= ?2 AND starts_at < ?3",
        )?;
        let rows = stmt.query_map(
            params![key, format!("{from}T00:00:00"), format!("{}T00:00:00", to + chrono::Duration::days(1))],
            |r| Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())),
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|(_, ext)| !seen.contains(ext)).collect()
    };
    for (id, _) in &stale {
        tx.execute("DELETE FROM events WHERE id = ?1", [id])?;
    }
    report.removed = stale.len();
    tx.commit()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::events;
    use std::cell::RefCell;

    struct Fake(RefCell<Vec<RemoteEvent>>);
    impl EventSource for Fake {
        fn key(&self) -> &'static str {
            "fake"
        }
        fn fetch(&self, _: NaiveDate, _: NaiveDate) -> Result<Vec<RemoteEvent>> {
            Ok(self.0.borrow().clone())
        }
    }

    fn remote(id: &str, title: &str, day: u32) -> RemoteEvent {
        RemoteEvent {
            external_id: id.into(),
            calendar_id: "primary".into(),
            title: title.into(),
            starts_at: NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(10, 0, 0).unwrap(),
            ends_at: None,
            all_day: false,
            location: String::new(),
            notes: String::new(),
        }
    }

    #[test]
    fn upserts_and_removes() {
        let mut db = Db::open_in_memory().unwrap();
        let from = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let src = Fake(RefCell::new(vec![remote("a", "A", 12), remote("b", "B", 13)]));
        assert_eq!(sync(&mut db, &src, from, to).unwrap(), SyncReport { added: 2, updated: 0, removed: 0 });

        *src.0.borrow_mut() = vec![remote("a", "A renamed", 12)];
        assert_eq!(sync(&mut db, &src, from, to).unwrap(), SyncReport { added: 0, updated: 1, removed: 1 });
        let list = events::between(&db, from, to);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "A renamed");
        assert!(!list[0].is_manual());
    }
}
