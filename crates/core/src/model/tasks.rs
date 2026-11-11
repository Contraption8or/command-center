//! Tasks: daily lists, carry-over, ordering and edits.
//!
//! A task lives on one day's list (`scheduled_for`) and may also have a `due_date`.
//! Unfinished tasks from past days are moved onto today's list by [`carry_over`];
//! `carried_count` adds up how many days each one has slipped.

use crate::db::Db;
use crate::time::now_ts;
use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::{params, Row};

/// Gap between consecutive `position` values, so most inserts need no renumbering.
pub const STEP: i64 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub scheduled_for: NaiveDate,
    pub position: i64,
    pub done: bool,
    pub carried_count: i64,
    /// Steps for multi-part tasks ("Edit 3 hours" = 3). Plain tasks have 1.
    pub steps_total: i64,
    pub steps_done: i64,
}

impl Task {
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.done && self.due_date.is_some_and(|d| d < today)
    }
    pub fn is_due_today(&self, today: NaiveDate) -> bool {
        !self.done && self.due_date == Some(today)
    }
    pub fn has_steps(&self) -> bool {
        self.steps_total > 1
    }
    /// Completion as a fraction: 1.0 when done, otherwise steps done / steps total.
    pub fn fraction(&self) -> f64 {
        if self.done { 1.0 } else { self.steps_done.min(self.steps_total) as f64 / self.steps_total.max(1) as f64 }
    }
}

const SELECT: &str = "SELECT t.id, t.title, t.notes, t.project_id, p.name, p.color, t.due_date,
        t.scheduled_for, t.position, t.done_at IS NOT NULL, t.carried_count, t.steps_total, t.steps_done
     FROM tasks t LEFT JOIN projects p ON p.id = t.project_id";

fn from_row(r: &Row) -> rusqlite::Result<Task> {
    let date = |s: Option<String>| s.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());
    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        project_id: r.get(3)?,
        project_name: r.get(4)?,
        project_color: r.get(5)?,
        due_date: date(r.get(6)?),
        scheduled_for: date(Some(r.get(7)?)).unwrap_or_default(),
        position: r.get(8)?,
        done: r.get(9)?,
        carried_count: r.get(10)?,
        steps_total: r.get(11)?,
        steps_done: r.get(12)?,
    })
}

