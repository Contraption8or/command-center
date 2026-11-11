//! Claude planning assistant: what Claude sees (a compact snapshot of your data), the changes it
//! may *propose* (never apply), and how an approved change is applied.
//!
//! Claude only ever calls two tools: `propose_changes`, which the app shows as cards you approve
//! one by one, and `read_note`, which returns a note's text. Nothing changes without a click.

pub mod api;

use crate::db::Db;
use crate::model::events::{self, EventInput};
use crate::model::{goals, habits, notes, projects, tasks};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Write as _;

/// The instructions are fixed for the life of a conversation (the API requires the system
/// prompt and tools to stay byte-identical across turns); live data goes in user messages.
pub const SYSTEM_PROMPT: &str = "\
You are the planning assistant inside Command Center, a personal dashboard used by a video \
editor who makes Minecraft YouTube content and juggles several side projects. You help them \
sort, prioritise and schedule their tasks, events, projects, habits and goals.

Each user message may start with a <context> block: a snapshot of their current data with ids, \
the current local date and time, and notes on changes they applied or skipped since your last \
reply. Treat the newest snapshot as the truth. Times are the user's local time.

You cannot change anything directly. To suggest changes, call propose_changes once with every \
change you recommend; the user approves or skips each one. Never say a change has been made \
unless a later context block says it was applied. Use ids exactly as they appear in the snapshot. \
Use read_note when a note's full text matters for the answer.

Be practical and concise. Respect their existing commitments and deadlines, keep realistic room \
for editing and recording work, and prefer a few well-placed changes over reshuffling everything. \
Write in plain, friendly language; short lists are fine.";

/// Tool definitions sent with every request (kept identical across a conversation).
pub fn tools() -> Value {
    let id = json!({ "type": "integer", "description": "Id from the snapshot" });
    let date = json!({ "type": "string", "description": "YYYY-MM-DD" });
    let time = json!({ "type": "string", "description": "HH:MM, 24-hour" });
    json!([
        {
            "name": "propose_changes",
            "description": "Propose one or more changes to the user's tasks, events, projects or notes. The user sees each change as a card and applies or skips it; nothing changes until they do. Call this at most once per reply, with all your suggestions.",
            "eager_input_streaming": true,
            "input_schema": {
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "One sentence describing the plan" },
                    "changes": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": ["create_task", "update_task", "complete_task", "delete_task",
                                             "create_event", "move_event", "delete_event",
                                             "update_project", "create_note"]
                                },
                                "id": id,
                                "title": { "type": "string" },
                                "date": date,
                                "due": { "type": "string", "description": "YYYY-MM-DD, or \"none\" to clear the due date" },
                                "start": time,
                                "end": time,
                                "steps": { "type": "integer", "description": "Split a task into this many parts (2-99)" },
                                "project_id": { "type": "integer", "description": "Project id, or 0 to unlink" },
                                "location": { "type": "string" },
                                "status": { "type": "string", "enum": ["active", "planned", "paused", "idea", "done"] },
                                "next_action": { "type": "string" },
                                "body": { "type": "string", "description": "Markdown text for create_note" },
                                "reason": { "type": "string", "description": "Why, in a few words" }
                            },
                            "required": ["action"]
                        }
                    }
                },
                "required": ["summary", "changes"]
            }
        },
        {
            "name": "read_note",
            "description": "Read the full Markdown text of one note by its id.",
            "eager_input_streaming": true,
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }
        }
    ])
}

/// One proposed change. Fields not used by an action are ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub action: String,
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub steps: Option<i64>,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Proposal {
    pub summary: String,
    pub changes: Vec<Change>,
}

/// Parse and sanity-check `propose_changes` input. Errors go back to Claude as `is_error`.
pub fn parse_proposal(input: &Value) -> Result<Proposal> {
    let p: Proposal = serde_json::from_value(input.clone()).context("input doesn't match the propose_changes schema")?;
    if p.changes.is_empty() {
        bail!("changes is empty");
    }
    for (i, c) in p.changes.iter().enumerate() {
        validate(c).with_context(|| format!("change {} ({})", i + 1, c.action))?;
    }
    Ok(p)
}

fn d(s: &Option<String>, field: &str) -> Result<Option<NaiveDate>> {
    s.as_deref()
        .map(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| anyhow!("{field} must be YYYY-MM-DD, got {v:?}")))
        .transpose()
}

fn t(s: &Option<String>, field: &str) -> Result<Option<NaiveTime>> {
    s.as_deref()
        .map(|v| NaiveTime::parse_from_str(v, "%H:%M").map_err(|_| anyhow!("{field} must be HH:MM, got {v:?}")))
        .transpose()
}

