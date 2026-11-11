//! Calendar events: manual entries now, synced sources (Google Calendar) later via
//! [`crate::calendar::EventSource`]. Times are local and stored as `YYYY-MM-DDTHH:MM:SS`.

use crate::db::Db;
use anyhow::{bail, Result};
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::{params, Row};

pub const MANUAL: &str = "manual";
const TS: &str = "%Y-%m-%dT%H:%M:%S";

pub fn ts(dt: NaiveDateTime) -> String {
    dt.format(TS).to_string()
}

pub fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, TS).ok()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub title: String,
    pub starts_at: NaiveDateTime,
    pub ends_at: Option<NaiveDateTime>,
    pub all_day: bool,
    pub location: String,
    pub notes: String,
    /// `manual`, or the key of a synced source such as `google`.
    pub source: String,
    /// Hex colour like `#4ade80`; `None` = the theme accent.
    pub color: Option<String>,
}

impl Event {
    pub fn is_manual(&self) -> bool {
        self.source == MANUAL
    }
    pub fn start_date(&self) -> NaiveDate {
        self.starts_at.date()
    }
    /// Last day the event covers (all-day events can span several days).
    pub fn end_date(&self) -> NaiveDate {
        match (self.all_day, self.ends_at) {
            (true, Some(e)) if e.date() > self.start_date() => e.date(),
            _ => self.start_date(),
        }
    }
    /// Days within `[from, to]` on which the event should be listed.
    pub fn days_in(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let mut d = self.start_date().max(from);
        let last = self.end_date().min(to);
        let mut out = Vec::new();
        while d <= last {
            out.push(d);
            d += Duration::days(1);
        }
        out
    }
    pub fn is_now(&self, now: NaiveDateTime) -> bool {
        !self.all_day && self.starts_at <= now && self.ends_at.is_some_and(|e| now < e)
    }
    pub fn is_past(&self, now: NaiveDateTime) -> bool {
        if self.all_day {
            self.end_date() < now.date()
        } else {
            self.ends_at.unwrap_or(self.starts_at) <= now
        }
    }
    /// "All day", "9:15 – 10:00" / "9:15 – 10:00 AM", or a single start time.
    pub fn time_label(&self, h12: bool) -> String {
        if self.all_day {
            return "All day".into();
        }
        let s = self.starts_at.time();
        match self.ends_at {
            Some(e) if e.date() == self.start_date() => time_range(s, e.time(), h12),
            Some(e) => format!("{} – {} {}", fmt_time(s, h12), e.format("%a"), fmt_time(e.time(), h12)),
            None => fmt_time(s, h12),
        }
    }
}

/// `17:00` or `5:00 PM`.
pub fn fmt_time(t: NaiveTime, h12: bool) -> String {
    if h12 { t.format("%-I:%M %p").to_string() } else { t.format("%-H:%M").to_string() }
}

/// `9:15 – 10:00` (24h), `11:00 – 11:30 AM` (12h, same half of the day) or `11:30 AM – 1:00 PM`.
pub fn time_range(s: NaiveTime, e: NaiveTime, h12: bool) -> String {
    if h12 && s.format("%p").to_string() == e.format("%p").to_string() {
        format!("{} – {}", s.format("%-I:%M"), fmt_time(e, true))
    } else {
        format!("{} – {}", fmt_time(s, h12), fmt_time(e, h12))
    }
}

impl Event {
}

const SELECT: &str = "SELECT id, title, starts_at, ends_at, all_day, location, notes, source, color FROM events";

fn from_row(r: &Row) -> rusqlite::Result<Event> {
    let start: String = r.get(2)?;
    let end: Option<String> = r.get(3)?;
    Ok(Event {
        id: r.get(0)?,
        title: r.get(1)?,
        starts_at: parse_ts(&start).unwrap_or_default(),
        ends_at: end.as_deref().and_then(parse_ts),
        all_day: r.get(4)?,
        location: r.get(5)?,
        notes: r.get(6)?,
        source: r.get(7)?,
        color: r.get(8)?,
    })
}