fn query(db: &Db, where_order: &str, p: impl rusqlite::Params) -> Vec<Task> {
    let sql = format!("{SELECT} {where_order}");
    let Ok(mut stmt) = db.conn.prepare_cached(&sql) else { return Vec::new() };
    stmt.query_map(p, from_row).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// One day's list: open tasks in manual order, then the ones completed, in completion order.
pub fn for_day(db: &Db, day: NaiveDate) -> Vec<Task> {
    query(
        db,
        "WHERE t.scheduled_for = ?1 ORDER BY t.done_at IS NOT NULL, CASE WHEN t.done_at IS NULL THEN t.position END, t.done_at",
        [day.to_string()],
    )
}

/// Open tasks scheduled after `day`, by date then manual order.
pub fn upcoming(db: &Db, day: NaiveDate) -> Vec<Task> {
    query(db, "WHERE t.scheduled_for > ?1 AND t.done_at IS NULL ORDER BY t.scheduled_for, t.position", [day.to_string()])
}

pub fn for_project(db: &Db, project_id: i64) -> Vec<Task> {
    query(db, "WHERE t.project_id = ?1 ORDER BY t.done_at IS NOT NULL, t.scheduled_for, t.position", [project_id])
}

pub fn get(db: &Db, id: i64) -> Option<Task> {
    query(db, "WHERE t.id = ?1", [id]).into_iter().next()
}

/// Open tasks whose title or project name contains `q` (case-insensitive), soonest first.
pub fn search(db: &Db, q: &str) -> Vec<Task> {
    query(
        db,
        "WHERE t.done_at IS NULL AND (t.title LIKE ?1 ESCAPE '\\' OR p.name LIKE ?1 ESCAPE '\\')
         ORDER BY t.scheduled_for, t.position LIMIT 8",
        [crate::db::like_pattern(q)],
    )
}

/// Move every unfinished task from earlier days onto `today`'s list, above today's own
/// tasks, keeping their previous relative order. Returns how many tasks moved.
pub fn carry_over(db: &mut Db, today: NaiveDate) -> Result<usize> {
    let t = today.to_string();
    let tx = db.conn.transaction()?;
    let stale: Vec<(i64, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, scheduled_for FROM tasks WHERE scheduled_for < ?1 AND done_at IS NULL ORDER BY scheduled_for, position",
        )?;
        let rows = stmt.query_map([&t], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    if stale.is_empty() {
        return Ok(0);
    }
    let top: i64 = tx.query_row(
        "SELECT COALESCE(MIN(position), ?2) FROM tasks WHERE scheduled_for = ?1 AND done_at IS NULL",
        params![t, STEP],
        |r| r.get(0),
    )?;
    let n = stale.len() as i64;
    for (i, (id, from)) in stale.iter().enumerate() {
        let days = NaiveDate::parse_from_str(from, "%Y-%m-%d").map(|d| (today - d).num_days()).unwrap_or(1).max(1);
        tx.execute(
            "UPDATE tasks SET scheduled_for = ?1, position = ?2, carried_count = carried_count + ?3 WHERE id = ?4",
            params![t, top - (n - i as i64) * STEP, days, id],
        )?;
    }
    tx.commit()?;
    Ok(stale.len())
}

pub fn add(db: &Db, title: &str, day: NaiveDate, due: Option<NaiveDate>, project_id: Option<i64>) -> Result<i64> {
    add_steps(db, title, day, due, project_id, 1)
}

/// Add a task with `steps` parts (1 = a plain task).
pub fn add_steps(db: &Db, title: &str, day: NaiveDate, due: Option<NaiveDate>, project_id: Option<i64>, steps: i64) -> Result<i64> {
    let pos: i64 = db.conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + ?2 FROM tasks WHERE scheduled_for = ?1",
        params![day.to_string(), STEP],
        |r| r.get(0),
    )?;
    db.conn.execute(
        "INSERT INTO tasks(title,project_id,due_date,scheduled_for,position,created_at,steps_total) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![title.trim(), project_id, due.map(|d| d.to_string()), day.to_string(), pos, now_ts(), steps.clamp(1, 99)],
    )?;
    Ok(db.conn.last_insert_rowid())
}

/// Mark done (all steps complete) or re-open. Re-opening a step task undoes only its last step.
pub fn set_done(db: &Db, id: i64, done: bool) -> Result<()> {
    if done {
        db.conn.execute("UPDATE tasks SET done_at = ?1, steps_done = steps_total WHERE id = ?2", params![now_ts(), id])?;
    } else {
        // Re-opened tasks go to the bottom of their day's open list.
        db.conn.execute(
            "UPDATE tasks SET done_at = NULL, steps_done = MAX(steps_total - 1, 0),
                 position = (SELECT COALESCE(MAX(position), 0) + ?2 FROM tasks o
                             WHERE o.scheduled_for = tasks.scheduled_for AND o.done_at IS NULL)
             WHERE id = ?1",
            params![id, STEP],
        )?;
    }
    Ok(())
}

/// Check off one more step (`delta = 1`) or undo one (`delta = -1`). Completing the last
/// step marks the task done; undoing a step of a done task re-opens it.
pub fn step(db: &Db, id: i64, delta: i64) -> Result<()> {
    let Some(t) = get(db, id) else { return Ok(()) };
    let next = (t.steps_done + delta).clamp(0, t.steps_total);
    if next >= t.steps_total && !t.done {
        set_done(db, id, true)
    } else if next < t.steps_total && t.done {
        set_done(db, id, false)?;
        db.conn.execute("UPDATE tasks SET steps_done = ?1 WHERE id = ?2", params![next, id])?;
        Ok(())
    } else {
        db.conn.execute("UPDATE tasks SET steps_done = ?1 WHERE id = ?2", params![next, id])?;
        Ok(())
    }
}

/// Change how many steps a task has (1–99). Progress already made is kept where it fits.
pub fn set_steps(db: &Db, id: i64, total: i64) -> Result<()> {
    let Some(t) = get(db, id) else { return Ok(()) };
    let total = total.clamp(1, 99);
    let done_steps = if t.done { total } else { t.steps_done.min(total) };
    db.conn.execute("UPDATE tasks SET steps_total = ?1, steps_done = ?2 WHERE id = ?3", params![total, done_steps, id])?;
    if !t.done && done_steps >= total && total > 1 {
        set_done(db, id, true)?;
    }
    Ok(())
}

pub fn set_title(db: &Db, id: i64, title: &str) -> Result<()> {
    let title = title.trim();
    if !title.is_empty() {
        db.conn.execute("UPDATE tasks SET title = ?1 WHERE id = ?2", params![title, id])?;
    }
    Ok(())
}

pub fn set_notes(db: &Db, id: i64, notes: &str) -> Result<()> {
    db.conn.execute("UPDATE tasks SET notes = ?1 WHERE id = ?2", params![notes, id])?;
    Ok(())
}

pub fn set_due(db: &Db, id: i64, due: Option<NaiveDate>) -> Result<()> {
    db.conn.execute("UPDATE tasks SET due_date = ?1 WHERE id = ?2", params![due.map(|d| d.to_string()), id])?;
    Ok(())
}

pub fn set_project(db: &Db, id: i64, project_id: Option<i64>) -> Result<()> {
    db.conn.execute("UPDATE tasks SET project_id = ?1 WHERE id = ?2", params![project_id, id])?;
    Ok(())
}

/// Move a task to another day's list (appended at the bottom). Deliberate moves don't
/// count as slipping, so `carried_count` is reset.
pub fn reschedule(db: &Db, id: i64, day: NaiveDate) -> Result<()> {
    db.conn.execute(
        "UPDATE tasks SET scheduled_for = ?1, carried_count = 0,
             position = (SELECT COALESCE(MAX(position), 0) + ?3 FROM tasks WHERE scheduled_for = ?1)
         WHERE id = ?2",
        params![day.to_string(), id, STEP],
    )?;
    Ok(())
}

pub fn delete(db: &Db, id: i64) -> Result<Option<Task>> {
    let task = get(db, id);
    db.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    Ok(task)
}

/// Re-insert a task removed by [`delete`] (undo).
pub fn restore(db: &Db, t: &Task) -> Result<()> {
    db.conn.execute(
        "INSERT INTO tasks(id,title,notes,project_id,due_date,scheduled_for,position,done_at,carried_count,created_at,steps_total,steps_done)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            t.id,
            t.title,
            t.notes,
            t.project_id,
            t.due_date.map(|d| d.to_string()),
            t.scheduled_for.to_string(),
            t.position,
            t.done.then(now_ts),
            t.carried_count,
            now_ts(),
            t.steps_total,
            t.steps_done
        ],
    )?;
    Ok(())
}