fn need_id(c: &Change) -> Result<i64> {
    c.id.ok_or_else(|| anyhow!("id is required"))
}

fn validate(c: &Change) -> Result<()> {
    d(&c.date, "date")?;
    if c.due.as_deref() != Some("none") {
        d(&c.due, "due")?;
    }
    t(&c.start, "start")?;
    t(&c.end, "end")?;
    match c.action.as_str() {
        "create_task" => {
            c.title.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| anyhow!("title is required"))?;
        }
        "update_task" | "complete_task" | "delete_task" | "delete_event" | "update_project" => {
            need_id(c)?;
        }
        "create_event" => {
            c.title.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| anyhow!("title is required"))?;
            c.date.as_ref().ok_or_else(|| anyhow!("date is required"))?;
        }
        "move_event" => {
            need_id(c)?;
            c.date.as_ref().ok_or_else(|| anyhow!("date is required"))?;
        }
        "create_note" => {
            c.body.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| anyhow!("body is required"))?;
        }
        other => bail!("unknown action {other:?}"),
    }
    if let Some(s) = c.steps {
        if !(1..=99).contains(&s) {
            bail!("steps must be 1-99");
        }
    }
    Ok(())
}

fn task_name(db: &Db, id: i64) -> String {
    tasks::get(db, id).map(|t| format!("“{}”", t.title)).unwrap_or_else(|| format!("task #{id}"))
}

fn fmt_day(s: &str) -> String {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.format("%a %b %-d").to_string()).unwrap_or_else(|_| s.to_string())
}

fn fmt_hm(s: &str, h12: bool) -> String {
    NaiveTime::parse_from_str(s, "%H:%M").map(|t| events::fmt_time(t, h12)).unwrap_or_else(|_| s.to_string())
}

/// Human-readable one-liner for a proposal card.
pub fn describe(db: &Db, c: &Change, h12: bool) -> String {
    let title = c.title.clone().unwrap_or_default();
    match c.action.as_str() {
        "create_task" => {
            let mut s = format!("Add task “{title}”");
            if let Some(dt) = &c.date { let _ = write!(s, " on {}", fmt_day(dt)); }
            if let Some(due) = c.due.as_deref().filter(|d| *d != "none") { let _ = write!(s, ", due {}", fmt_day(due)); }
            if let Some(n) = c.steps.filter(|n| *n > 1) { let _ = write!(s, ", {n} steps"); }
            if let Some(p) = c.project_id.and_then(|p| projects::get(db, p)) { let _ = write!(s, " · {}", p.name); }
            s
        }
        "update_task" => {
            let mut parts = Vec::new();
            if let Some(tl) = &c.title { parts.push(format!("rename to “{tl}”")); }
            if let Some(dt) = &c.date { parts.push(format!("move to {}", fmt_day(dt))); }
            match c.due.as_deref() {
                Some("none") => parts.push("clear due date".into()),
                Some(due) => parts.push(format!("due {}", fmt_day(due))),
                None => {}
            }
            if let Some(n) = c.steps { parts.push(format!("{n} steps")); }
            if let Some(p) = c.project_id {
                parts.push(if p == 0 { "unlink project".into() } else { format!("link to {}", projects::get(db, p).map(|p| p.name).unwrap_or(format!("project #{p}"))) });
            }
            format!("Task {}: {}", task_name(db, c.id.unwrap_or(0)), if parts.is_empty() { "no change".into() } else { parts.join(", ") })
        }
        "complete_task" => format!("Mark {} done", task_name(db, c.id.unwrap_or(0))),
        "delete_task" => format!("Delete task {}", task_name(db, c.id.unwrap_or(0))),
        "create_event" => {
            let when = match (&c.start, &c.end) {
                (Some(s), Some(e)) => format!("{}–{}", fmt_hm(s, h12), fmt_hm(e, h12)),
                (Some(s), None) => fmt_hm(s, h12),
                _ => "all day".into(),
            };
            format!("Add event “{title}” · {} {when}", c.date.as_deref().map(fmt_day).unwrap_or_default())
        }
        "move_event" => {
            let name = c.id.and_then(|i| events::get(db, i)).map(|e| format!("“{}”", e.title)).unwrap_or_else(|| "event".into());
            let when = match (&c.start, &c.end) {
                (Some(s), Some(e)) => format!(" {}–{}", fmt_hm(s, h12), fmt_hm(e, h12)),
                (Some(s), None) => format!(" {}", fmt_hm(s, h12)),
                _ => String::new(),
            };
            format!("Move {name} to {}{when}", c.date.as_deref().map(fmt_day).unwrap_or_default())
        }
        "delete_event" => {
            let name = c.id.and_then(|i| events::get(db, i)).map(|e| format!("“{}”", e.title)).unwrap_or_else(|| "event".into());
            format!("Delete event {name}")
        }
        "update_project" => {
            let name = c.id.and_then(|i| projects::get(db, i)).map(|p| p.name).unwrap_or_else(|| "project".into());
            let mut parts = Vec::new();
            if let Some(s) = &c.status { parts.push(format!("status {}", projects::status_label(s))); }
            if let Some(n) = &c.next_action { parts.push(format!("next: {n}")); }
            format!("Project {name}: {}", parts.join(", "))
        }
        "create_note" => {
            let first = c.body.as_deref().map(crate::markdown::title).unwrap_or_default();
            format!("Add note “{first}”")
        }
        other => format!("Unknown change ({other})"),
    }
}

