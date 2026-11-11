//! Parser for the one-line quick-add box.
//!
//! ```text
//! Color-grade ep 7 !fri #survival      -> task, due Friday, linked to "Hardcore Survival S3"
//! Record door build @tomorrow          -> task, scheduled on tomorrow's list
//! Edit ep 8 x3                         -> task with 3 steps (each check-off = 33%)
//! n Idea: world-shrinks challenge      -> note
//! e Dentist @fri 9:15-10               -> event (no time = all day)
//! ```
//! Date words: today, tomorrow/tmr, mon..sun (next occurrence, today counts), +N (days), YYYY-MM-DD.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};

#[derive(Debug, Clone, PartialEq)]
pub enum QuickAdd {
    Task(TaskDraft),
    Note(String),
    Event(EventDraft),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDraft {
    pub title: String,
    pub date: NaiveDate,
    /// `None` = all-day.
    pub start: Option<NaiveTime>,
    pub end: Option<NaiveTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskDraft {
    pub title: String,
    pub scheduled_for: NaiveDate,
    pub due: Option<NaiveDate>,
    /// Text after `#`, to be matched against project names by the caller.
    pub project_query: Option<String>,
    /// Steps from an `x3` token (1 = plain task).
    pub steps: i64,
}

pub fn parse(input: &str, today: NaiveDate) -> Option<QuickAdd> {
    let raw = input.trim_start();
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    for prefix in ["n ", "note ", "n:", "note:"] {
        if let Some(rest) = strip_prefix_ci(raw, prefix) {
            let body = rest.trim();
            return (!body.is_empty()).then(|| QuickAdd::Note(body.to_string()));
        }
    }
    for prefix in ["e ", "event ", "e:", "event:"] {
        if let Some(rest) = strip_prefix_ci(raw, prefix) {
            return parse_event(rest, today).map(QuickAdd::Event);
        }
    }

    let mut title = Vec::new();
    let mut due = None;
    let mut scheduled = None;
    let mut project = None;
    let mut steps = 1;
    for word in input.split_whitespace() {
        if let Some(n) = parse_steps(word) {
            steps = n;
        } else if let Some(d) = word.strip_prefix('!').and_then(|w| parse_date(w, today)) {
            due = Some(d);
        } else if let Some(d) = word.strip_prefix('@').and_then(|w| parse_date(w, today)) {
            scheduled = Some(d);
        } else if let Some(p) = word.strip_prefix('#').filter(|p| !p.is_empty()) {
            project = Some(p.to_string());
        } else {
            title.push(word);
        }
    }
    let title = title.join(" ");
    if title.is_empty() {
        return None;
    }
    Some(QuickAdd::Task(TaskDraft {
        title,
        scheduled_for: scheduled.unwrap_or(today),
        due,
        project_query: project,
        steps,
    }))
}

/// `x3` … `x99`: number of steps for a multi-part task.
fn parse_steps(word: &str) -> Option<i64> {
    let n: i64 = word.strip_prefix('x').or_else(|| word.strip_prefix('X'))?.parse().ok()?;
    (2..=99).contains(&n).then_some(n)
}

/// `Dentist @fri 9:15-10`, `Stream @sat 8pm`, `Rent due @2026-12-01` (no time = all day).
fn parse_event(rest: &str, today: NaiveDate) -> Option<EventDraft> {
    let mut title = Vec::new();
    let mut date = None;
    let mut start = None;
    let mut end = None;
    for word in rest.split_whitespace() {
        if let Some(d) = word.strip_prefix('@').and_then(|w| parse_date(w, today)) {
            date = Some(d);
        } else if let (None, Some((s, e))) = (start, parse_time_range(word)) {
            start = Some(s);
            end = e;
        } else {
            title.push(word);
        }
    }
    let title = title.join(" ");
    (!title.is_empty()).then(|| EventDraft { title, date: date.unwrap_or(today), start, end })
}

/// `9:15`, `14:30`, `9am`, `3pm`, `12:30pm`, optionally as a range `9-10:30` / `3pm-4pm`.
/// A bare number like `9` only counts as a time inside a range, so "ep 9" stays in the title.
pub fn parse_time_range(word: &str) -> Option<(NaiveTime, Option<NaiveTime>)> {
    let w = word.to_ascii_lowercase();
    if let Some((a, b)) = w.split_once('-') {
        let end = parse_time(b, true)?;
        // "3-4pm": the start borrows the end's am/pm.
        let suffix = if b.ends_with("pm") && !a.ends_with("am") && !a.ends_with("pm") { "pm" } else { "" };
        let start = parse_time(&format!("{a}{suffix}"), true)?;
        return Some((start, Some(end)));
    }
    parse_time(&w, false).map(|t| (t, None))
}

fn parse_time(s: &str, allow_bare: bool) -> Option<NaiveTime> {
    let (body, pm) = match (s.strip_suffix("pm"), s.strip_suffix("am")) {
        (Some(b), _) => (b, Some(true)),
        (_, Some(b)) => (b, Some(false)),
        _ => (s, None),
    };
    if pm.is_none() && !allow_bare && !body.contains(':') {
        return None;
    }
    let (h, m) = match body.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        Some(_) => return None,
        None => (body.parse::<u32>().ok()?, 0),
    };
    let h = match pm {
        Some(true) if (1..12).contains(&h) => h + 12,
        Some(false) if h == 12 => 0,
        Some(_) if !(1..=12).contains(&h) => return None,
        _ => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

pub fn parse_date(word: &str, today: NaiveDate) -> Option<NaiveDate> {
    let w = word.to_ascii_lowercase();
    match w.as_str() {
        "today" | "tod" => return Some(today),
        "tomorrow" | "tmr" | "tom" => return Some(today + Duration::days(1)),
        _ => {}
    }
    if let Some(n) = w.strip_prefix('+').and_then(|n| n.parse::<i64>().ok()) {
        return Some(today + Duration::days(n));
    }
    if let Ok(d) = NaiveDate::parse_from_str(&w, "%Y-%m-%d") {
        return Some(d);
    }
    const DAYS: [(&str, Weekday); 7] = [
        ("monday", Weekday::Mon),
        ("tuesday", Weekday::Tue),
        ("wednesday", Weekday::Wed),
        ("thursday", Weekday::Thu),
        ("friday", Weekday::Fri),
        ("saturday", Weekday::Sat),
        ("sunday", Weekday::Sun),
    ];
    if w.len() < 3 {
        return None;
    }
    let wd = DAYS.iter().find(|(full, _)| full.starts_with(w.as_str()))?.1;
    let ahead = (wd.num_days_from_monday() as i64 - today.weekday().num_days_from_monday() as i64).rem_euclid(7);
    Some(today + Duration::days(ahead))
}

/// Loose match for `#query` against a project name: case- and punctuation-insensitive prefix
/// of the whole name or of any word, falling back to substring.
pub fn project_matches(query: &str, name: &str) -> Option<u8> {
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let q = norm(query);
    if q.is_empty() {
        return None;
    }
    let n = norm(name);
    if n.starts_with(&q) {
        Some(3)
    } else if name.split_whitespace().any(|w| norm(w).starts_with(&q)) {
        Some(2)
    } else if n.contains(&q) {
        Some(1)
    } else {
        None
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len()).filter(|h| h.eq_ignore_ascii_case(prefix)).map(|_| &s[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wed() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap() // a Wednesday
    }

    #[test]
    fn plain_task() {
        let QuickAdd::Task(t) = parse("Export banner", wed()).unwrap() else { panic!() };
        assert_eq!(t.title, "Export banner");
        assert_eq!(t.scheduled_for, wed());
        assert_eq!(t.due, None);
    }

    #[test]
    fn tokens() {
        let QuickAdd::Task(t) = parse("Grade ep 7 !fri #survival @tomorrow", wed()).unwrap() else { panic!() };
        assert_eq!(t.title, "Grade ep 7");
        assert_eq!(t.due, NaiveDate::from_ymd_opt(2026, 11, 13));
        assert_eq!(t.scheduled_for, NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        assert_eq!(t.project_query.as_deref(), Some("survival"));
    }

    #[test]
    fn weekday_today_counts() {
        assert_eq!(parse_date("wed", wed()), Some(wed()));
        assert_eq!(parse_date("tue", wed()), NaiveDate::from_ymd_opt(2026, 11, 17));
        assert_eq!(parse_date("+3", wed()), NaiveDate::from_ymd_opt(2026, 11, 14));
        assert_eq!(parse_date("2026-12-01", wed()), NaiveDate::from_ymd_opt(2026, 12, 1));
        assert_eq!(parse_date("banana", wed()), None);
    }

    #[test]
    fn unknown_bang_word_stays_in_title() {
        let QuickAdd::Task(t) = parse("Wow !important", wed()).unwrap() else { panic!() };
        assert_eq!(t.title, "Wow !important");
    }

    #[test]
    fn note() {
        assert_eq!(parse("n  idea: shrink world", wed()), Some(QuickAdd::Note("idea: shrink world".into())));
        assert_eq!(parse("Note: x", wed()), Some(QuickAdd::Note("x".into())));
        assert_eq!(parse("n ", wed()), None);
        assert!(matches!(parse("now edit", wed()), Some(QuickAdd::Task(_))));
    }

    #[test]
    fn project_matching() {
        assert_eq!(project_matches("survival", "Hardcore Survival S3"), Some(2));
        assert_eq!(project_matches("hard", "Hardcore Survival S3"), Some(3));
        assert_eq!(project_matches("ppcurves", "PP Curves panel"), Some(3));
        assert_eq!(project_matches("zzz", "PP Curves panel"), None);
    }
}

#[cfg(test)]
mod event_tests {
    use super::*;

    fn wed() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }
    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn events() {
        let Some(QuickAdd::Event(e)) = parse("e Dentist @fri 9:15-10", wed()) else { panic!() };
        assert_eq!((e.title.as_str(), e.date, e.start, e.end), ("Dentist", NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(), Some(t(9, 15)), Some(t(10, 0))));

        let Some(QuickAdd::Event(e)) = parse("event Edit ep 9 review 3-4pm", wed()) else { panic!() };
        assert_eq!((e.title.as_str(), e.date, e.start, e.end), ("Edit ep 9 review", wed(), Some(t(15, 0)), Some(t(16, 0))));

        let Some(QuickAdd::Event(e)) = parse("e Rent due @+5", wed()) else { panic!() };
        assert_eq!((e.start, e.date.day()), (None, 16));

        assert_eq!(parse("e ", wed()), None);
        assert!(matches!(parse("Edit stuff", wed()), Some(QuickAdd::Task(_))));
    }

    #[test]
    fn times() {
        assert_eq!(parse_time_range("12am"), Some((t(0, 0), None)));
        assert_eq!(parse_time_range("12:30pm"), Some((t(12, 30), None)));
        assert_eq!(parse_time_range("8pm"), Some((t(20, 0), None)));
        assert_eq!(parse_time_range("22:05"), Some((t(22, 5), None)));
        assert_eq!(parse_time_range("9"), None);
        assert_eq!(parse_time_range("13pm"), None);
        assert_eq!(parse_time_range("25:00"), None);
        assert_eq!(parse_time_range("9-10:30"), Some((t(9, 0), Some(t(10, 30)))));
    }
}

#[cfg(test)]
mod step_token_tests {
    use super::*;

    #[test]
    fn x_token() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let Some(QuickAdd::Task(t)) = parse("Edit ep 8 x3 #survival", d) else { panic!() };
        assert_eq!((t.title.as_str(), t.steps), ("Edit ep 8", 3));
        let Some(QuickAdd::Task(t)) = parse("Render 1080x3 test x1 xbox", d) else { panic!() };
        assert_eq!((t.title.as_str(), t.steps), ("Render 1080x3 test x1 xbox", 1));
    }
}
