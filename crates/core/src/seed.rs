//! Realistic sample data, generated relative to "today" so the dashboard always looks current.
//! Every seeded row has `seed = 1`; `clear` removes only those rows, never your own data.

use crate::db::Db;
use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Transaction};

const STATE_KEY: &str = "seed.state"; // "seeded" | "cleared"

/// Tables that can contain seed rows, children first so deletes respect foreign keys.
const SEED_TABLES: &[&str] = &[
    "habit_checks",
    "tasks",
    "notes",
    "goals",
    "events",
    "expenses",
    "category_rules",
    "import_profiles",
    "expense_categories",
    "habits",
    "projects",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedState {
    Never,
    Seeded,
    Cleared,
}

pub fn state(db: &Db) -> SeedState {
    match db.setting(STATE_KEY).as_deref() {
        Some("seeded") => SeedState::Seeded,
        Some("cleared") => SeedState::Cleared,
        _ => SeedState::Never,
    }
}

pub fn seed_row_count(db: &Db) -> i64 {
    SEED_TABLES
        .iter()
        .map(|t| db.count(&format!("SELECT count(*) FROM {t} WHERE seed = 1")))
        .sum()
}

/// First launch only: insert sample data. Does nothing once seeded or cleared.
pub fn ensure(db: &mut Db, today: NaiveDate) -> Result<()> {
    if state(db) == SeedState::Never {
        insert(db, today)?;
    }
    Ok(())
}

/// Remove all sample rows. Your own rows are untouched (rows you linked to a sample
/// project simply lose the link).
pub fn clear(db: &mut Db) -> Result<()> {
    let tx = db.conn.transaction()?;
    for t in SEED_TABLES {
        tx.execute(&format!("DELETE FROM {t} WHERE seed = 1"), [])?;
    }
    tx.execute(
        "INSERT INTO settings(key,value) VALUES (?1,'cleared')
         ON CONFLICT(key) DO UPDATE SET value='cleared'",
        [STATE_KEY],
    )?;
    tx.commit()?;
    Ok(())
}

/// Clear, then insert a fresh copy of the sample data.
pub fn reload(db: &mut Db, today: NaiveDate) -> Result<()> {
    clear(db)?;
    insert(db, today)
}

fn insert(db: &mut Db, today: NaiveDate) -> Result<()> {
    let tx = db.conn.transaction()?;
    let ts = format!("{}T09:00:00", today - Duration::days(30));
    let projects = projects(&tx, &ts)?;
    tasks(&tx, today, &projects)?;
    events(&tx, today)?;
    notes(&tx, today, &projects)?;
    spending(&tx, today)?;
    habits(&tx, today)?;
    goals(&tx, today, &projects)?;
    tx.execute(
        "INSERT INTO settings(key,value) VALUES (?1,'seeded')
         ON CONFLICT(key) DO UPDATE SET value='seeded'",
        [STATE_KEY],
    )?;
    tx.commit()?;
    Ok(())
}

/// Tiny deterministic PRNG so sample data is stable between reloads.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
}

struct ProjectIds {
    survival: i64,
    redstone: i64,
    rebrand: i64,
    smp: i64,
    lut: i64,
    panel: i64,
}

