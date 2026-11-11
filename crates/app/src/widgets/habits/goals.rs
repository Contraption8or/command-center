//! Goal cards with quick progress logging and an inline editor.

use super::{EDITING, JUST_CREATED};
use crate::icons::{self, Icon};
use crate::state::use_ui;
use crate::store::use_store;
use crate::widgets::ProgressBar;
use cc_core::model::goals::{self, fmt_num, Goal, GoalInput, Pace};
use cc_core::model::project_options;
use chrono::NaiveDate;
use dioxus::prelude::*;

pub fn pace_color(p: Pace) -> String {
    match p {
        Pace::Done | Pace::Ahead => "var(--good)",
        Pace::OnTrack | Pace::Open => "var(--accent)",
        Pace::Behind => "var(--warn)",
        Pace::Overdue => "var(--bad)",
    }
    .into()
}

fn pace_label(p: Pace) -> (&'static str, &'static str) {
    match p {
        Pace::Done => ("Done", "pill-active"),
        Pace::Ahead => ("Ahead", "pill-active"),
        Pace::OnTrack => ("On track", "pill-planned"),
        Pace::Behind => ("Behind", "pill-paused"),
        Pace::Overdue => ("Overdue", "pill-bad"),
        Pace::Open => ("No deadline", "pill-done"),
    }
}

#[component]
pub fn GoalCard(goal: Goal) -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let g = goal;
    let key = -g.id;
    let id = g.id;
    let editing = EDITING() == Some(key);
    let pace = g.pace(today);
    let (pace_text, pace_class) = pace_label(pace);
    let unit = if g.unit.is_empty() { String::new() } else { format!(" {}", g.unit) };
    let mut log = use_signal(String::new);

    let mut foot: Vec<String> = Vec::new();
    if let (Some(d), Some(left)) = (g.deadline, g.days_left(today)) {
        foot.push(format!("Due {}", d.format("%b %-d, %Y")));
        if left >= 0 && !g.is_done() {
            foot.push(if left == 0 { "today".into() } else { format!("{left} days left") });
        }
    }
    if let Some(w) = g.needed_per_week(today) {
        foot.push(format!("needs {}{unit}/week", fmt_num((w * 10.0).round() / 10.0)));
    }
    let foot = foot.join(" · ");

    let s_log = store.clone();
    rsx! {
        div { class: if editing { "goal-card editing" } else { "goal-card" },
            div { class: "gc-head",
                span { class: "gc-title", "{g.title}" }
                span { class: "pill {pace_class}", "{pace_text}" }
                button { class: if editing { "icon-btn on" } else { "icon-btn" }, title: "Edit",
                    onclick: move |_| { let next = if EDITING() == Some(key) { None } else { Some(key) }; *EDITING.write() = next; },
                    Icon { d: icons::SETTINGS }
                }
            }
            div { class: "gc-values",
                span { class: "gc-current", "{fmt_num(g.current)}" }
                span { class: "muted", " / {fmt_num(g.target)}{unit}" }
                div { class: "grow" }
                span { class: "pct", "{g.percent()}%" }
            }
            ProgressBar { pct: g.percent(), color: pace_color(pace) }
            div { class: "gc-foot",
                span { class: "muted small", "{foot}" }
                if let Some(p) = &g.project_name { span { class: "chip", "{p}" } }
            }
            if !g.is_done() || editing {
                div { class: "gc-log",
                    input {
                        class: "pd-input", r#type: "number", step: "any", placeholder: "Update value",
                        value: "{log}",
                        oninput: move |e| log.set(e.value()),
                        onkeydown: move |e| if e.key() == Key::Enter {
                            let parsed = log.peek().trim().parse::<f64>();
                            if let Ok(v) = parsed {
                                let _ = s_log.write(|db| goals::set_current(db, id, v));
                                log.set(String::new());
                            }
                        },
                    }
                }
            }
            if editing { GoalEditor { goal: g.clone() } }
        }
    }
}

