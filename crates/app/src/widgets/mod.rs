//! Widget registry. To add a widget: create a module that exports a `DEF: WidgetDef`,
//! declare it below, and add it to `REGISTRY`. Order here = order on the overview grid
//! and in the side rail (Ctrl+1…Ctrl+9).

use crate::state::{Command, Ui};
use crate::store::Store;
use dioxus::prelude::*;

pub mod assistant;
pub mod events;
pub mod habits;
pub mod notes;
pub mod projects;
pub mod spending;
pub mod tasks;

pub struct WidgetDef {
    /// Stable id, used in settings and saved layout. Never rename once shipped.
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    /// Width on the 12-column overview grid.
    pub cols: u8,
    /// Height in grid rows (each ~120px minimum).
    pub rows: u8,
    /// Compact body shown on the overview card (the shell draws the card frame and header).
    pub card: fn() -> Element,
    /// Full-screen view.
    pub full: fn() -> Element,
    /// Entries this widget contributes to the command palette.
    pub commands: fn() -> Vec<Command>,
    /// Optional: items matching a palette query (e.g. tasks or notes by title).
    pub search: Option<fn(&cc_core::Db, &str) -> Vec<Hit>>,
    /// Optional: called before the full view opens on a chosen search hit (select it, expand it…).
    pub open_item: Option<fn(i64)>,
    /// Optional: handles messages sent from the page as `"<id>:<payload>"` (see shell/keys.rs);
    /// receives the payload. Used for clicks inside rendered HTML, such as note checkboxes.
    pub on_message: Option<fn(&Store, Ui, &str)>,
}

/// One palette search result.
pub struct Hit {
    pub id: i64,
    pub label: String,
    pub detail: String,
}

pub static REGISTRY: &[&WidgetDef] = &[
    &tasks::DEF,
    &events::DEF,
    &habits::DEF,
    &projects::DEF,
    &spending::DEF,
    &notes::DEF,
    &assistant::DEF,
];

pub fn get(id: &str) -> Option<&'static WidgetDef> {
    REGISTRY.iter().copied().find(|w| w.id == id)
}

/// Compact relative time for a stored `YYYY-MM-DDTHH:MM:SS` timestamp: "just now", "5m", "3h",
/// "Yesterday", "Mon", "Nov 3".
pub fn ago(ts: &str) -> String {
    use chrono::{Datelike, Local, NaiveDateTime};
    let Ok(t) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S") else { return String::new() };
    let now = Local::now().naive_local();
    let mins = (now - t).num_minutes();
    let days = (now.date() - t.date()).num_days();
    match () {
        _ if mins < 1 => "just now".into(),
        _ if mins < 60 && days == 0 => format!("{mins}m"),
        _ if days == 0 => format!("{}h", mins / 60),
        _ if days == 1 => "Yesterday".into(),
        _ if days < 7 => t.format("%a").to_string(),
        _ if t.year() == now.year() => t.format("%b %-d").to_string(),
        _ => t.format("%b %-d, %Y").to_string(),
    }
}

/// Format integer cents as money with the configured symbol.
pub fn money(cents: i64, symbol: &str) -> String {
    let neg = cents < 0;
    let whole = (cents.abs() + 50) / 100;
    let s = whole.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{}{symbol}{out}", if neg { "-" } else { "" })
}

/// Phase-1 full view: shows the card body larger until the module's real view is built.
#[component]
pub fn PendingFull(id: &'static str, phase: u8) -> Element {
    let def = get(id).expect("registered widget");
    rsx! {
        div { class: "full",
            div { class: "panel",
                {(def.card)()}
            }
            div { class: "panel muted", "Full view: phase {phase}" }
        }
    }
}

/// Thin progress bar. `pct` is clamped to 0–100; `color` defaults to the accent.
#[component]
pub fn ProgressBar(pct: i64, color: Option<String>) -> Element {
    let w = pct.clamp(0, 100);
    let c = color.unwrap_or_else(|| "var(--accent)".into());
    rsx! {
        div { class: "bar", div { class: "bar-fill", style: "width: {w}%; background: {c};" } }
    }
}

#[component]
pub fn Stat(value: String, label: String, class: Option<String>) -> Element {
    let cls = class.unwrap_or_default();
    rsx! {
        div { class: "stat",
            div { class: "v {cls}", "{value}" }
            div { class: "l", "{label}" }
        }
    }
}