/// Persist a new manual order: `ids` in their new top-to-bottom order.
pub fn reorder(db: &mut Db, ids: &[i64]) -> Result<()> {
    let tx = db.conn.transaction()?;
    for (i, id) in ids.iter().enumerate() {
        tx.execute("UPDATE tasks SET position = ?1 WHERE id = ?2", params![(i as i64 + 1) * STEP, id])?;
    }
    tx.commit()?;
    Ok(())
}

/// Move `id` to sit at index `to` within `order` (the current visible order). Returns the new order.
pub fn moved(order: &[i64], id: i64, to: usize) -> Vec<i64> {
    let mut v: Vec<i64> = order.iter().copied().filter(|&x| x != id).collect();
    v.insert(to.min(v.len()), id);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn carry_over_moves_open_tasks_to_top() {
        let mut db = Db::open_in_memory().unwrap();
        let mon = d("2026-11-09");
        let wed = d("2026-11-11");
        let old_a = add(&db, "old a", mon, None, None).unwrap();
        let old_b = add(&db, "old b", mon, None, None).unwrap();
        let old_done = add(&db, "old done", mon, None, None).unwrap();
        set_done(&db, old_done, true).unwrap();
        let today = add(&db, "today", wed, None, None).unwrap();

        assert_eq!(carry_over(&mut db, wed).unwrap(), 2);
        let ids: Vec<i64> = for_day(&db, wed).iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![old_a, old_b, today]);
        assert_eq!(get(&db, old_a).unwrap().carried_count, 2);
        assert_eq!(get(&db, old_done).unwrap().scheduled_for, mon);
        assert_eq!(carry_over(&mut db, wed).unwrap(), 0, "idempotent");
    }

    #[test]
    fn done_sorts_last_and_reopen_appends() {
        let db = Db::open_in_memory().unwrap();
        let day = d("2026-11-11");
        let a = add(&db, "a", day, None, None).unwrap();
        let b = add(&db, "b", day, None, None).unwrap();
        set_done(&db, a, true).unwrap();
        assert_eq!(for_day(&db, day).iter().map(|t| t.id).collect::<Vec<_>>(), vec![b, a]);
        set_done(&db, a, false).unwrap();
        assert_eq!(for_day(&db, day).iter().map(|t| t.id).collect::<Vec<_>>(), vec![b, a]);
    }

    #[test]
    fn reorder_and_moved() {
        let mut db = Db::open_in_memory().unwrap();
        let day = d("2026-11-11");
        let ids: Vec<i64> = ["a", "b", "c"].iter().map(|t| add(&db, t, day, None, None).unwrap()).collect();
        let new = moved(&ids, ids[2], 0);
        assert_eq!(new, vec![ids[2], ids[0], ids[1]]);
        reorder(&mut db, &new).unwrap();
        assert_eq!(for_day(&db, day).iter().map(|t| t.id).collect::<Vec<_>>(), new);
    }

    #[test]
    fn delete_and_restore() {
        let db = Db::open_in_memory().unwrap();
        let day = d("2026-11-11");
        let a = add(&db, "a", day, Some(day), None).unwrap();
        let t = delete(&db, a).unwrap().unwrap();
        assert!(get(&db, a).is_none());
        restore(&db, &t).unwrap();
        assert_eq!(get(&db, a).unwrap().due_date, Some(day));
    }

    #[test]
    fn reschedule_resets_carried() {
        let mut db = Db::open_in_memory().unwrap();
        let a = add(&db, "a", d("2026-11-09"), None, None).unwrap();
        carry_over(&mut db, d("2026-11-11")).unwrap();
        reschedule(&db, a, d("2026-11-12")).unwrap();
        let t = get(&db, a).unwrap();
        assert_eq!((t.scheduled_for, t.carried_count), (d("2026-11-12"), 0));
    }
}

