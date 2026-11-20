//! Desktop reminders: which ones are due right now, and their wording.
//!
//! - Calendar events: 5 minutes before they start.
//! - Tasks with a deadline time: 1 hour and 5 minutes before it.
//! - Tasks with a daily reminder: once a day at that time, until done.
//!
//! Each reminder has a stable key and is recorded in `reminders_sent` once shown, so it
//! fires once. Reminders more than [`LATE_LIMIT`] past their moment are skipped (the app
//! wasn't running then), so starting the app never floods you with stale ones.

use crate::db::Db;
use crate::model::events::{self, fmt_time};
use crate::model::tasks::{self, Task};
use crate::time::now_ts;
use anyhow::Result;
use chrono::{Duration, NaiveDateTime};
use rusqlite::params;

/// How long after its moment a missed reminder is still worth showing.
pub const LATE_LIMIT: Duration = Duration::minutes(15);
const EVENT_LEAD: Duration = Duration::minutes(5);

#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    /// Stable id: the same reminder is never shown twice.
    pub key: String,
    pub title: String,
    pub body: String,
}

/// Reminders due at `now` that haven't been shown yet.
pub fn due(db: &Db, now: NaiveDateTime, h12: bool) -> Vec<Reminder> {
    let mut out = Vec::new();
    let in_window = |at: NaiveDateTime| now >= at && now - at <= LATE_LIMIT;

    // Events starting soon (timed, not checked off).
    for e in events::between(db, now.date(), now.date() + Duration::days(1)) {
        if e.all_day || e.done || e.starts_at <= now {
            continue;
        }
        let at = e.starts_at - EVENT_LEAD;
        if in_window(at) {
            let mins = minutes_until(now, e.starts_at);
            out.push(Reminder {
                key: format!("ev:{}:{}", e.id, events::ts(e.starts_at)),
                title: format!("{} {}", e.title, starts_phrase(mins)),
                body: format!("{} at {}", if e.location.is_empty() { "Starts".to_string() } else { e.location.clone() }, fmt_time(e.starts_at.time(), h12)),
            });
        }
    }

    for t in open_tasks(db) {
        // Deadline: an hour before and 5 minutes before.
        if let Some(deadline) = deadline(&t) {
            for (tag, lead) in [("1h", Duration::hours(1)), ("5m", Duration::minutes(5))] {
                if deadline > now && in_window(deadline - lead) {
                    out.push(Reminder {
                        key: format!("task-{tag}:{}:{}", t.id, events::ts(deadline)),
                        title: format!("“{}” is due {}", t.title, in_phrase(minutes_until(now, deadline))),
                        body: finish_line(&t.reward, Some(minutes_until(now, deadline))),
                    });
                }
            }
        }
        // Daily reminder.
        if let Some(time) = t.remind_daily {
            let at = now.date().and_time(time);
            if in_window(at) {
                out.push(Reminder {
                    key: format!("daily:{}:{}", t.id, now.date()),
                    title: daily_title(&t, now, h12),
                    body: finish_line(&t.reward, deadline(&t).filter(|d| *d > now && d.date() == now.date()).map(|d| minutes_until(now, d))),
                });
            }
        }
    }

    out.retain(|r| !was_sent(db, &r.key));
    out
}

pub fn mark_sent(db: &Db, key: &str) -> Result<()> {
    db.conn.execute("INSERT OR IGNORE INTO reminders_sent(key, sent_at) VALUES (?1, ?2)", params![key, now_ts()])?;
    // Keep the table small: entries older than a month can't matter any more.
    db.conn.execute("DELETE FROM reminders_sent WHERE sent_at < datetime('now', '-31 days')", [])?;
    Ok(())
}

fn was_sent(db: &Db, key: &str) -> bool {
    db.count(&format!("SELECT count(*) FROM reminders_sent WHERE key = '{}'", key.replace('\'', "''"))) > 0
}

fn open_tasks(db: &Db) -> Vec<Task> {
    let ids: Vec<i64> = db
        .conn
        .prepare("SELECT id FROM tasks WHERE done_at IS NULL AND (remind_daily IS NOT NULL OR (due_date IS NOT NULL AND due_time IS NOT NULL))")
        .and_then(|mut s| s.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()))
        .unwrap_or_default();
    ids.into_iter().filter_map(|id| tasks::get(db, id)).collect()
}

/// The task's deadline as a moment, if it has both a due date and a time.
pub fn deadline(t: &Task) -> Option<NaiveDateTime> {
    Some(t.due_date?.and_time(t.due_time?))
}

fn minutes_until(now: NaiveDateTime, at: NaiveDateTime) -> i64 {
    // Round up, so 4 min 30 s left reads as "5 minutes".
    ((at - now).num_seconds() + 59).div_euclid(60).max(0)
}