fn projects(tx: &Transaction, ts: &str) -> Result<ProjectIds> {
    let rows: &[(&str, &str, &str, &str, i64, &str, &str)] = &[
        ("Hardcore Survival S3", "active", "100-day hardcore series. Episodes 1–10, ~25 min each.", "Color-grade episode 7", 62, "manual", "#4ade80"),
        ("Redstone in 60s", "planned", "Short-form redstone builds for Shorts and TikTok.", "Script the first five builds", 15, "manual", "#f87171"),
        ("Channel rebrand", "active", "New banner, intro sting, and thumbnail template.", "Export banner at all sizes", 40, "manual", "#7c8cff"),
        ("Modded SMP collab", "paused", "Collab series with four creators on a Create-mod server.", "Wait for the server pack update", 25, "manual", "#fbbf24"),
        ("Minecraft LUT pack", "idea", "Sellable color LUTs tuned for Minecraft biomes and shaders.", "Test a draft LUT on three biomes", 0, "manual", "#22d3ee"),
        ("PP Curves panel", "active", "Premiere panel for keyframe easing presets.", "Finish bake sampling", 0, "tasks", "#c084fc"),
        ("Thumbnail A/B test", "done", "Compared face vs. no-face thumbnails over six uploads.", "", 100, "manual", "#94a3b8"),
    ];
    let mut ids = Vec::new();
    for (i, (name, status, desc, next, prog, mode, color)) in rows.iter().enumerate() {
        tx.execute(
            "INSERT INTO projects(name,status,description,next_action,progress,progress_mode,color,position,created_at,updated_at,seed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,1)",
            params![name, status, desc, next, prog, mode, color, i as i64, ts],
        )?;
        ids.push(tx.last_insert_rowid());
    }
    Ok(ProjectIds { survival: ids[0], redstone: ids[1], rebrand: ids[2], smp: ids[3], lut: ids[4], panel: ids[5] })
}

fn tasks(tx: &Transaction, today: NaiveDate, p: &ProjectIds) -> Result<()> {
    let d = |n: i64| (today + Duration::days(n)).to_string();
    // (title, project, scheduled offset, due offset, done?, carried)
    let rows: Vec<(&str, Option<i64>, i64, Option<i64>, bool, i64)> = vec![
        ("Color-grade episode 7", Some(p.survival), 0, Some(1), false, 0),
        ("Sync game audio and voiceover for ep 8", Some(p.survival), 0, Some(3), false, 1),
        ("Export banner at 2560×1440 and 1546×423", Some(p.rebrand), 0, None, false, 0),
        ("Reply to sponsor email about January slot", None, 0, Some(0), false, 2),
        ("Back up raw footage to the NAS", None, 0, None, true, 0),
        ("Finish bake sampling in easing.js", Some(p.panel), 0, None, false, 0),
        ("Pick music for ep 7 outro", Some(p.survival), 0, None, true, 0),
        ("Record redstone door build", Some(p.redstone), 1, None, false, 0),
        ("Draft thumbnail for ep 8", Some(p.survival), 1, Some(4), false, 0),
        ("Renew domain", None, 2, Some(5), false, 0),
        ("Test draft LUT on desert, jungle, and cherry biomes", Some(p.lut), 3, None, false, 0),
        ("Ask the SMP group about the new pack date", Some(p.smp), 4, None, false, 0),
        // history (done), for project progress and stats
        ("Write the bezier math module", Some(p.panel), -6, None, true, 0),
        ("Add frame-safe clamp", Some(p.panel), -4, None, true, 0),
        ("Read Motion keyframes from the selected clip", Some(p.panel), -3, None, true, 0),
        ("Write preset JSON format", Some(p.panel), 2, None, false, 0),
        ("Add linked zoom/move", Some(p.panel), 3, None, false, 0),
        ("Upload episode 6", Some(p.survival), -2, None, true, 0),
        ("Make the intro sting v2", Some(p.rebrand), -1, None, true, 0),
    ];
    let mut pos_by_day = std::collections::HashMap::<i64, i64>::new();
    for (title, proj, sched, due, done, carried) in rows {
        let pos = pos_by_day.entry(sched).or_insert(0);
        *pos += 1;
        let done_at = done.then(|| format!("{}T16:30:00", d(sched.min(0))));
        tx.execute(
            "INSERT INTO tasks(title,project_id,due_date,scheduled_for,position,done_at,carried_count,created_at,seed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1)",
            params![title, proj, due.map(d), d(sched), *pos * 1000, done_at, carried, format!("{}T08:00:00", d(sched.min(0) - carried))],
        )?;
    }
    Ok(())
}