#[component]
fn GoalEditor(goal: Goal) -> Element {
    let store = use_store();
    let ui = use_ui();
    let id = goal.id;
    let base = GoalInput::of(&goal);
    let mut title = use_signal(|| goal.title.clone());
    let mut unit = use_signal(|| goal.unit.clone());
    let mut error = use_signal(|| None::<String>);
    let autofocus = JUST_CREATED() == Some(-id);
    let projects = store.read(project_options);
    let proj_val = goal.project_id.map(|p| p.to_string()).unwrap_or_default();

    let save = {
        let store = store.clone();
        let base = base.clone();
        move |change: Box<dyn FnOnce(&mut GoalInput)>| {
            let mut input = base.clone();
            input.title = title.peek().clone();
            input.unit = unit.peek().clone();
            change(&mut input);
            match store.write(|db| goals::update(db, id, &input)) {
                Ok(()) => error.set(None),
                Err(e) => error.set(Some(e.to_string())),
            }
        }
    };
    let (mut s1, mut s2, mut s3, mut s4, mut s5, mut s6, mut s7) =
        (save.clone(), save.clone(), save.clone(), save.clone(), save.clone(), save.clone(), save);
    let num = |v: String| v.trim().parse::<f64>().ok();
    let s_del = store.clone();
    let deadline_v = goal.deadline.map(|d| d.to_string()).unwrap_or_default();

    rsx! {
        div { class: "gc-editor",
            label { class: "pd-field",
                span { class: "pd-label", "Title" }
                input { class: "pd-input", value: "{title}", spellcheck: "false",
                    onmounted: move |e| async move {
                        if autofocus {
                            *JUST_CREATED.write() = None;
                            let _ = e.set_focus(true).await;
                            document::eval("document.activeElement?.select?.()");
                        }
                    },
                    oninput: move |e| { title.set(e.value()); if !e.value().trim().is_empty() { s1(Box::new(|_| {})); } } }
            }
            div { class: "gc-grid",
                label { class: "pd-field",
                    span { class: "pd-label", "Start" }
                    input { class: "pd-input", r#type: "number", step: "any", value: "{goal.start}",
                        onchange: move |e| if let Some(v) = num(e.value()) { s2(Box::new(move |i| i.start = v)); } }
                }
                label { class: "pd-field",
                    span { class: "pd-label", "Current" }
                    input { class: "pd-input", r#type: "number", step: "any", value: "{goal.current}",
                        onchange: move |e| if let Some(v) = num(e.value()) { s3(Box::new(move |i| i.current = v)); } }
                }
                label { class: "pd-field",
                    span { class: "pd-label", "Target" }
                    input { class: "pd-input", r#type: "number", step: "any", value: "{goal.target}",
                        onchange: move |e| if let Some(v) = num(e.value()) { s4(Box::new(move |i| i.target = v)); } }
                }
                label { class: "pd-field",
                    span { class: "pd-label", "Unit" }
                    input { class: "pd-input", value: "{unit}", placeholder: "subs, videos, $…",
                        oninput: move |e| { unit.set(e.value()); s5(Box::new(|_| {})); } }
                }
            }
            div { class: "gc-grid",
                label { class: "pd-field",
                    span { class: "pd-label", "Deadline" }
                    input { class: "pd-input", r#type: "date", value: "{deadline_v}",
                        onchange: move |e| { let d = NaiveDate::parse_from_str(&e.value(), "%Y-%m-%d").ok(); s6(Box::new(move |i| i.deadline = d)); } }
                }
                label { class: "pd-field",
                    span { class: "pd-label", "Project" }
                    select { class: "pd-input",
                        onchange: move |e| { let p = e.value().parse().ok(); s7(Box::new(move |i| i.project_id = p)); },
                        option { value: "", selected: proj_val.is_empty(), "None" }
                        for (pid, name, _) in projects {
                            option { value: "{pid}", selected: proj_val == pid.to_string(), "{name}" }
                        }
                    }
                }
            }
            if let Some(err) = error() { div { class: "chip warn", "{err}" } }
            div { class: "hb-editor-row",
                button { class: "btn", onclick: move |_| *EDITING.write() = None, "Done" }
                div { class: "grow" }
                button { class: "btn danger",
                    onclick: move |_| {
                        if let Ok(Some((g, created))) = s_del.write(|db| goals::delete(db, id)) {
                            *EDITING.write() = None;
                            let label = g.title.clone();
                            ui.notify_undo(format!("Deleted “{label}”"), move |s| { let _ = s.write(|db| goals::restore(db, &g, &created)); });
                        }
                    },
                    "Delete"
                }
            }
        }
    }
}
