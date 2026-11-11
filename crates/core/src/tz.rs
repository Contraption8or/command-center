//! Time zones and Discord timestamps.
//!
//! Events are stored in local wall-clock time. These helpers convert a local time to other
//! IANA zones (and back), and build Discord's `<t:UNIX:STYLE>` tags, which every Discord
//! user sees rendered in their own time zone.

use chrono::{DateTime, Local, LocalResult, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// A zone shown in the timezone panel, e.g. `{ label: "Alex", tz: "Europe/London" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub label: String,
    pub tz: String,
}

pub fn default_zones() -> Vec<Zone> {
    [
        ("Los Angeles", "America/Los_Angeles"),
        ("New York", "America/New_York"),
        ("London", "Europe/London"),
        ("Berlin", "Europe/Berlin"),
        ("Sydney", "Australia/Sydney"),
    ]
    .into_iter()
    .map(|(l, t)| Zone { label: l.into(), tz: t.into() })
    .collect()
}

pub fn parse(tz: &str) -> Option<Tz> {
    tz.parse::<Tz>().ok()
}

/// All IANA zone names (for the "add zone" picker).
pub fn all_names() -> impl Iterator<Item = &'static str> {
    chrono_tz::TZ_VARIANTS.iter().map(|t| t.name())
}

/// Interpret a local wall-clock time. In a DST gap the time doesn't exist, so it moves
/// forward an hour; in an overlap the earlier instant wins.
pub fn local_instant(t: NaiveDateTime) -> DateTime<Local> {
    match Local.from_local_datetime(&t) {
        LocalResult::Single(d) => d,
        LocalResult::Ambiguous(a, _) => a,
        LocalResult::None => Local.from_local_datetime(&(t + chrono::Duration::hours(1))).earliest().unwrap_or_else(|| Local::now()),
    }
}

pub fn unix(t: NaiveDateTime) -> i64 {
    local_instant(t).timestamp()
}

/// The same instant as local `t`, as wall-clock time in `zone`.
pub fn to_zone(t: NaiveDateTime, zone: Tz) -> NaiveDateTime {
    local_instant(t).with_timezone(&zone).naive_local()
}

/// A wall-clock time in `zone`, converted to local wall-clock time.
pub fn from_zone(t: NaiveDateTime, zone: Tz) -> NaiveDateTime {
    let instant = match zone.from_local_datetime(&t) {
        LocalResult::Single(d) => d,
        LocalResult::Ambiguous(a, _) => a,
        LocalResult::None => zone.from_local_datetime(&(t + chrono::Duration::hours(1))).earliest().unwrap_or_else(|| Utc::now().with_timezone(&zone)),
    };
    instant.with_timezone(&Local).naive_local()
}

/// Zone abbreviation or offset at instant `t` (local), e.g. "GMT", "CET", "+1030".
pub fn abbreviation(t: NaiveDateTime, zone: Tz) -> String {
    local_instant(t).with_timezone(&zone).format("%Z").to_string()
}

/// Offset from local time in hours at local instant `t`, e.g. -8.0, +5.5.
pub fn offset_from_local_hours(t: NaiveDateTime, zone: Tz) -> f64 {
    let inst = local_instant(t);
    let there = inst.with_timezone(&zone).offset().fix().local_minus_utc();
    let here = inst.offset().fix().local_minus_utc();
    (there - here) as f64 / 3600.0
}

/// Discord timestamp styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscordStyle {
    /// `t` — 3:00 PM
    ShortTime,
    /// `T` — 3:00:00 PM
    LongTime,
    /// `d` — 11/13/2026
    ShortDate,
    /// `D` — November 13, 2026
    LongDate,
    /// `f` — November 13, 2026 3:00 PM
    ShortDateTime,
    /// `F` — Friday, November 13, 2026 3:00 PM
    LongDateTime,
    /// `R` — in 2 days
    Relative,
}

impl DiscordStyle {
    pub const MENU: [DiscordStyle; 5] =
        [DiscordStyle::LongDateTime, DiscordStyle::ShortDateTime, DiscordStyle::ShortTime, DiscordStyle::LongDate, DiscordStyle::Relative];