/// Apply one approved change. Returns a short past-tense description for Claude's next context.
pub fn apply(db: &mut Db, c: &Change, today: NaiveDate) -> Result<String> {
    validate(c)?;
    let date = d(&c.date, "date")?;
    let due = if c.due.as_deref() == Some("none") { None } else { d(&c.due, "due")? };
    match c.action.as_str() {
        "create_task" => {
            let title = c.title.clone().unwrap_or_default();
            let project = c.project_id.filter(|p| *p > 0 && projects::get(db, *p).is_some());
            let id = tasks::add_steps(db, &title, date.unwrap_or(today), due, project, c.steps.unwrap_or(1))?;
            Ok(format!("created task #{id} “{title}”"))
        }
        "update_task" => {
            let id = need_id(c)?;
            let t = tasks::get(db, id).ok_or_else(|| anyhow!("task #{id} no longer exists"))?;
            if let Some(title) = &c.title { tasks::set_title(db, id, title)?; }
            if let Some(dt) = date { tasks::reschedule(db, id, dt)?; }
            if c.due.is_some() { tasks::set_due(db, id, due)?; }
            if let Some(n) = c.steps { tasks::set_steps(db, id, n)?; }
            if let Some(p) = c.project_id { tasks::set_project(db, id, (p > 0).then_some(p))?; }
            Ok(format!("updated task #{id} “{}”", t.title))
        }
        "complete_task" => {
            let id = need_id(c)?;
            let t = tasks::get(db, id).ok_or_else(|| anyhow!("task #{id} no longer exists"))?;
            tasks::set_done(db, id, true)?;
            Ok(format!("completed task #{id} “{}”", t.title))
        }
        "delete_task" => {
            let id = need_id(c)?;
            let t = tasks::delete(db, id)?.ok_or_else(|| anyhow!("task #{id} no longer exists"))?;
            Ok(format!("deleted task #{id} “{}”", t.title))
        }
        "create_event" => {
            let title = c.title.clone().unwrap_or_default();
            let input = EventInput {
                title: title.clone(),
                date: date.ok_or_else(|| anyhow!("date is required"))?,
                start: t(&c.start, "start")?,
                end: t(&c.end, "end")?.or_else(|| t(&c.start, "start").ok().flatten().map(|s| s + Duration::hours(1))),
                end_date: None,
                location: c.location.clone().unwrap_or_default(),
                notes: String::new(),
            };
            let id = events::add(db, &input)?;
            Ok(format!("created event #{id} “{title}”"))
        }
        "move_event" => {
            let id = need_id(c)?;
            let e = events::get(db, id).ok_or_else(|| anyhow!("event #{id} no longer exists"))?;
            let mut input = events::input_of(&e);
            input.date = date.ok_or_else(|| anyhow!("date is required"))?;
            if let Some(s) = t(&c.start, "start")? {
                let dur = e.ends_at.map(|x| x - e.starts_at).unwrap_or(Duration::hours(1));
                input.start = Some(s);
                input.end = Some(t(&c.end, "end")?.unwrap_or(s + dur));
            }
            input.end_date = None;
            events::update(db, id, &input)?;
            Ok(format!("moved event #{id} “{}”", e.title))
        }
        "delete_event" => {
            let id = need_id(c)?;
            let e = events::delete(db, id)?.ok_or_else(|| anyhow!("event #{id} can't be deleted (missing or synced)"))?;
            Ok(format!("deleted event #{id} “{}”", e.title))
        }
        "update_project" => {
            let id = need_id(c)?;
            let p = projects::get(db, id).ok_or_else(|| anyhow!("project #{id} no longer exists"))?;
            if let Some(s) = &c.status { projects::set_status(db, id, s)?; }
            if let Some(n) = &c.next_action { projects::set_next_action(db, id, n)?; }
            Ok(format!("updated project #{id} “{}”", p.name))
        }
        "create_note" => {
            let body = c.body.clone().unwrap_or_default();
            let project = c.project_id.filter(|p| *p > 0 && projects::get(db, *p).is_some());
            let id = notes::add(db, &body, project)?;
            Ok(format!("created note #{id}"))
        }
        other => bail!("unknown action {other:?}"),
    }
}