/// "in 5 minutes", "in an hour", "in 1 hour 20 minutes", "now".
pub fn in_phrase(mins: i64) -> String {
    match mins {
        m if m <= 0 => "now".into(),
        1 => "in 1 minute".into(),
        m if m < 60 => format!("in {m} minutes"),
        60 => "in an hour".into(),
        m if m % 60 == 0 => format!("in {} hours", m / 60),
        m => format!("in {} {} {}", m / 60, if m / 60 == 1 { "hour" } else { "hours" }, plural(m % 60, "minute")),
    }
}

fn starts_phrase(mins: i64) -> String {
    if mins <= 0 { "is starting".into() } else { format!("starts {}", in_phrase(mins)) }
}

fn plural(n: i64, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

/// "the next hour", "the next 5 minutes", "the next 2 hours".
fn span_phrase(mins: i64) -> String {
    match mins {
        60 => "the next hour".into(),
        m if m < 60 => format!("the next {}", plural(m.max(1), "minute")),
        m if m % 60 == 0 => format!("the next {} hours", m / 60),
        m => format!("the next {} {}", m / 60, if m / 60 == 1 { "hour" } else { "hours" }),
    }
}

fn daily_title(t: &Task, now: NaiveDateTime, h12: bool) -> String {
    let when = match (t.due_date, deadline(t)) {
        (_, Some(d)) if d.date() == now.date() => format!("is due today at {}", fmt_time(d.time(), h12)),
        (Some(d), _) if d < now.date() => "is overdue".to_string(),
        (Some(d), _) if d == now.date() => "is due today".to_string(),
        (Some(d), _) if d == now.date() + Duration::days(1) => "is due tomorrow".to_string(),
        (Some(d), _) if d < now.date() + Duration::days(7) => format!("is due {}", d.format("%A")),
        (Some(d), _) => format!("is due {}", d.format("%b %-d")),
        (None, _) => return format!("Reminder: {}", t.title),
    };
    format!("“{}” {when}", t.title)
}

/// The encouragement line: "Finish it in the next hour and you can play Minecraft."
/// Without a reward: "Finish it in the next hour." / "Get it done today."
pub fn finish_line(reward: &str, mins_left: Option<i64>) -> String {
    let lead = match mins_left {
        Some(m) => format!("Finish it in {}", span_phrase(m)),
        None => "Get it done today".into(),
    };
    match reward_clause(reward) {
        Some(clause) => format!("{lead} and {clause}."),
        None => format!("{lead}."),
    }
}

/// Turn a free-form reward into the end of a sentence that follows "…and ":
///
/// - a verb phrase ("play Minecraft") → "you can play Minecraft"
/// - something already about you ("you get pizza", "I can nap") → "you get pizza", "you can nap"
/// - an "-ing" activity ("gaming") → "you can spend some time gaming"
/// - anything else, a thing ("pizza", "a new game") → "you've earned pizza"
pub fn reward_clause(reward: &str) -> Option<String> {
    let r = reward.trim().trim_end_matches(['.', '!', ',', ';']).trim();
    if r.is_empty() {
        return None;
    }
    let first = r.split_whitespace().next().unwrap_or("").to_lowercase();
    let rest = r[r.find(char::is_whitespace).unwrap_or(r.len())..].trim_start();
    // Lower-case a capitalised first word only if it's an ordinary word ("Play", "A"), so
    // names keep their spelling ("YouTube", "Minecraft").
    let ordinary = VERBS.contains(&first.as_str()) || SMALL_WORDS.contains(&first.as_str());
    let text = match r.chars().next() {
        Some(a) if ordinary && a.is_uppercase() => a.to_lowercase().chain(r[a.len_utf8()..].chars()).collect(),
        _ => r.to_string(),
    };

    Some(match first.as_str() {
        "you" | "you'll" | "you're" => text,
        "i" => format!("you {}", rest),
        "i'll" => format!("you'll {}", rest),
        "i'm" => format!("you're {}", rest),
        "i've" => format!("you've {}", rest),
        "then" => return reward_clause(rest),
        w if SMALL_WORDS.contains(&w) => {
            format!("you've earned {}", text.replacen("my ", "your ", 1))
        }
        w if VERBS.contains(&w) => format!("you can {text}"),
        w if w.ends_with("ing") && w.len() > 4 => format!("you can spend some time {text}"),
        w if w.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '$') => format!("you've earned {text}"),
        _ => format!("you've earned {text}"),
    })
}

/// Determiners that start a "thing" reward ("a new game", "my favourite snack").
const SMALL_WORDS: &[&str] = &["a", "an", "the", "some", "one", "two", "three", "my", "more", "extra", "another", "free"];