    pub fn code(self) -> char {
        match self {
            DiscordStyle::ShortTime => 't',
            DiscordStyle::LongTime => 'T',
            DiscordStyle::ShortDate => 'd',
            DiscordStyle::LongDate => 'D',
            DiscordStyle::ShortDateTime => 'f',
            DiscordStyle::LongDateTime => 'F',
            DiscordStyle::Relative => 'R',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DiscordStyle::ShortTime => "Time",
            DiscordStyle::LongTime => "Time with seconds",
            DiscordStyle::ShortDate => "Short date",
            DiscordStyle::LongDate => "Date",
            DiscordStyle::ShortDateTime => "Date and time",
            DiscordStyle::LongDateTime => "Weekday, date and time",
            DiscordStyle::Relative => "Relative (in 2 days)",
        }
    }

    /// Roughly what Discord will show (in *your* zone; others see theirs).
    pub fn preview(self, ts: i64, now: i64) -> String {
        let t = Local.timestamp_opt(ts, 0).single().unwrap_or_else(Local::now);
        match self {
            DiscordStyle::ShortTime => t.format("%-I:%M %p").to_string(),
            DiscordStyle::LongTime => t.format("%-I:%M:%S %p").to_string(),
            DiscordStyle::ShortDate => t.format("%m/%d/%Y").to_string(),
            DiscordStyle::LongDate => t.format("%B %-d, %Y").to_string(),
            DiscordStyle::ShortDateTime => t.format("%B %-d, %Y %-I:%M %p").to_string(),
            DiscordStyle::LongDateTime => t.format("%A, %B %-d, %Y %-I:%M %p").to_string(),
            DiscordStyle::Relative => relative(ts - now),
        }
    }
}

pub fn discord_tag(ts: i64, style: DiscordStyle) -> String {
    format!("<t:{ts}:{}>", style.code())
}

/// "in 2 days", "3 hours ago", "in 5 minutes".
pub fn relative(secs: i64) -> String {
    let a = secs.abs();
    let (n, unit) = match a {
        0..=59 => (a, "second"),
        60..=3599 => (a / 60, "minute"),
        3600..=86_399 => (a / 3600, "hour"),
        86_400..=2_591_999 => (a / 86_400, "day"),
        2_592_000..=31_535_999 => (a / 2_592_000, "month"),
        _ => (a / 31_536_000, "year"),
    };
    let unit = if n == 1 { unit.to_string() } else { format!("{unit}s") };
    if secs >= 0 { format!("in {n} {unit}") } else { format!("{n} {unit} ago") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    #[test]
    fn zone_round_trip() {
        let london = parse("Europe/London").unwrap();
        let tokyo = parse("Asia/Tokyo").unwrap();
        let local = utc(2026, 11, 13, 15, 0);
        let there = to_zone(local, tokyo);
        assert_eq!(from_zone(there, tokyo), local);
        // London and Tokyo are 9 hours apart in November regardless of the machine's zone.
        assert_eq!(to_zone(local, tokyo) - to_zone(local, london), chrono::Duration::hours(9));
        assert!(parse("Mars/Olympus").is_none());
        assert!(all_names().any(|n| n == "America/Los_Angeles"));
    }

    #[test]
    fn discord_tags_and_relative() {
        assert_eq!(discord_tag(1_794_000_000, DiscordStyle::LongDateTime), "<t:1794000000:F>");
        assert_eq!(discord_tag(5, DiscordStyle::Relative), "<t:5:R>");
        assert_eq!(relative(2 * 86_400 + 5), "in 2 days");
        assert_eq!(relative(-3 * 3600), "3 hours ago");
        assert_eq!(relative(60), "in 1 minute");
        // Unix time for a local instant matches chrono's own conversion.
        let t = utc(2026, 11, 13, 15, 0);
        assert_eq!(unix(t), Local.from_local_datetime(&t).earliest().unwrap().timestamp());
    }
}
