//! Habit rows (card), habit blocks with heatmap and inline editor (full view).

use super::{EDITING, JUST_CREATED};
use crate::icons::{self, Icon};
use crate::state::use_ui;
use crate::store::use_store;
use cc_core::model::habits::{self, week_start, Habit};
use cc_core::model::projects::COLORS;
use chrono::{Duration, NaiveDate};
use dioxus::prelude::*;

/// Weeks of history in the full-view heatmap.
const HEAT_WEEKS: i64 = 26;

pub fn streak_text(h: &Habit, n: i64) -> String {
    match (h.is_weekly(), n) {
        (_, 0) => "No streak".into(),
        (false, 1) => "1 day".into(),
        (false, n) => format!("{n} days"),
        (true, 1) => "1 week".into(),
        (true, n) => format!("{n} weeks"),
    }
}

fn cadence_text(h: &Habit) -> String {
    if h.is_weekly() { format!("{}× a week", h.target) } else { "Daily".into() }
}

/// Round check button for today. Weekly habits show this week's count.
#[component]
fn TodayCheck(habit: Habit) -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let id = habit.id;
    let done = habit.done_on(today);
    let s = habit.stats(today);
    let met = habit.is_weekly() && s.this_period >= habit.target;
    let label = if habit.is_weekly() { format!("{}/{}", s.this_period, habit.target) } else { String::new() };
    rsx! {
        button {
            class: if done { "hcheck on" } else if met { "hcheck met" } else { "hcheck" },
            style: "--hc: {habit.color}",
            title: if done { "Undo today's check-in" } else { "Check in today" },
            onclick: move |e| { e.stop_propagation(); let _ = store.write(|db| habits::toggle(db, id, today).map(|_| ())); },
            if habit.is_weekly() { span { "{label}" } } else { svg { view_box: "0 0 24 24", path { d: "M6 12.5l4 4 8-9" } } }
        }
    }
}

/// Compact card row: check, name, last 7 days, streak.
#[component]
pub fn HabitMini(habit: Habit) -> Element {
    let today = *use_ui().today.read();
    let s = habit.stats(today);
    let days: Vec<(NaiveDate, bool)> = (0..7).rev().map(|i| today - Duration::days(i)).map(|d| (d, habit.done_on(d))).collect();
    let unit = if habit.is_weekly() { "w" } else { "d" };
    rsx! {
        div { class: "hmini",
            TodayCheck { habit: habit.clone() }
            span { class: "hmini-name", "{habit.name}" }
            div { class: "week-dots",
                for (d, on) in days {
                    span {
                        class: if on { "wd on" } else { "wd" },
                        style: "--hc: {habit.color}",
                        title: "{d.format(\"%a %b %-d\")}",
                    }
                }
            }
            span { class: if s.current > 0 { "streak" } else { "streak zero" }, "{s.current}{unit}" }
        }
    }
}