/// Common first words of a "do something" reward.
const VERBS: &[&str] = &[
    "play", "watch", "eat", "get", "buy", "order", "take", "have", "go", "sleep", "nap", "rest", "relax", "chill", "read",
    "game", "stream", "call", "text", "visit", "see", "make", "cook", "bake", "drink", "grab", "treat", "spend", "start",
    "open", "try", "use", "enjoy", "listen", "scroll", "browse", "hang", "binge", "skip", "leave", "stop", "finish", "join",
    "ride", "walk", "run", "swim", "shop", "draw", "paint", "build", "explore", "celebrate", "unwind", "head", "keep",
    "stay", "sit", "lie", "veg", "touch", "do", "log", "hop", "jump", "queue", "launch", "boot", "rewatch", "replay",
];

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    #[test]
    fn reward_grammar() {
        let c = |s| reward_clause(s).unwrap();
        assert_eq!(c("play Minecraft"), "you can play Minecraft");
        assert_eq!(c("Play Minecraft for an hour."), "you can play Minecraft for an hour");
        assert_eq!(c("pizza"), "you've earned pizza");
        assert_eq!(c("a new game"), "you've earned a new game");
        assert_eq!(c("my favourite snack"), "you've earned your favourite snack");
        assert_eq!(c("gaming"), "you can spend some time gaming");
        assert_eq!(c("I can nap"), "you can nap");
        assert_eq!(c("I get boba"), "you get boba");
        assert_eq!(c("you get to sleep in"), "you get to sleep in");
        assert_eq!(c("then watch YouTube"), "you can watch YouTube");
        assert_eq!(c("$20 on steam"), "you've earned $20 on steam");
        assert_eq!(c("YouTube"), "you've earned YouTube");
        assert_eq!(reward_clause("   "), None);
    }

    #[test]
    fn finish_lines() {
        assert_eq!(finish_line("play Minecraft", Some(60)), "Finish it in the next hour and you can play Minecraft.");
        assert_eq!(finish_line("pizza", Some(5)), "Finish it in the next 5 minutes and you've earned pizza.");
        assert_eq!(finish_line("", Some(1)), "Finish it in the next 1 minute.");
        assert_eq!(finish_line("", None), "Get it done today.");
        assert_eq!(in_phrase(60), "in an hour");
        assert_eq!(in_phrase(85), "in 1 hour 25 minutes");
        assert_eq!(in_phrase(120), "in 2 hours");
    }

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 20).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn schedule_once_each() {
        let db = Db::open_in_memory().unwrap();
        let day = at(0, 0).date();
        // Event at 15:00, task due 17:00 with a reward, task with a daily reminder at 09:00.
        let ev = events::add(&db, &events::EventInput {
            title: "Dentist".into(), date: day, start: NaiveTime::from_hms_opt(15, 0, 0), end: None, end_date: None,
            location: String::new(), notes: String::new(),
        }).unwrap();
        let t = tasks::add(&db, "Record Debug World", day, Some(day), None).unwrap();
        tasks::set_due_time(&db, t, NaiveTime::from_hms_opt(17, 0, 0)).unwrap();
        tasks::set_reward(&db, t, "play Minecraft").unwrap();
        let d = tasks::add(&db, "Stretch", day, None, None).unwrap();
        tasks::set_remind_daily(&db, d, NaiveTime::from_hms_opt(9, 0, 0)).unwrap();

        assert!(due(&db, at(8, 59), true).is_empty());
        let r = due(&db, at(9, 0), true);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Reminder: Stretch");
        mark_sent(&db, &r[0].key).unwrap();
        assert!(due(&db, at(9, 1), true).is_empty(), "shown once");
        assert!(due(&db, at(9, 20), true).is_empty(), "too late to bother");

        let r = due(&db, at(14, 55), true);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Dentist starts in 5 minutes");
        assert_eq!(r[0].body, "Starts at 3:00 PM");
        mark_sent(&db, &r[0].key).unwrap();
        events::mark_done(&db, ev, day).unwrap();

        let r = due(&db, at(16, 0), true);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "“Record Debug World” is due in an hour");
        assert_eq!(r[0].body, "Finish it in the next hour and you can play Minecraft.");
        mark_sent(&db, &r[0].key).unwrap();
        let r = due(&db, at(16, 55), true);
        assert_eq!(r[0].title, "“Record Debug World” is due in 5 minutes");
        mark_sent(&db, &r[0].key).unwrap();

        tasks::set_done(&db, d, true).unwrap();
        let next_day = at(9, 0) + Duration::days(1);
        assert!(due(&db, next_day, true).is_empty(), "done tasks stop reminding");
    }
}