fn events(tx: &Transaction, today: NaiveDate) -> Result<()> {
    let at = |days: i64, hm: &str| format!("{}T{hm}:00", today + Duration::days(days));
    // (title, start day, start, end, all_day, location)
    let rows: &[(&str, i64, &str, Option<&str>, bool, &str)] = &[
        ("Episode 7 premiere", 1, "17:00", Some("17:30"), false, "YouTube"),
        ("Sponsor call — VPN brand", 2, "11:00", Some("11:30"), false, "Google Meet"),
        ("SMP collab recording", 3, "19:00", Some("22:00"), false, "Discord"),
        ("Dentist", 5, "09:15", Some("10:00"), false, "Main St Dental"),
        ("Episode 8 edit deadline", 6, "00:00", None, true, ""),
        ("Community stream", 7, "20:00", Some("22:00"), false, "Twitch"),
        ("Creator meetup", 9, "18:30", Some("21:00"), false, "Downtown"),
        ("Episode 8 premiere", 10, "17:00", Some("17:30"), false, "YouTube"),
        ("Rent due", 12, "00:00", None, true, ""),
        ("Thumbnail review with editor friend", 13, "15:00", Some("15:45"), false, "Discord"),
    ];
    for (title, day, start, end, all_day, loc) in rows {
        tx.execute(
            "INSERT INTO events(title,starts_at,ends_at,all_day,location,source,seed) VALUES (?1,?2,?3,?4,?5,'manual',1)",
            params![title, at(*day, start), end.map(|e| at(*day, e)), *all_day as i64, loc],
        )?;
    }
    Ok(())
}

fn notes(tx: &Transaction, today: NaiveDate, p: &ProjectIds) -> Result<()> {
    let rows: Vec<(&str, bool, Option<i64>, i64)> = vec![
        ("# Ep 7 edit notes\n\n- Cut the cave section by ~40s, it drags\n- **Zoom punch-in** on the creeper jump scare\n- Music: swap the 2nd track, too close to ep 5\n- [ ] Fix the audio pop at 12:41", true, Some(p.survival), 0),
        ("# Video ideas\n\n1. I built every villager job site in one base\n2. Redstone that *shouldn't* work\n3. 100 days but the world shrinks\n4. Ranking every biome for shader screenshots", true, None, 1),
        ("# Rebrand palette\n\n| Use | Hex |\n|---|---|\n| Primary | `#7c8cff` |\n| Accent | `#4ade80` |\n| Dark bg | `#0f1115` |", false, Some(p.rebrand), 3),
        ("# Sponsor talking points\n\n- 60s integration, mid-roll\n- Rate: ask for 20% more than last time\n- No scripted jokes", false, None, 2),
        ("# Easing presets to ship\n\n- Smooth in/out (0.42, 0, 0.58, 1)\n- Snappy (0.2, 0.9, 0.1, 1)\n- Bounce settle", false, Some(p.panel), 5),
    ];
    for (body, pinned, proj, age) in rows {
        let ts = format!("{}T12:00:00", today - Duration::days(age));
        tx.execute(
            "INSERT INTO notes(body_md,pinned,project_id,created_at,updated_at,seed) VALUES (?1,?2,?3,?4,?4,1)",
            params![body, pinned as i64, proj, ts],
        )?;
    }
    Ok(())
}