/// Compact plain-text snapshot of everything Claude may plan with.
pub fn snapshot(db: &Db, now: NaiveDateTime, h12: bool) -> String {
    let today = now.date();
    let mut s = String::new();
    let tf = |t: NaiveTime| events::fmt_time(t, h12);
    let _ = writeln!(s, "Now: {} {} (week starts Monday)", now.format("%A %Y-%m-%d"), tf(now.time()));

    let _ = writeln!(s, "\nTasks for today (open first):");
    for t in tasks::for_day(db, today) {
        let _ = writeln!(s, "- #{} {}{}", t.id, t.title, task_meta(&t, today));
    }
    let up = tasks::upcoming(db, today);
    if !up.is_empty() {
        let _ = writeln!(s, "\nUpcoming tasks (by day):");
        for t in up.iter().take(60) {
            let _ = writeln!(s, "- #{} [{}] {}{}", t.id, t.scheduled_for.format("%a %m-%d"), t.title, task_meta(t, today));
        }
    }

    let evs = events::between(db, today, today + Duration::days(20));
    let _ = writeln!(s, "\nEvents, next 3 weeks:");
    for e in &evs {
        let when = if e.all_day { "all day".to_string() } else { e.time_label(h12) };
        let loc = if e.location.is_empty() { String::new() } else { format!(" @ {}", e.location) };
        let ro = if e.is_manual() { "" } else { " (synced, read-only)" };
        let _ = writeln!(s, "- #{} {} {} {}{loc}{ro}", e.id, e.start_date().format("%a %Y-%m-%d"), when, e.title);
    }

    let _ = writeln!(s, "\nProjects:");
    for p in projects::list(db).iter().filter(|p| p.status != "done") {
        let next = if p.next_action.is_empty() { String::new() } else { format!(" · next: {}", p.next_action) };
        let _ = writeln!(s, "- #{} {} [{}] {}%{next}", p.id, p.name, p.status, p.percent());
    }

    let hs = habits::list(db, false);
    if !hs.is_empty() {
        let _ = writeln!(s, "\nHabits:");
        for h in hs {
            let st = h.stats(today);
            let cadence = if h.is_weekly() { format!("{}x/week, {} this week", h.target, st.this_period) } else {
                format!("daily, {}", if st.this_period > 0 { "done today" } else { "not done today" })
            };
            let _ = writeln!(s, "- {} ({cadence}; streak {})", h.name, st.current);
        }
    }

    let gs = goals::list(db);
    if !gs.is_empty() {
        let _ = writeln!(s, "\nGoals:");
        for g in gs.iter().filter(|g| !g.is_done()) {
            let dl = g.deadline.map(|d| format!(", due {d}")).unwrap_or_default();
            let _ = writeln!(s, "- {}: {} / {} {} ({}%{dl}, {:?})", g.title, goals::fmt_num(g.current), goals::fmt_num(g.target), g.unit, g.percent(), g.pace(today));
        }
    }

    let ns = notes::list(db, "");
    if !ns.is_empty() {
        let _ = writeln!(s, "\nNotes (use read_note for the text):");
        for n in ns.iter().take(25) {
            let pin = if n.pinned { " (pinned)" } else { "" };
            let proj = n.project_name.as_deref().map(|p| format!(" · {p}")).unwrap_or_default();
            let _ = writeln!(s, "- #{} {}{pin}{proj}", n.id, n.title());
        }
    }
    s
}

fn task_meta(t: &tasks::Task, today: NaiveDate) -> String {
    let mut m = Vec::new();
    if t.done { m.push("done".to_string()); }
    if let Some(d) = t.due_date { m.push(if d < today && !t.done { format!("OVERDUE since {d}") } else { format!("due {d}") }); }
    if t.has_steps() && !t.done { m.push(format!("{}/{} steps", t.steps_done, t.steps_total)); }
    if t.carried_count > 0 && !t.done { m.push(format!("slipped {}d", t.carried_count)); }
    if let Some(p) = &t.project_name { m.push(p.clone()); }
    if m.is_empty() { String::new() } else { format!(" ({})", m.join(", ")) }
}