fn query(db: &Db, tail: &str, p: impl rusqlite::Params) -> Vec<Event> {
    let Ok(mut stmt) = db.conn.prepare_cached(&format!("{SELECT} {tail}")) else { return Vec::new() };
    stmt.query_map(p, from_row).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// Events touching the days `from..=to`, by start time (all-day first within a day).
pub fn between(db: &Db, from: NaiveDate, to: NaiveDate) -> Vec<Event> {
    let start = format!("{from}T00:00:00");
    let end = format!("{}T00:00:00", to + Duration::days(1));
    query(
        db,
        "WHERE starts_at < ?2 AND COALESCE(ends_at, starts_at) >= ?1
         ORDER BY substr(starts_at,1,10), all_day DESC, starts_at, title",
        params![start, end],
    )
}

pub fn get(db: &Db, id: i64) -> Option<Event> {
    query(db, "WHERE id = ?1", [id]).into_iter().next()
}

pub fn search(db: &Db, q: &str, from: NaiveDate) -> Vec<Event> {
    query(
        db,
        "WHERE (title LIKE ?1 ESCAPE '\\' OR location LIKE ?1 ESCAPE '\\') AND COALESCE(ends_at, starts_at) >= ?2
         ORDER BY starts_at LIMIT 6",
        params![crate::db::like_pattern(q), format!("{from}T00:00:00")],
    )
}

/// Fields the editor (or quick add) can set.
#[derive(Debug, Clone, PartialEq)]
pub struct EventInput {
    pub title: String,
    pub date: NaiveDate,
    /// `None` = all-day.
    pub start: Option<NaiveTime>,
    pub end: Option<NaiveTime>,
    pub end_date: Option<NaiveDate>,
    pub location: String,
    pub notes: String,
}

impl EventInput {
    /// (starts_at, ends_at, all_day) with sensible fixes: an end before the start rolls
    /// to the next day; a multi-day all-day event keeps its last day.
    fn resolve(&self) -> Result<(NaiveDateTime, Option<NaiveDateTime>, bool)> {
        if self.title.trim().is_empty() {
            bail!("an event needs a title");
        }
        Ok(match self.start {
            None => {
                let end = self.end_date.filter(|d| *d > self.date).map(|d| d.and_time(NaiveTime::MIN));
                (self.date.and_time(NaiveTime::MIN), end, true)
            }
            Some(s) => {
                let start = self.date.and_time(s);
                let end = self.end.map(|e| {
                    let mut end = self.end_date.unwrap_or(self.date).and_time(e);
                    if end <= start {
                        end = self.date.and_time(e) + Duration::days(1);
                    }
                    end
                });
                (start, end, false)
            }
        })
    }
}

pub fn add(db: &Db, input: &EventInput) -> Result<i64> {
    let (s, e, all_day) = input.resolve()?;
    db.conn.execute(
        "INSERT INTO events(title,starts_at,ends_at,all_day,location,notes,source) VALUES (?1,?2,?3,?4,?5,?6,'manual')",
        params![input.title.trim(), ts(s), e.map(ts), all_day, input.location.trim(), input.notes],
    )?;
    Ok(db.conn.last_insert_rowid())
}

/// Update a manual event. Synced events are read-only here (edit them at their source).
pub fn update(db: &Db, id: i64, input: &EventInput) -> Result<()> {
    let (s, e, all_day) = input.resolve()?;
    let n = db.conn.execute(
        "UPDATE events SET title=?1, starts_at=?2, ends_at=?3, all_day=?4, location=?5, notes=?6
         WHERE id=?7 AND source='manual'",
        params![input.title.trim(), ts(s), e.map(ts), all_day, input.location.trim(), input.notes, id],
    )?;
    if n == 0 {
        bail!("event is read-only or missing");
    }
    Ok(())
}

/// Place a manual event on `date` from `start_min` to `end_min` (minutes after midnight,
/// `end_min` up to 1440 = midnight). Used by the week grid's drag/resize.
/// All-day events keep their length in days and only change date.
pub fn place(db: &Db, id: i64, date: NaiveDate, start_min: i64, end_min: i64) -> Result<()> {
    let Some(e) = get(db, id) else { bail!("event not found") };
    let mut input = input_of(&e);
    if e.all_day {
        let span = e.end_date() - e.start_date();
        input.date = date;
        input.end_date = (span.num_days() > 0).then(|| date + span);
    } else {
        let t = |m: i64| NaiveTime::from_num_seconds_from_midnight_opt((m.clamp(0, 1439) * 60) as u32, 0);
        let start = start_min.clamp(0, 1425);
        let end = end_min.clamp(start + 15, 1440);
        input.date = date;
        input.start = t(start);
        // 1440 = midnight at the end of the day: stored as 00:00 the next day.
        input.end = if end >= 1440 { t(0) } else { t(end) };
        input.end_date = None;
    }
    update(db, id, &input)
}

pub fn input_of(e: &Event) -> EventInput {
    EventInput {
        title: e.title.clone(),
        date: e.start_date(),
        start: (!e.all_day).then(|| e.starts_at.time()),
        end: if e.all_day { None } else { e.ends_at.map(|x| x.time()) },
        end_date: if e.all_day { Some(e.end_date()) } else { e.ends_at.map(|x| x.date()) },
        location: e.location.clone(),
        notes: e.notes.clone(),
    }
}

/// Set or clear (`None`) an event's colour. Works for synced events too: colour is local only.
pub fn set_color(db: &Db, id: i64, color: Option<&str>) -> Result<()> {
    let color = color.filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()));
    db.conn.execute("UPDATE events SET color = ?1 WHERE id = ?2", params![color, id])?;
    Ok(())
}