fn spending(tx: &Transaction, today: NaiveDate) -> Result<()> {
    // (name, color, budget cents)
    let cats: &[(&str, &str, Option<i64>)] = &[
        ("Software", "#7c8cff", Some(12_000)),
        ("Hardware", "#c084fc", Some(20_000)),
        ("Music & assets", "#22d3ee", Some(5_000)),
        ("Groceries", "#4ade80", Some(40_000)),
        ("Eating out", "#fbbf24", Some(15_000)),
        ("Transport", "#f87171", Some(8_000)),
        ("Bills", "#94a3b8", None),
        ("Fun", "#f472b6", Some(6_000)),
    ];
    let mut cat_ids = Vec::new();
    for (i, (name, color, budget)) in cats.iter().enumerate() {
        tx.execute(
            "INSERT INTO expense_categories(name,color,monthly_budget_cents,position,seed) VALUES (?1,?2,?3,?4,1)",
            params![name, color, budget, i as i64],
        )?;
        cat_ids.push(tx.last_insert_rowid());
    }

    // Fixed monthly charges: (day, cents, description, merchant, cat idx)
    let fixed: &[(u32, i64, &str, &str, usize)] = &[
        (1, 140_000, "Rent", "Landlord", 6),
        (3, 5_999, "Creative Cloud", "Adobe", 0),
        (5, 1_599, "Music licence", "Music library", 2),
        (8, 1_199, "Streaming", "Streaming service", 7),
        (12, 6_500, "Internet", "ISP", 6),
        (15, 1_000, "Cloud storage 2TB", "Cloud storage", 0),
        (20, 4_800, "Phone", "Carrier", 6),
    ];
    // Variable spend templates: (description, merchant, cat idx, min, max)
    let variable: &[(&str, &str, usize, i64, i64)] = &[
        ("Groceries", "Supermarket", 3, 2_500, 9_500),
        ("Groceries", "Corner shop", 3, 600, 2_200),
        ("Lunch", "Cafe", 4, 900, 1_900),
        ("Takeout", "Pizza place", 4, 1_800, 3_400),
        ("Coffee", "Coffee shop", 4, 350, 650),
        ("Ride", "Rideshare", 5, 900, 2_600),
        ("Transit card top-up", "Transit", 5, 2_000, 2_000),
        ("Sound effects pack", "Asset store", 2, 1_200, 3_500),
        ("Game", "Game store", 7, 1_500, 4_000),
    ];

    let mut rng = Lcg(0xC0FFEE);
    let this_month = today.with_day(1).unwrap();
    let months = [prev_month(prev_month(this_month)), prev_month(this_month), this_month];
    for (mi, month) in months.iter().enumerate() {
        let last_day = if mi == 2 { today.day() } else { days_in_month(*month) };
        for (day, cents, desc, merchant, cat) in fixed {
            if *day <= last_day {
                insert_expense(tx, month.with_day(*day).unwrap(), *cents, desc, merchant, cat_ids[*cat])?;
            }
        }
        for day in 1..=last_day {
            let n = rng.range(0, 2);
            for _ in 0..n {
                let (desc, merchant, cat, lo, hi) = variable[rng.range(0, variable.len() as i64 - 1) as usize];
                insert_expense(tx, month.with_day(day).unwrap(), rng.range(lo, hi), desc, merchant, cat_ids[cat])?;
            }
        }
    }
    // A one-off hardware purchase last month makes the comparison interesting.
    insert_expense(tx, prev_month(this_month).with_day(18).unwrap(), 17_999, "2TB NVMe SSD for footage", "Electronics store", cat_ids[1])?;

    tx.execute(
        "INSERT INTO import_profiles(name,mapping,seed) VALUES ('Generic CSV',?1,1)",
        [r#"{"date":"Date","amount":"Amount","description":"Description","date_format":"%Y-%m-%d","negative_is_expense":true}"#],
    )?;
    for (pattern, cat) in [("adobe", 0), ("rideshare", 5), ("supermarket", 3), ("cafe", 4), ("coffee", 4), ("asset store", 2)] {
        tx.execute("INSERT INTO category_rules(pattern,category_id,seed) VALUES (?1,?2,1)", params![pattern, cat_ids[cat]])?;
    }
    Ok(())
}

fn insert_expense(tx: &Transaction, date: NaiveDate, cents: i64, desc: &str, merchant: &str, cat: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO expenses(date,amount_cents,description,merchant,category_id,source,seed) VALUES (?1,?2,?3,?4,?5,'manual',1)",
        params![date.to_string(), cents, desc, merchant, cat],
    )?;
    Ok(())
}

