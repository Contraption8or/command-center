//! Habits: daily or N-times-per-week, with check-ins, streaks and completion rate.
//! Weeks start on Monday.

use crate::db::Db;
use crate::time::now_ts;
use anyhow::{bail, Result};
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::params;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub struct Habit {
    pub id: i64,
    pub name: String,
    /// `daily` or `weekly`.
    pub cadence: String,
    /// Check-ins needed per week (weekly habits; 1 for daily).
    pub target: i64,
    pub color: String,
    pub archived: bool,
    pub position: i64,
    /// Every check-in date, oldest first.
    pub checks: BTreeSet<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    /// Current streak: days (daily) or weeks (weekly). Today / this week not being done
    /// yet doesn't break it.
    pub current: i64,
    pub best: i64,
    /// Done today (daily) or check-ins so far this week (weekly).
    pub this_period: i64,
    /// Share of the last 30 days (daily) or last 4 full weeks (weekly) that were met, 0–100.
    pub rate: i64,
}

pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

impl Habit {
    pub fn is_weekly(&self) -> bool {
        self.cadence == "weekly"
    }

    pub fn done_on(&self, d: NaiveDate) -> bool {
        self.checks.contains(&d)
    }

    fn week_count(&self, monday: NaiveDate) -> i64 {
        self.checks.range(monday..monday + Duration::days(7)).count() as i64
    }

    pub fn stats(&self, today: NaiveDate) -> Stats {
        if self.is_weekly() { self.weekly_stats(today) } else { self.daily_stats(today) }
    }

    fn daily_stats(&self, today: NaiveDate) -> Stats {
        let mut current = 0;
        let mut d = if self.done_on(today) { today } else { today - Duration::days(1) };
        while self.done_on(d) {
            current += 1;
            d -= Duration::days(1);
        }
        let (mut best, mut run, mut prev) = (0i64, 0i64, None::<NaiveDate>);
        for &c in &self.checks {
            run = if prev == Some(c - Duration::days(1)) { run + 1 } else { 1 };
            best = best.max(run);
            prev = Some(c);
        }
        let done30 = (0..30).filter(|i| self.done_on(today - Duration::days(*i))).count() as i64;
        Stats { current, best: best.max(current), this_period: self.done_on(today) as i64, rate: done30 * 100 / 30 }
    }

    fn weekly_stats(&self, today: NaiveDate) -> Stats {
        let target = self.target.max(1);
        let this_week = week_start(today);
        let met = |monday: NaiveDate| self.week_count(monday) >= target;
        let mut current = if met(this_week) { 1 } else { 0 };
        let mut w = this_week - Duration::days(7);
        while met(w) {
            current += 1;
            w -= Duration::days(7);
        }
        let mut best = 0;
        if let Some(first) = self.checks.first() {
            let (mut run, mut w) = (0, week_start(*first));
            while w <= this_week {
                run = if met(w) { run + 1 } else { 0 };
                best = best.max(run);
                w += Duration::days(7);
            }
        }
        let rate = (1..=4).filter(|i| met(this_week - Duration::days(7 * i))).count() as i64 * 25;
        Stats { current, best: best.max(current), this_period: self.week_count(this_week), rate }
    }
}

