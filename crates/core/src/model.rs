//! Domain logic. Each module grows its own file under `model/` when its full view is built;
//! shared helpers (quick add, project lookup, overview summary) stay here.

pub mod events;
pub mod goals;
pub mod habits;
pub mod notes;
pub mod projects;
pub mod tasks;

use crate::db::Db;
use crate::quick_add::{project_matches, QuickAdd};
use crate::time::now_ts;
use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::params;

#[derive(Debug, Clone, PartialEq)]
pub enum Created {
    Task { id: i64, project: Option<String> },
    Note { id: i64 },
    Event { id: i64 },
}

/// Persist a parsed quick-add entry.
pub fn apply_quick_add(db: &Db, entry: &QuickAdd) -> Result<Created> {
    match entry {
        QuickAdd::Task(t) => {
            let project = t.project_query.as_deref().and_then(|q| find_project(db, q));
            let id = tasks::add_steps(db, &t.title, t.scheduled_for, t.due, project.as_ref().map(|p| p.0), t.steps)?;
            Ok(Created::Task { id, project: project.map(|p| p.1) })
        }
        QuickAdd::Note(body) => {
            let ts = now_ts();
            db.conn.execute("INSERT INTO notes(body_md,created_at,updated_at) VALUES (?1,?2,?2)", params![body, ts])?;
            Ok(Created::Note { id: db.conn.last_insert_rowid() })
        }
        QuickAdd::Event(e) => {
            let id = events::add(
                db,
                &events::EventInput {
                    title: e.title.clone(),
                    date: e.date,
                    start: e.start,
                    end: e.end,
                    end_date: None,
                    location: String::new(),
                    notes: String::new(),
                },
            )?;
            Ok(Created::Event { id })
        }
    }
}

/// Best project match for a `#query`; done projects rank below open ones.
pub fn find_project(db: &Db, query: &str) -> Option<(i64, String)> {
    let mut stmt = db.conn.prepare_cached("SELECT id, name, status FROM projects ORDER BY position").ok()?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .ok()?;
    rows.flatten()
        .filter_map(|(id, name, status)| {
            let score = project_matches(query, &name)? as i32 * 2 - (status == "done") as i32;
            Some((score, id, name))
        })
        .max_by_key(|(s, _, _)| *s)
        .map(|(_, id, name)| (id, name))
}

/// Counts shown on the phase-1 placeholder cards (and handy for sanity checks).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub tasks_open_today: i64,
    pub tasks_done_today: i64,
    pub events_next_14: i64,
    pub projects_active: i64,
    pub projects_total: i64,
    pub spent_this_month_cents: i64,
    pub spent_last_month_cents: i64,
    pub habits: i64,
    pub goals: i64,
    pub notes: i64,
    pub notes_pinned: i64,
}

pub fn summary(db: &Db, today: NaiveDate) -> Summary {
    let t = today.to_string();
    let c = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> i64 { db.conn.query_row(sql, p, |r| r.get(0)).unwrap_or(0) };
    let month = &t[..7];
    let last = crate::seed::prev_month(today.with_day0(0).unwrap_or(today)).to_string();
    let last_month = &last[..7];
    let in14 = (today + chrono::Duration::days(14)).to_string();
    Summary {
        tasks_open_today: c("SELECT count(*) FROM tasks WHERE scheduled_for <= ?1 AND done_at IS NULL", &[&t]),
        tasks_done_today: c("SELECT count(*) FROM tasks WHERE scheduled_for = ?1 AND done_at IS NOT NULL", &[&t]),
        events_next_14: c("SELECT count(*) FROM events WHERE substr(starts_at,1,10) BETWEEN ?1 AND ?2", &[&t, &in14]),
        projects_active: c("SELECT count(*) FROM projects WHERE status = 'active'", &[]),
        projects_total: c("SELECT count(*) FROM projects", &[]),
        spent_this_month_cents: c("SELECT COALESCE(SUM(amount_cents),0) FROM expenses WHERE substr(date,1,7) = ?1", &[&month]),
        spent_last_month_cents: c("SELECT COALESCE(SUM(amount_cents),0) FROM expenses WHERE substr(date,1,7) = ?1", &[&last_month]),
        habits: c("SELECT count(*) FROM habits WHERE archived = 0", &[]),
        goals: c("SELECT count(*) FROM goals", &[]),
        notes: c("SELECT count(*) FROM notes", &[]),
        notes_pinned: c("SELECT count(*) FROM notes WHERE pinned = 1", &[]),
    }
}

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_add::parse;

    #[test]
    fn quick_add_links_project() {
        let mut db = Db::open_in_memory().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        crate::seed::ensure(&mut db, today).unwrap();
        let before = summary(&db, today);
        let created = apply_quick_add(&db, &parse("Grade ep 9 #survival", today).unwrap()).unwrap();
        assert!(matches!(created, Created::Task { project: Some(ref p), .. } if p == "Hardcore Survival S3"));
        assert_eq!(summary(&db, today).tasks_open_today, before.tasks_open_today + 1);
        assert!(before.spent_last_month_cents > before.spent_this_month_cents / 2);
    }
}

/// Projects for pickers: (id, name, color), open projects first.
pub fn project_options(db: &Db) -> Vec<(i64, String, String)> {
    let Ok(mut stmt) = db
        .conn
        .prepare_cached("SELECT id, name, color FROM projects ORDER BY status = 'done', position, name")
    else {
        return Vec::new();
    };
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}