#[cfg(test)]
mod step_tests {
    use super::*;

    #[test]
    fn steps_progress_and_complete() {
        let db = Db::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let id = add_steps(&db, "Edit 3 hours", day, None, None, 3).unwrap();
        step(&db, id, 1).unwrap();
        let t = get(&db, id).unwrap();
        assert_eq!((t.steps_done, t.done), (1, false));
        assert!((t.fraction() - 1.0 / 3.0).abs() < 1e-9);
        step(&db, id, 1).unwrap();
        step(&db, id, 1).unwrap();
        assert!(get(&db, id).unwrap().done, "last step completes the task");
        step(&db, id, -1).unwrap();
        let t = get(&db, id).unwrap();
        assert_eq!((t.steps_done, t.done), (2, false), "undo re-opens");
        set_done(&db, id, true).unwrap();
        set_done(&db, id, false).unwrap();
        assert_eq!(get(&db, id).unwrap().steps_done, 2, "re-open undoes one step");
        set_steps(&db, id, 2).unwrap();
        assert!(get(&db, id).unwrap().done, "shrinking to what's done completes it");
        let d = delete(&db, id).unwrap().unwrap();
        restore(&db, &d).unwrap();
        assert_eq!(get(&db, id).unwrap().steps_total, 2);
    }
}