fn habits(tx: &Transaction, today: NaiveDate) -> Result<()> {
    // (name, cadence, target, color, daily check chance %, current streak days)
    let rows: &[(&str, &str, i64, &str, u64, i64)] = &[
        ("Edit for 2 hours", "daily", 1, "#7c8cff", 80, 9),
        ("Read 20 minutes", "daily", 1, "#4ade80", 55, 3),
        ("No caffeine after 3pm", "daily", 1, "#fbbf24", 65, 0),
        ("Workout", "weekly", 3, "#f87171", 45, 0),
        ("Post a Short", "weekly", 2, "#22d3ee", 35, 0),
    ];
    let mut rng = Lcg(42);
    let created = format!("{}T08:00:00", today - Duration::days(150));
    for (i, (name, cadence, target, color, chance, streak)) in rows.iter().enumerate() {
        tx.execute(
            "INSERT INTO habits(name,cadence,target_per_period,color,position,created_at,seed) VALUES (?1,?2,?3,?4,?5,?6,1)",
            params![name, cadence, target, color, i as i64, created],
        )?;
        let id = tx.last_insert_rowid();
        for back in 1..=140i64 {
            let checked = if back <= *streak { true } else if back == *streak + 1 && *streak > 0 { false } else { rng.chance(*chance) };
            if checked {
                tx.execute(
                    "INSERT INTO habit_checks(habit_id,date,seed) VALUES (?1,?2,1)",
                    params![id, (today - Duration::days(back)).to_string()],
                )?;
            }
        }
    }
    Ok(())
}

fn goals(tx: &Transaction, today: NaiveDate, p: &ProjectIds) -> Result<()> {
    let year_end = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap();
    let rows: Vec<(&str, &str, f64, f64, f64, NaiveDate, Option<i64>)> = vec![
        ("Reach 50k subscribers", "subs", 31_200.0, 42_850.0, 50_000.0, today + Duration::days(120), None),
        ("Publish 24 long-form videos this year", "videos", 0.0, 19.0, 24.0, year_end, None),
        ("Finish the 100-day series", "days", 0.0, 64.0, 100.0, today + Duration::days(45), Some(p.survival)),
        ("Emergency fund", "$", 0.0, 3_400.0, 6_000.0, today + Duration::days(200), None),
    ];
    let ts = format!("{}T08:00:00", today - Duration::days(60));
    for (title, unit, start, cur, target, deadline, proj) in rows {
        tx.execute(
            "INSERT INTO goals(title,unit,start_value,current_value,target_value,deadline,project_id,created_at,seed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1)",
            params![title, unit, start, cur, target, deadline.to_string(), proj, ts],
        )?;
    }
    let _ = (p.redstone, p.lut); // referenced by tasks; kept for clarity
    Ok(())
}

pub fn prev_month(first: NaiveDate) -> NaiveDate {
    if first.month() == 1 {
        NaiveDate::from_ymd_opt(first.year() - 1, 12, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() - 1, 1).unwrap()
    }
}

pub fn days_in_month(first: NaiveDate) -> u32 {
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1).unwrap()
    };
    (next - first).num_days() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 2).unwrap() // early in a month, Jan rollover covered below
    }

    #[test]
    fn seeds_once_and_clears_only_seed_rows() {
        let mut db = Db::open_in_memory().unwrap();
        ensure(&mut db, day()).unwrap();
        let n = seed_row_count(&db);
        assert!(n > 100, "expected a full sample set, got {n}");
        ensure(&mut db, day()).unwrap();
        assert_eq!(seed_row_count(&db), n, "ensure must not double-seed");

        db.conn
            .execute("INSERT INTO tasks(title,scheduled_for,created_at) VALUES ('mine','2026-03-02','2026-03-02T10:00:00')", [])
            .unwrap();
        clear(&mut db).unwrap();
        assert_eq!(seed_row_count(&db), 0);
        assert_eq!(db.count("SELECT count(*) FROM tasks"), 1);
        assert_eq!(state(&db), SeedState::Cleared);

        ensure(&mut db, day()).unwrap();
        assert_eq!(seed_row_count(&db), 0, "cleared stays cleared");
        reload(&mut db, day()).unwrap();
        assert_eq!(seed_row_count(&db), n);
    }

    #[test]
    fn seeds_in_january() {
        let mut db = Db::open_in_memory().unwrap();
        ensure(&mut db, NaiveDate::from_ymd_opt(2027, 1, 31).unwrap()).unwrap();
        assert!(seed_row_count(&db) > 100);
    }
}
