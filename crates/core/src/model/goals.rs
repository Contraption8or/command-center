//! Goals: a number to move from a start value to a target by a deadline.

use crate::db::Db;
use crate::time::now_ts;
use anyhow::{bail, Result};
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{params, Row};

#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    pub id: i64,
    pub title: String,
    pub unit: String,
    pub start: f64,
    pub current: f64,
    pub target: f64,
    pub deadline: Option<NaiveDate>,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub created: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Done,
    /// Deadline passed without reaching the target.
    Overdue,
    Ahead,
    OnTrack,
    Behind,
    /// No deadline set.
    Open,
}

impl Goal {
    /// Progress from start to target, 0–100.
    pub fn percent(&self) -> i64 {
        let span = self.target - self.start;
        if span == 0.0 {
            return if self.current >= self.target { 100 } else { 0 };
        }
        (((self.current - self.start) / span) * 100.0).round().clamp(0.0, 100.0) as i64
    }

    pub fn is_done(&self) -> bool {
        if self.target >= self.start { self.current >= self.target } else { self.current <= self.target }
    }

    pub fn days_left(&self, today: NaiveDate) -> Option<i64> {
        self.deadline.map(|d| (d - today).num_days())
    }

    /// Compare actual progress with a straight line from creation to the deadline.
    /// Within ±10 percentage points counts as on track.
    pub fn pace(&self, today: NaiveDate) -> Pace {
        if self.is_done() {
            return Pace::Done;
        }
        let Some(deadline) = self.deadline else { return Pace::Open };
        if today > deadline {
            return Pace::Overdue;
        }
        let total = (deadline - self.created).num_days().max(1) as f64;
        let elapsed = (today - self.created).num_days().clamp(0, total as i64) as f64;
        let expected = elapsed / total * 100.0;
        let diff = self.percent() as f64 - expected;
        if diff > 10.0 {
            Pace::Ahead
        } else if diff < -10.0 {
            Pace::Behind
        } else {
            Pace::OnTrack
        }
    }

    /// How much is needed per week from now to hit the target on time.
    pub fn needed_per_week(&self, today: NaiveDate) -> Option<f64> {
        let days = self.days_left(today)?;
        if days <= 0 || self.is_done() {
            return None;
        }
        Some((self.target - self.current) / days as f64 * 7.0)
    }
}

/// `42,850`, `3.5`, `1,200.25`: thousands separators, at most two decimals.
pub fn fmt_num(v: f64) -> String {
    let neg = v < 0.0;
    let v = v.abs();
    let whole = v.trunc() as i64;
    let frac = ((v - v.trunc()) * 100.0).round() as i64;
    let s = whole.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if frac > 0 {
        let f = format!("{frac:02}");
        out.push('.');
        out.push_str(f.trim_end_matches('0'));
    }
    if neg { format!("-{out}") } else { out }
}

const SELECT: &str = "SELECT g.id, g.title, g.unit, g.start_value, g.current_value, g.target_value, g.deadline,
        g.project_id, p.name, g.created_at
     FROM goals g LEFT JOIN projects p ON p.id = g.project_id";

fn from_row(r: &Row) -> rusqlite::Result<Goal> {
    let deadline: Option<String> = r.get(6)?;
    let created: String = r.get(9)?;
    Ok(Goal {
        id: r.get(0)?,
        title: r.get(1)?,
        unit: r.get(2)?,
        start: r.get(3)?,
        current: r.get(4)?,
        target: r.get(5)?,
        deadline: deadline.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        project_id: r.get(7)?,
        project_name: r.get(8)?,
        created: NaiveDateTime::parse_from_str(&created, "%Y-%m-%dT%H:%M:%S").map(|t| t.date()).unwrap_or_default(),
    })
}