pub fn delete(db: &Db, id: i64) -> Result<Option<Event>> {
    let ev = get(db, id).filter(Event::is_manual);
    if ev.is_some() {
        db.conn.execute("DELETE FROM events WHERE id = ?1 AND source = 'manual'", [id])?;
    }
    Ok(ev)
}

pub fn restore(db: &Db, e: &Event) -> Result<()> {
    db.conn.execute(
        "INSERT INTO events(id,title,starts_at,ends_at,all_day,location,notes,source,color) VALUES (?1,?2,?3,?4,?5,?6,?7,'manual',?8)",
        params![e.id, e.title, ts(e.starts_at), e.ends_at.map(ts), e.all_day, e.location, e.notes, e.color],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }
    fn input(title: &str, date: &str, start: Option<NaiveTime>, end: Option<NaiveTime>) -> EventInput {
        EventInput { title: title.into(), date: d(date), start, end, end_date: None, location: String::new(), notes: String::new() }
    }

    #[test]
    fn range_query_and_labels() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, &input("Call", "2026-11-12", Some(t(11, 0)), Some(t(11, 30)))).unwrap();
        add(&db, &input("Deadline", "2026-11-12", None, None)).unwrap();
        add(&db, &input("Far", "2026-12-30", None, None)).unwrap();
        let list = between(&db, d("2026-11-11"), d("2026-11-17"));
        assert_eq!(list.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), vec!["Deadline", "Call"]);
        assert_eq!(get(&db, a).unwrap().time_label(false), "11:00 – 11:30");
        assert_eq!(get(&db, a).unwrap().time_label(true), "11:00 – 11:30 AM");
        assert_eq!(time_range(NaiveTime::from_hms_opt(11, 30, 0).unwrap(), NaiveTime::from_hms_opt(13, 0, 0).unwrap(), true), "11:30 AM – 1:00 PM");
    }

    #[test]
    fn overnight_and_multi_day() {
        let db = Db::open_in_memory().unwrap();
        let id = add(&db, &input("Stream", "2026-11-14", Some(t(22, 0)), Some(t(1, 0)))).unwrap();
        let e = get(&db, id).unwrap();
        assert_eq!(e.ends_at.unwrap().date(), d("2026-11-15"), "end before start rolls to next day");

        let mut trip = input("Trip", "2026-11-20", None, None);
        trip.end_date = Some(d("2026-11-22"));
        let id = add(&db, &trip).unwrap();
        let e = get(&db, id).unwrap();
        assert_eq!(e.days_in(d("2026-11-21"), d("2026-11-30")), vec![d("2026-11-21"), d("2026-11-22")]);
        assert_eq!(between(&db, d("2026-11-22"), d("2026-11-22")).len(), 1);
        assert_eq!(input_of(&e).end_date, Some(d("2026-11-22")));
    }

    #[test]
    fn synced_events_are_read_only() {
        let db = Db::open_in_memory().unwrap();
        db.conn
            .execute("INSERT INTO events(title,starts_at,source,external_id) VALUES ('G','2026-11-12T10:00:00','google','x1')", [])
            .unwrap();
        let id = db.conn.last_insert_rowid();
        assert!(update(&db, id, &input("Changed", "2026-11-12", None, None)).is_err());
        assert!(delete(&db, id).unwrap().is_none());
        assert!(get(&db, id).is_some());
        assert!(add(&db, &input("  ", "2026-11-12", None, None)).is_err());
    }
}

