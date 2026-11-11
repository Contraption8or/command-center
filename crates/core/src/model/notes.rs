//! Quick notes: Markdown bodies, pinning, project links, search.

use crate::db::Db;
use crate::markdown;
use crate::time::now_ts;
use anyhow::Result;
use rusqlite::{params, Row};

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: i64,
    pub body: String,
    pub pinned: bool,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Note {
    pub fn title(&self) -> String {
        let t = markdown::title(&self.body);
        if t.is_empty() { "Untitled".into() } else { t }
    }
    pub fn snippet(&self, max: usize) -> String {
        markdown::snippet(&self.body, max)
    }
}

const SELECT: &str = "SELECT n.id, n.body_md, n.pinned, n.project_id, p.name, p.color, n.created_at, n.updated_at
     FROM notes n LEFT JOIN projects p ON p.id = n.project_id";

fn from_row(r: &Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        body: r.get(1)?,
        pinned: r.get(2)?,
        project_id: r.get(3)?,
        project_name: r.get(4)?,
        project_color: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

fn query(db: &Db, tail: &str, p: impl rusqlite::Params) -> Vec<Note> {
    let Ok(mut stmt) = db.conn.prepare_cached(&format!("{SELECT} {tail}")) else { return Vec::new() };
    stmt.query_map(p, from_row).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// All notes, pinned first, then most recently edited. `filter` matches anywhere in the
/// body or the linked project's name (case-insensitive).
pub fn list(db: &Db, filter: &str) -> Vec<Note> {
    let f = filter.trim();
    if f.is_empty() {
        query(db, "ORDER BY n.pinned DESC, n.updated_at DESC, n.id DESC", [])
    } else {
        let like = crate::db::like_pattern(f);
        query(
            db,
            "WHERE n.body_md LIKE ?1 ESCAPE '\\' OR p.name LIKE ?1 ESCAPE '\\'
             ORDER BY n.pinned DESC, n.updated_at DESC, n.id DESC",
            [like],
        )
    }
}

pub fn for_project(db: &Db, project_id: i64) -> Vec<Note> {
    query(db, "WHERE n.project_id = ?1 ORDER BY n.pinned DESC, n.updated_at DESC", [project_id])
}

pub fn get(db: &Db, id: i64) -> Option<Note> {
    query(db, "WHERE n.id = ?1", [id]).into_iter().next()
}

pub fn add(db: &Db, body: &str, project_id: Option<i64>) -> Result<i64> {
    let ts = now_ts();
    db.conn.execute(
        "INSERT INTO notes(body_md,project_id,created_at,updated_at) VALUES (?1,?2,?3,?3)",
        params![body, project_id, ts],
    )?;
    Ok(db.conn.last_insert_rowid())
}

/// Save new text. No-op (and no timestamp bump) if nothing changed.
pub fn set_body(db: &Db, id: i64, body: &str) -> Result<()> {
    db.conn.execute(
        "UPDATE notes SET body_md = ?1, updated_at = ?2 WHERE id = ?3 AND body_md <> ?1",
        params![body, now_ts(), id],
    )?;
    Ok(())
}

pub fn set_pinned(db: &Db, id: i64, pinned: bool) -> Result<()> {
    db.conn.execute("UPDATE notes SET pinned = ?1 WHERE id = ?2", params![pinned, id])?;
    Ok(())
}

pub fn set_project(db: &Db, id: i64, project_id: Option<i64>) -> Result<()> {
    db.conn.execute("UPDATE notes SET project_id = ?1 WHERE id = ?2", params![project_id, id])?;
    Ok(())
}

pub fn toggle_checkbox(db: &Db, id: i64, index: usize) -> Result<()> {
    if let Some(n) = get(db, id) {
        if let Some(body) = markdown::toggle_task(&n.body, index) {
            set_body(db, id, &body)?;
        }
    }
    Ok(())
}

pub fn delete(db: &Db, id: i64) -> Result<Option<Note>> {
    let note = get(db, id);
    db.conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
    Ok(note)
}

pub fn restore(db: &Db, n: &Note) -> Result<()> {
    db.conn.execute(
        "INSERT INTO notes(id,body_md,pinned,project_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![n.id, n.body, n.pinned, n.project_id, n.created_at, n.updated_at],
    )?;
    Ok(())
}

/// Delete the note if it has no text (used when leaving a freshly created, unused note).
pub fn delete_if_empty(db: &Db, id: i64) -> Result<bool> {
    Ok(db.conn.execute("DELETE FROM notes WHERE id = ?1 AND trim(body_md) = ''", [id])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crud_search_and_order() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, "# Alpha\nfirst", None).unwrap();
        let b = add(&db, "# Beta 50%\nsecond", None).unwrap();
        set_pinned(&db, a, true).unwrap();
        assert_eq!(list(&db, "").iter().map(|n| n.id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(list(&db, "beta").len(), 1);
        assert_eq!(list(&db, "50%").len(), 1);
        assert_eq!(list(&db, "_").len(), 0, "wildcards are literal");
        assert_eq!(get(&db, b).unwrap().title(), "Beta 50%");

        let gone = delete(&db, b).unwrap().unwrap();
        assert!(get(&db, b).is_none());
        restore(&db, &gone).unwrap();
        assert_eq!(get(&db, b).unwrap().body, "# Beta 50%\nsecond");
    }

    #[test]
    fn checkbox_and_empty_cleanup() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, "- [ ] one", None).unwrap();
        toggle_checkbox(&db, a, 0).unwrap();
        assert_eq!(get(&db, a).unwrap().body, "- [x] one");
        let e = add(&db, "  ", None).unwrap();
        assert!(delete_if_empty(&db, e).unwrap());
        assert!(!delete_if_empty(&db, a).unwrap());
    }
}
