//! Projects: status, next action, progress (manual or derived from linked tasks).

use crate::db::Db;
use crate::time::now_ts;
use anyhow::{bail, Result};
use rusqlite::{params, Row};

pub const STATUSES: [&str; 5] = ["active", "planned", "paused", "idea", "done"];

/// Swatches offered in the editor.
pub const COLORS: [&str; 8] = ["#7c8cff", "#4ade80", "#22d3ee", "#c084fc", "#f472b6", "#f87171", "#fbbf24", "#94a3b8"];

pub fn status_label(s: &str) -> &'static str {
    match s {
        "active" => "Active",
        "planned" => "Planned",
        "paused" => "Paused",
        "idea" => "Idea",
        "done" => "Done",
        _ => "Unknown",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub description: String,
    pub next_action: String,
    /// Manual progress, 0–100 (used when `from_tasks` is false).
    pub progress: i64,
    pub from_tasks: bool,
    pub color: String,
    pub position: i64,
    pub tasks_open: i64,
    pub tasks_done: i64,
    /// Sum of linked task completion (done = 1, step tasks count partially).
    pub task_units: f64,
    pub notes: i64,
    pub updated_at: String,
}

impl Project {
    /// Progress shown on the bar: manual value, or done/total of linked tasks.
    pub fn percent(&self) -> i64 {
        if self.from_tasks {
            let total = self.tasks_open + self.tasks_done;
            if total == 0 { 0 } else { (self.task_units * 100.0 / total as f64).round() as i64 }
        } else {
            self.progress.clamp(0, 100)
        }
    }
}

const SELECT: &str = "SELECT p.id, p.name, p.status, p.description, p.next_action, p.progress,
        p.progress_mode = 'tasks', p.color, p.position,
        (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.done_at IS NULL),
        (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.done_at IS NOT NULL),
        (SELECT count(*) FROM notes n WHERE n.project_id = p.id),
        p.updated_at,
        (SELECT COALESCE(SUM(CASE WHEN t.done_at IS NOT NULL THEN 1.0
                ELSE MIN(t.steps_done, t.steps_total) * 1.0 / t.steps_total END), 0)
         FROM tasks t WHERE t.project_id = p.id)
     FROM projects p";

const ORDER: &str = "ORDER BY CASE p.status WHEN 'active' THEN 0 WHEN 'planned' THEN 1 WHEN 'paused' THEN 2
        WHEN 'idea' THEN 3 ELSE 4 END, p.position, p.name";

fn from_row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        status: r.get(2)?,
        description: r.get(3)?,
        next_action: r.get(4)?,
        progress: r.get(5)?,
        from_tasks: r.get(6)?,
        color: r.get(7)?,
        position: r.get(8)?,
        tasks_open: r.get(9)?,
        tasks_done: r.get(10)?,
        notes: r.get(11)?,
        updated_at: r.get(12)?,
        task_units: r.get(13)?,
    })
}