/// Text for a `read_note` call.
pub fn read_note(db: &Db, input: &Value) -> Result<String> {
    let id = input.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("id is required"))?;
    let n = notes::get(db, id).ok_or_else(|| anyhow!("note #{id} not found"))?;
    Ok(n.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }

    #[test]
    fn proposal_parsing_and_validation() {
        let ok = json!({"summary": "s", "changes": [
            {"action": "create_task", "title": "Edit ep 9", "date": "2026-11-12", "steps": 3},
            {"action": "move_event", "id": 4, "date": "2026-11-13", "start": "14:00"}
        ]});
        assert_eq!(parse_proposal(&ok).unwrap().changes.len(), 2);
        for bad in [
            json!({"summary": "s", "changes": []}),
            json!({"summary": "s", "changes": [{"action": "create_task"}]}),
            json!({"summary": "s", "changes": [{"action": "move_event", "id": 1, "date": "13/11/2026"}]}),
            json!({"summary": "s", "changes": [{"action": "launch_rocket"}]}),
            json!({"changes": [{"action": "delete_task", "id": 1}]}),
        ] {
            assert!(parse_proposal(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn apply_each_action() {
        let mut db = Db::open_in_memory().unwrap();
        crate::seed::ensure(&mut db, day()).unwrap();
        let ch = |v: Value| -> Change { serde_json::from_value(v).unwrap() };

        let msg = apply(&mut db, &ch(json!({"action": "create_task", "title": "Rough cut", "steps": 3, "due": "2026-11-14"})), day()).unwrap();
        let id: i64 = msg.split('#').nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        let t = tasks::get(&db, id).unwrap();
        assert_eq!((t.steps_total, t.scheduled_for, t.due_date), (3, day(), NaiveDate::from_ymd_opt(2026, 11, 14)));

        apply(&mut db, &ch(json!({"action": "update_task", "id": id, "date": "2026-11-12", "due": "none"})), day()).unwrap();
        let t = tasks::get(&db, id).unwrap();
        assert_eq!((t.scheduled_for.to_string(), t.due_date), ("2026-11-12".to_string(), None));
        apply(&mut db, &ch(json!({"action": "complete_task", "id": id})), day()).unwrap();
        assert!(tasks::get(&db, id).unwrap().done);

        let msg = apply(&mut db, &ch(json!({"action": "create_event", "title": "Collab", "date": "2026-11-13", "start": "19:00"})), day()).unwrap();
        let eid: i64 = msg.split('#').nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        assert_eq!(events::ts(events::get(&db, eid).unwrap().ends_at.unwrap()), "2026-11-13T20:00:00");
        apply(&mut db, &ch(json!({"action": "move_event", "id": eid, "date": "2026-11-14", "start": "18:00"})), day()).unwrap();
        let e = events::get(&db, eid).unwrap();
        assert_eq!((events::ts(e.starts_at), events::ts(e.ends_at.unwrap())), ("2026-11-14T18:00:00".into(), "2026-11-14T19:00:00".into()));

        let p = projects::list(&db)[0].id;
        apply(&mut db, &ch(json!({"action": "update_project", "id": p, "status": "paused", "next_action": "Wait"})), day()).unwrap();
        assert_eq!(projects::get(&db, p).unwrap().status, "paused");
        apply(&mut db, &ch(json!({"action": "create_note", "body": "# Plan\nstuff", "project_id": p})), day()).unwrap();
        apply(&mut db, &ch(json!({"action": "delete_event", "id": eid})), day()).unwrap();
        assert!(events::get(&db, eid).is_none());
        assert!(apply(&mut db, &ch(json!({"action": "delete_task", "id": 99999})), day()).is_err());
    }

    #[test]
    fn snapshot_lists_ids() {
        let mut db = Db::open_in_memory().unwrap();
        crate::seed::ensure(&mut db, day()).unwrap();
        let s = snapshot(&db, day().and_hms_opt(9, 30, 0).unwrap(), true);
        assert!(s.starts_with("Now: Wednesday 2026-11-11 9:30 AM"));
        assert!(s.contains("Tasks for today") && s.contains("Events, next 3 weeks") && s.contains("Projects:"));
        assert!(s.contains("#") && s.contains("Hardcore Survival S3"));
        let c: Change = serde_json::from_value(json!({"action": "create_task", "title": "X", "date": "2026-11-12", "steps": 3})).unwrap();
        assert_eq!(describe(&db, &c, true), "Add task “X” on Thu Nov 12, 3 steps");
    }
}