fn query(db: &Db, tail: &str, p: impl rusqlite::Params) -> Vec<Goal> {
    let Ok(mut stmt) = db.conn.prepare_cached(&format!("{SELECT} {tail}")) else { return Vec::new() };
    stmt.query_map(p, from_row).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// Open goals first (soonest deadline first), finished ones last.
pub fn list(db: &Db) -> Vec<Goal> {
    let mut v = query(db, "ORDER BY g.deadline IS NULL, g.deadline, g.id", []);
    v.sort_by_key(|g| g.is_done());
    v
}

pub fn get(db: &Db, id: i64) -> Option<Goal> {
    query(db, "WHERE g.id = ?1", [id]).into_iter().next()
}

pub fn search(db: &Db, q: &str) -> Vec<Goal> {
    query(db, "WHERE g.title LIKE ?1 ESCAPE '\\' ORDER BY g.deadline LIMIT 5", [crate::db::like_pattern(q)])
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalInput {
    pub title: String,
    pub unit: String,
    pub start: f64,
    pub current: f64,
    pub target: f64,
    pub deadline: Option<NaiveDate>,
    pub project_id: Option<i64>,
}

impl GoalInput {
    pub fn of(g: &Goal) -> Self {
        GoalInput {
            title: g.title.clone(),
            unit: g.unit.clone(),
            start: g.start,
            current: g.current,
            target: g.target,
            deadline: g.deadline,
            project_id: g.project_id,
        }
    }
    fn check(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            bail!("a goal needs a title");
        }
        if !(self.start.is_finite() && self.current.is_finite() && self.target.is_finite()) {
            bail!("values must be numbers");
        }
        Ok(())
    }
}

pub fn add(db: &Db, g: &GoalInput) -> Result<i64> {
    g.check()?;
    db.conn.execute(
        "INSERT INTO goals(title,unit,start_value,current_value,target_value,deadline,project_id,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![g.title.trim(), g.unit.trim(), g.start, g.current, g.target, g.deadline.map(|d| d.to_string()), g.project_id, now_ts()],
    )?;
    Ok(db.conn.last_insert_rowid())
}

pub fn update(db: &Db, id: i64, g: &GoalInput) -> Result<()> {
    g.check()?;
    db.conn.execute(
        "UPDATE goals SET title=?1, unit=?2, start_value=?3, current_value=?4, target_value=?5, deadline=?6, project_id=?7 WHERE id=?8",
        params![g.title.trim(), g.unit.trim(), g.start, g.current, g.target, g.deadline.map(|d| d.to_string()), g.project_id, id],
    )?;
    Ok(())
}

pub fn set_current(db: &Db, id: i64, value: f64) -> Result<()> {
    if !value.is_finite() {
        bail!("value must be a number");
    }
    db.conn.execute("UPDATE goals SET current_value = ?1 WHERE id = ?2", params![value, id])?;
    Ok(())
}

pub fn delete(db: &Db, id: i64) -> Result<Option<(Goal, String)>> {
    let Some(g) = get(db, id) else { return Ok(None) };
    let created: String = db.conn.query_row("SELECT created_at FROM goals WHERE id = ?1", [id], |r| r.get(0))?;
    db.conn.execute("DELETE FROM goals WHERE id = ?1", [id])?;
    Ok(Some((g, created)))
}

pub fn restore(db: &Db, g: &Goal, created_at: &str) -> Result<()> {
    db.conn.execute(
        "INSERT INTO goals(id,title,unit,start_value,current_value,target_value,deadline,project_id,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![g.id, g.title, g.unit, g.start, g.current, g.target, g.deadline.map(|d| d.to_string()), g.project_id, created_at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn goal(start: f64, current: f64, target: f64) -> Goal {
        Goal {
            id: 1,
            title: "g".into(),
            unit: String::new(),
            start,
            current,
            target,
            deadline: Some(d("2026-12-31")),
            project_id: None,
            project_name: None,
            created: d("2026-01-01"),
        }
    }

    #[test]
    fn percent_and_pace() {
        let today = d("2026-07-02"); // ~50% through the year
        assert_eq!(goal(0.0, 50.0, 100.0).percent(), 50);
        assert_eq!(goal(0.0, 50.0, 100.0).pace(today), Pace::OnTrack);
        assert_eq!(goal(0.0, 80.0, 100.0).pace(today), Pace::Ahead);
        assert_eq!(goal(0.0, 20.0, 100.0).pace(today), Pace::Behind);
        assert_eq!(goal(0.0, 120.0, 100.0).pace(today), Pace::Done);
        assert_eq!(goal(0.0, 20.0, 100.0).pace(d("2027-01-05")), Pace::Overdue);
        // Decreasing goal (e.g. weight, debt).
        assert_eq!(goal(90.0, 85.0, 80.0).percent(), 50);
        assert!(goal(90.0, 79.0, 80.0).is_done());
        let w = goal(0.0, 50.0, 100.0).needed_per_week(d("2026-12-03")).unwrap();
        assert!((w - 12.5).abs() < 0.01);
    }

    #[test]
    fn numbers() {
        assert_eq!(fmt_num(42850.0), "42,850");
        assert_eq!(fmt_num(3.5), "3.5");
        assert_eq!(fmt_num(1200.25), "1,200.25");
        assert_eq!(fmt_num(-7.0), "-7");
    }

    #[test]
    fn crud() {
        let db = Db::open_in_memory().unwrap();
        let input = GoalInput { title: "Subs".into(), unit: "subs".into(), start: 0.0, current: 10.0, target: 100.0, deadline: None, project_id: None };
        let id = add(&db, &input).unwrap();
        set_current(&db, id, 40.0).unwrap();
        assert_eq!(get(&db, id).unwrap().percent(), 40);
        let (g, created) = delete(&db, id).unwrap().unwrap();
        restore(&db, &g, &created).unwrap();
        assert_eq!(get(&db, id).unwrap().current, 40.0);
        assert!(add(&db, &GoalInput { title: " ".into(), ..input }).is_err());
    }
}