fn query(db: &Db, tail: &str, p: impl rusqlite::Params) -> Vec<Project> {
    let Ok(mut stmt) = db.conn.prepare_cached(&format!("{SELECT} {tail}")) else { return Vec::new() };
    stmt.query_map(p, from_row).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// All projects: active, planned, paused, idea, done; manual order within each.
pub fn list(db: &Db) -> Vec<Project> {
    query(db, ORDER, [])
}

pub fn get(db: &Db, id: i64) -> Option<Project> {
    query(db, "WHERE p.id = ?1", [id]).into_iter().next()
}

pub fn search(db: &Db, q: &str) -> Vec<Project> {
    query(
        db,
        &format!("WHERE p.name LIKE ?1 ESCAPE '\\' OR p.next_action LIKE ?1 ESCAPE '\\' {ORDER} LIMIT 6"),
        [crate::db::like_pattern(q)],
    )
}

pub fn add(db: &Db, name: &str, status: &str) -> Result<i64> {
    check_status(status)?;
    let ts = now_ts();
    let color = COLORS[(db.count("SELECT count(*) FROM projects") as usize) % COLORS.len()];
    db.conn.execute(
        "INSERT INTO projects(name,status,color,position,created_at,updated_at)
         VALUES (?1,?2,?3,(SELECT COALESCE(MAX(position),0)+1 FROM projects),?4,?4)",
        params![name.trim(), status, color, ts],
    )?;
    Ok(db.conn.last_insert_rowid())
}

fn check_status(s: &str) -> Result<()> {
    if !STATUSES.contains(&s) {
        bail!("unknown status {s}");
    }
    Ok(())
}

fn set(db: &Db, id: i64, column: &str, value: &dyn rusqlite::ToSql) -> Result<()> {
    // `column` only ever comes from the fixed names below.
    db.conn.execute(
        &format!("UPDATE projects SET {column} = ?1, updated_at = ?2 WHERE id = ?3"),
        params![value, now_ts(), id],
    )?;
    Ok(())
}

pub fn set_name(db: &Db, id: i64, v: &str) -> Result<()> {
    if v.trim().is_empty() { Ok(()) } else { set(db, id, "name", &v.trim()) }
}
pub fn set_description(db: &Db, id: i64, v: &str) -> Result<()> {
    set(db, id, "description", &v)
}
pub fn set_next_action(db: &Db, id: i64, v: &str) -> Result<()> {
    set(db, id, "next_action", &v)
}
pub fn set_progress(db: &Db, id: i64, v: i64) -> Result<()> {
    set(db, id, "progress", &v.clamp(0, 100))
}
pub fn set_from_tasks(db: &Db, id: i64, v: bool) -> Result<()> {
    set(db, id, "progress_mode", &if v { "tasks" } else { "manual" })
}
pub fn set_color(db: &Db, id: i64, v: &str) -> Result<()> {
    set(db, id, "color", &v)
}

/// Change status. Marking a manually tracked project done also fills its bar.
pub fn set_status(db: &Db, id: i64, status: &str) -> Result<()> {
    check_status(status)?;
    set(db, id, "status", &status)?;
    if status == "done" {
        db.conn.execute("UPDATE projects SET progress = 100 WHERE id = ?1 AND progress_mode = 'manual'", [id])?;
    }
    Ok(())
}

/// Everything needed to undo a delete: the row plus the tasks/notes that pointed at it.
#[derive(Debug, Clone)]
pub struct Deleted {
    pub project: Project,
    created_at: String,
    task_ids: Vec<i64>,
    note_ids: Vec<i64>,
}

/// Delete a project. Linked tasks and notes are kept, just unlinked.
pub fn delete(db: &mut Db, id: i64) -> Result<Option<Deleted>> {
    let Some(project) = get(db, id) else { return Ok(None) };
    let ids = |sql: &str| -> Result<Vec<i64>> {
        let mut stmt = db.conn.prepare(sql)?;
        let v = stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(v)
    };
    let task_ids = ids("SELECT id FROM tasks WHERE project_id = ?1")?;
    let note_ids = ids("SELECT id FROM notes WHERE project_id = ?1")?;
    let created_at: String = db.conn.query_row("SELECT created_at FROM projects WHERE id = ?1", [id], |r| r.get(0))?;
    db.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    Ok(Some(Deleted { project, created_at, task_ids, note_ids }))
}

pub fn restore(db: &mut Db, d: &Deleted) -> Result<()> {
    let p = &d.project;
    let tx = db.conn.transaction()?;
    tx.execute(
        "INSERT INTO projects(id,name,status,description,next_action,progress,progress_mode,color,position,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            p.id, p.name, p.status, p.description, p.next_action, p.progress,
            if p.from_tasks { "tasks" } else { "manual" }, p.color, p.position, d.created_at, p.updated_at
        ],
    )?;
    for t in &d.task_ids {
        tx.execute("UPDATE tasks SET project_id = ?1 WHERE id = ?2", params![p.id, t])?;
    }
    for n in &d.note_ids {
        tx.execute("UPDATE notes SET project_id = ?1 WHERE id = ?2", params![p.id, n])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{notes, tasks};
    use chrono::NaiveDate;

    #[test]
    fn progress_modes_and_ordering() {
        let db = Db::open_in_memory().unwrap();
        let idea = add(&db, "Idea", "idea").unwrap();
        let act = add(&db, "Act", "active").unwrap();
        assert_eq!(list(&db).iter().map(|p| p.id).collect::<Vec<_>>(), vec![act, idea]);

        set_progress(&db, act, 140).unwrap();
        assert_eq!(get(&db, act).unwrap().percent(), 100);

        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let t1 = tasks::add(&db, "a", day, None, Some(act)).unwrap();
        tasks::add(&db, "b", day, None, Some(act)).unwrap();
        tasks::add(&db, "c", day, None, Some(act)).unwrap();
        tasks::set_done(&db, t1, true).unwrap();
        set_from_tasks(&db, act, true).unwrap();
        let p = get(&db, act).unwrap();
        assert_eq!((p.tasks_open, p.tasks_done, p.percent()), (2, 1, 33));

        assert!(set_status(&db, idea, "bogus").is_err());
        set_status(&db, idea, "done").unwrap();
        assert_eq!(get(&db, idea).unwrap().percent(), 100);
    }

    #[test]
    fn delete_and_restore_relinks() {
        let mut db = Db::open_in_memory().unwrap();
        let p = add(&db, "P", "active").unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let t = tasks::add(&db, "t", day, None, Some(p)).unwrap();
        let n = notes::add(&db, "n", Some(p)).unwrap();
        let d = delete(&mut db, p).unwrap().unwrap();
        assert_eq!(tasks::get(&db, t).unwrap().project_id, None);
        restore(&mut db, &d).unwrap();
        assert_eq!(tasks::get(&db, t).unwrap().project_id, Some(p));
        assert_eq!(notes::get(&db, n).unwrap().project_id, Some(p));
        assert_eq!(get(&db, p).unwrap().tasks_open, 1);
    }
}