#[cfg(test)]
mod place_tests {
    use super::*;

    #[test]
    fn place_moves_and_resizes() {
        let db = Db::open_in_memory().unwrap();
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let input = EventInput {
            title: "Call".into(), date: d("2026-11-12"),
            start: NaiveTime::from_hms_opt(10, 0, 0), end: NaiveTime::from_hms_opt(11, 0, 0),
            end_date: None, location: String::new(), notes: String::new(),
        };
        let id = add(&db, &input).unwrap();
        place(&db, id, d("2026-11-13"), 14 * 60 + 30, 16 * 60).unwrap();
        let e = get(&db, id).unwrap();
        assert_eq!(ts(e.starts_at), "2026-11-13T14:30:00");
        assert_eq!(ts(e.ends_at.unwrap()), "2026-11-13T16:00:00");
        place(&db, id, d("2026-11-13"), 23 * 60, 1440).unwrap();
        assert_eq!(ts(get(&db, id).unwrap().ends_at.unwrap()), "2026-11-14T00:00:00");
        place(&db, id, d("2026-11-13"), 600, 600).unwrap();
        assert_eq!(ts(get(&db, id).unwrap().ends_at.unwrap()), "2026-11-13T10:15:00", "minimum 15 minutes");
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn colors_set_clear_and_survive_restore() {
        let db = Db::open_in_memory().unwrap();
        let input = EventInput {
            title: "Stream".into(), date: NaiveDate::from_ymd_opt(2026, 11, 14).unwrap(),
            start: NaiveTime::from_hms_opt(20, 0, 0), end: None, end_date: None,
            location: String::new(), notes: String::new(),
        };
        let id = add(&db, &input).unwrap();
        assert_eq!(get(&db, id).unwrap().color, None);
        set_color(&db, id, Some("#f472b6")).unwrap();
        assert_eq!(get(&db, id).unwrap().color.as_deref(), Some("#f472b6"));
        set_color(&db, id, Some("red; drop table")).unwrap();
        assert_eq!(get(&db, id).unwrap().color, None, "only #rrggbb is stored");
        set_color(&db, id, Some("#4ade80")).unwrap();
        update(&db, id, &input_of(&get(&db, id).unwrap())).unwrap();
        assert_eq!(get(&db, id).unwrap().color.as_deref(), Some("#4ade80"), "editing keeps the colour");
        let e = delete(&db, id).unwrap().unwrap();
        restore(&db, &e).unwrap();
        assert_eq!(get(&db, id).unwrap().color.as_deref(), Some("#4ade80"));
    }
}