/// Full-view block: header with stats, heatmap, and an inline editor when editing.
#[component]
pub fn HabitBlock(habit: Habit) -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let id = habit.id;
    let s = habit.stats(today);
    let editing = EDITING() == Some(id);

    // Heatmap: HEAT_WEEKS columns (Mon–Sun), ending with the current week.
    let first = week_start(today) - Duration::days(7 * (HEAT_WEEKS - 1));
    let weeks: Vec<Vec<NaiveDate>> =
        (0..HEAT_WEEKS).map(|w| (0..7).map(|d| first + Duration::days(w * 7 + d)).collect()).collect();

    rsx! {
        div { class: if habit.archived { "habit-block archived" } else { "habit-block" }, style: "--hc: {habit.color}",
            div { class: "hb-head",
                if !habit.archived { TodayCheck { habit: habit.clone() } }
                div { class: "hb-title",
                    span { class: "hb-name", "{habit.name}" }
                    span { class: "muted small", "{cadence_text(&habit)}" }
                }
                div { class: "hb-stats",
                    div { class: "hb-stat", span { class: "v", "{streak_text(&habit, s.current)}" } span { class: "l", "current" } }
                    div { class: "hb-stat", span { class: "v", "{streak_text(&habit, s.best)}" } span { class: "l", "best" } }
                    div { class: "hb-stat", span { class: "v", "{s.rate}%" } span { class: "l", if habit.is_weekly() { "last 4 wk" } else { "last 30 d" } } }
                }
                button { class: if editing { "icon-btn on" } else { "icon-btn" }, title: "Edit",
                    onclick: move |_| { let next = if EDITING() == Some(id) { None } else { Some(id) }; *EDITING.write() = next; },
                    Icon { d: icons::SETTINGS }
                }
            }
            div { class: "heatmap",
                for week in weeks {
                    div { class: "hm-col",
                        for d in week {
                            {
                                let future = d > today;
                                let on = habit.done_on(d);
                                let st = store.clone();
                                rsx! {
                                    button {
                                        key: "{d}",
                                        class: if future { "hm-cell future" } else if on { "hm-cell on" } else if d == today { "hm-cell today" } else { "hm-cell" },
                                        disabled: future || habit.archived,
                                        title: "{d.format(\"%a %b %-d\")}",
                                        onclick: move |_| { let _ = st.write(|db| habits::toggle(db, id, d).map(|_| ())); },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if editing { HabitEditor { habit: habit.clone() } }
        }
    }
}

#[component]
fn HabitEditor(habit: Habit) -> Element {
    let store = use_store();
    let ui = use_ui();
    let id = habit.id;
    let mut name = use_signal(|| habit.name.clone());
    let cadence = habit.cadence.clone();
    let target = habit.target;
    let color = habit.color.clone();
    let autofocus = JUST_CREATED() == Some(id);

    let save = {
        let store = store.clone();
        move |n: String, c: String, t: i64, col: String| {
            if !n.trim().is_empty() {
                let _ = store.write(|db| habits::update(db, id, &n, &c, t, &col));
            }
        }
    };
    let (sv1, sv2, sv3, sv4) = (save.clone(), save.clone(), save.clone(), save);
    let (c1, c2, c3) = (cadence.clone(), cadence.clone(), cadence.clone());
    let (col1, col2) = (color.clone(), color.clone());
    let (s_arch, s_del) = (store.clone(), store.clone());
    let archived = habit.archived;

    rsx! {
        div { class: "hb-editor",
            input {
                class: "pd-input", value: "{name}", spellcheck: "false", placeholder: "Habit name",
                onmounted: move |e| async move {
                    if autofocus {
                        *JUST_CREATED.write() = None;
                        let _ = e.set_focus(true).await;
                        document::eval("document.activeElement?.select?.()");
                    }
                },
                oninput: move |e| { name.set(e.value()); sv1(e.value(), c1.clone(), target, col1.clone()); },
            }
            div { class: "hb-editor-row",
                div { class: "seg small",
                    button { class: if cadence == "daily" { "on" }, onclick: { let col = col2.clone(); let sv = sv2.clone(); move |_| sv(name.peek().clone(), "daily".into(), 1, col.clone()) }, "Daily" }
                    button { class: if cadence == "weekly" { "on" }, onclick: { let col = col2.clone(); let sv = sv2.clone(); move |_| sv(name.peek().clone(), "weekly".into(), target.max(2), col.clone()) }, "Weekly" }
                }
                if cadence == "weekly" {
                    label { class: "hb-target",
                        input { class: "pd-input", r#type: "number", min: 1, max: 7, value: "{target}",
                            onchange: { let col = color.clone(); move |e| if let Ok(t) = e.value().parse::<i64>() { sv3(name.peek().clone(), c2.clone(), t, col.clone()); } } }
                        span { class: "muted small", "times a week" }
                    }
                }
                div { class: "swatches",
                    for c in COLORS {
                        button {
                            class: if color == c { "swatch on" } else { "swatch" },
                            style: "background: {c}",
                            onclick: { let sv = sv4.clone(); let cad = c3.clone(); move |_| sv(name.peek().clone(), cad.clone(), target, c.to_string()) },
                        }
                    }
                }
            }
            div { class: "hb-editor-row",
                button { class: "btn", onclick: move |_| *EDITING.write() = None, "Done" }
                div { class: "grow" }
                button { class: "btn",
                    onclick: move |_| { let _ = s_arch.write(|db| habits::set_archived(db, id, !archived)); *EDITING.write() = None; },
                    if archived { "Unarchive" } else { "Archive" }
                }
                button { class: "btn danger",
                    onclick: move |_| {
                        if let Ok(Some(h)) = s_del.write(|db| habits::delete(db, id)) {
                            *EDITING.write() = None;
                            let label = h.name.clone();
                            ui.notify_undo(format!("Deleted “{label}” and its history"), move |s| { let _ = s.write(|db| habits::restore(db, &h)); });
                        }
                    },
                    "Delete"
                }
            }
        }
    }
}