/// Active habits (or all, including archived) with their check-ins.
pub fn list(db: &Db, include_archived: bool) -> Vec<Habit> {
    let mut checks: HashMap<i64, BTreeSet<NaiveDate>> = HashMap::new();
    if let Ok(mut stmt) = db.conn.prepare_cached("SELECT habit_id, date FROM habit_checks") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
            for (id, d) in rows.flatten() {
                if let Ok(d) = NaiveDate::parse_from_str(&d, "%Y-%m-%d") {
                    checks.entry(id).or_default().insert(d);
                }
            }
        }
    }
    let sql = format!(
        "SELECT id, name, cadence, target_per_period, color, archived, position FROM habits {} ORDER BY archived, position, id",
        if include_archived { "" } else { "WHERE archived = 0" }
    );
    let Ok(mut stmt) = db.conn.prepare_cached(&sql) else { return Vec::new() };
    stmt.query_map([], |r| {
        Ok(Habit {
            id: r.get(0)?,
            name: r.get(1)?,
            cadence: r.get(2)?,
            target: r.get(3)?,
            color: r.get(4)?,
            archived: r.get(5)?,
            position: r.get(6)?,
            checks: BTreeSet::new(),
        })
    })
    .map(|rows| {
        rows.flatten()
            .map(|mut h| {
                h.checks = checks.remove(&h.id).unwrap_or_default();
                h
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn get(db: &Db, id: i64) -> Option<Habit> {
    list(db, true).into_iter().find(|h| h.id == id)
}

pub fn search(db: &Db, q: &str) -> Vec<Habit> {
    let q = q.trim().to_lowercase();
    list(db, false).into_iter().filter(|h| h.name.to_lowercase().contains(&q)).take(5).collect()
}

pub fn add(db: &Db, name: &str, cadence: &str, target: i64, color: &str) -> Result<i64> {
    check(name, cadence)?;
    db.conn.execute(
        "INSERT INTO habits(name,cadence,target_per_period,color,position,created_at)
         VALUES (?1,?2,?3,?4,(SELECT COALESCE(MAX(position),0)+1 FROM habits),?5)",
        params![name.trim(), cadence, target.clamp(1, 7), color, now_ts()],
    )?;
    Ok(db.conn.last_insert_rowid())
}

fn check(name: &str, cadence: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("a habit needs a name");
    }
    if cadence != "daily" && cadence != "weekly" {
        bail!("cadence must be daily or weekly");
    }
    Ok(())
}

/// Check in on `day`, or undo the check-in if it's already there. Returns the new state.
pub fn toggle(db: &Db, id: i64, day: NaiveDate) -> Result<bool> {
    let d = day.to_string();
    let removed = db.conn.execute("DELETE FROM habit_checks WHERE habit_id = ?1 AND date = ?2", params![id, d])?;
    if removed == 0 {
        db.conn.execute("INSERT INTO habit_checks(habit_id,date) VALUES (?1,?2)", params![id, d])?;
    }
    Ok(removed == 0)
}

pub fn update(db: &Db, id: i64, name: &str, cadence: &str, target: i64, color: &str) -> Result<()> {
    check(name, cadence)?;
    let target = if cadence == "daily" { 1 } else { target.clamp(1, 7) };
    db.conn.execute(
        "UPDATE habits SET name=?1, cadence=?2, target_per_period=?3, color=?4 WHERE id=?5",
        params![name.trim(), cadence, target, color, id],
    )?;
    Ok(())
}

pub fn set_archived(db: &Db, id: i64, archived: bool) -> Result<()> {
    db.conn.execute("UPDATE habits SET archived = ?1 WHERE id = ?2", params![archived, id])?;
    Ok(())
}

pub fn delete(db: &Db, id: i64) -> Result<Option<Habit>> {
    let h = get(db, id);
    db.conn.execute("DELETE FROM habits WHERE id = ?1", [id])?; // check-ins cascade
    Ok(h)
}

pub fn restore(db: &mut Db, h: &Habit) -> Result<()> {
    let tx = db.conn.transaction()?;
    tx.execute(
        "INSERT INTO habits(id,name,cadence,target_per_period,color,archived,position,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![h.id, h.name, h.cadence, h.target, h.color, h.archived, h.position, now_ts()],
    )?;
    for d in &h.checks {
        tx.execute("INSERT INTO habit_checks(habit_id,date) VALUES (?1,?2)", params![h.id, d.to_string()])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn habit(cadence: &str, target: i64, days: &[&str]) -> Habit {
        Habit {
            id: 1,
            name: "h".into(),
            cadence: cadence.into(),
            target,
            color: String::new(),
            archived: false,
            position: 0,
            checks: days.iter().map(|s| d(s)).collect(),
        }
    }

    #[test]
    fn daily_streaks() {
        let today = d("2026-11-11");
        // Not done today yet: streak counts back from yesterday.
        let h = habit("daily", 1, &["2026-11-08", "2026-11-09", "2026-11-10", "2026-11-01", "2026-11-02"]);
        let s = h.stats(today);
        assert_eq!((s.current, s.best, s.this_period), (3, 3, 0));
        let h = habit("daily", 1, &["2026-11-11", "2026-11-10"]);
        assert_eq!(h.stats(today).current, 2);
        let h = habit("daily", 1, &["2026-11-09"]);
        assert_eq!(h.stats(today).current, 0, "missing yesterday breaks it");
    }

    #[test]
    fn weekly_streaks() {
        let today = d("2026-11-11"); // Wednesday; week starts Mon Nov 9
        // Last two full weeks met 2x; this week has 1 so far (in progress, not counted, not breaking).
        let h = habit("weekly", 2, &["2026-10-26", "2026-10-28", "2026-11-02", "2026-11-05", "2026-11-10"]);
        let s = h.stats(today);
        assert_eq!((s.current, s.this_period), (2, 1));
        assert_eq!(s.rate, 50);
        let h = habit("weekly", 1, &["2026-11-02", "2026-11-09"]);
        assert_eq!(h.stats(today).current, 2, "this week met counts");
    }

    #[test]
    fn toggle_and_restore() {
        let mut db = Db::open_in_memory().unwrap();
        let id = add(&db, "Read", "daily", 1, "#4ade80").unwrap();
        assert!(toggle(&db, id, d("2026-11-11")).unwrap());
        assert!(!toggle(&db, id, d("2026-11-11")).unwrap());
        toggle(&db, id, d("2026-11-10")).unwrap();
        let h = delete(&db, id).unwrap().unwrap();
        assert!(get(&db, id).is_none());
        restore(&mut db, &h).unwrap();
        assert_eq!(get(&db, id).unwrap().checks.len(), 1);
        assert!(add(&db, " ", "daily", 1, "").is_err());
        assert!(add(&db, "x", "hourly", 1, "").is_err());
    }
}
