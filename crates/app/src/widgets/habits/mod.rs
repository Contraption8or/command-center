//! Habits & goals: check-ins with streaks and heatmaps; goals with pace tracking.

mod goals;
mod habit;

use super::{Hit, ProgressBar, WidgetDef};
use crate::icons::{self, Icon};
use crate::state::{use_ui, Action, Command, Ui, View};
use crate::store::{use_store, Store};
use cc_core::model::goals as goal_model;
use cc_core::model::habits;
use cc_core::model::projects::COLORS;
use chrono::Duration;
use dioxus::prelude::*;

pub static DEF: WidgetDef = WidgetDef {
    id: "habits",
    title: "Habits & goals",
    icon: icons::HABITS,
    cols: 4,
    rows: 2,
    card,
    full,
    commands,
    search: Some(search),
    open_item: Some(open_item),
    on_message: None,
};

/// Habit or goal whose editor is open. Goals use negative ids to share one slot.
pub static EDITING: GlobalSignal<Option<i64>> = Signal::global(|| None);
/// Item just created: its name field takes focus once.
pub static JUST_CREATED: GlobalSignal<Option<i64>> = Signal::global(|| None);

pub fn new_habit(store: &Store, ui: Ui) {
    let n = store.peek(|db| habits::list(db, true).len());
    if let Ok(id) = store.write(|db| habits::add(db, "New habit", "daily", 1, COLORS[(n + 1) % COLORS.len()])) {
        *EDITING.write() = Some(id);
        *JUST_CREATED.write() = Some(id);
        ui.open(View::Widget("habits"));
    }
}

pub fn new_goal(store: &Store, ui: Ui) {
    let today = *ui.today.peek();
    let input = goal_model::GoalInput {
        title: "New goal".into(),
        unit: String::new(),
        start: 0.0,
        current: 0.0,
        target: 100.0,
        deadline: Some(today + Duration::days(90)),
        project_id: None,
    };
    if let Ok(id) = store.write(|db| goal_model::add(db, &input)) {
        *EDITING.write() = Some(-id);
        *JUST_CREATED.write() = Some(-id);
        ui.open(View::Widget("habits"));
    }
}

fn card() -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    let (hs, gs) = store.read(|db| (habits::list(db, false), goal_model::list(db)));
    let open_goals: Vec<_> = gs.into_iter().filter(|g| !g.is_done()).take(3).collect();
    rsx! {
        div { class: "hg-card",
            div { class: "hg-habits",
                if hs.is_empty() { div { class: "muted empty-line", "No habits yet" } }
                for h in hs { habit::HabitMini { key: "{h.id}", habit: h } }
            }
            if !open_goals.is_empty() {
                div { class: "hg-goals",
                    for g in open_goals {
                        button {
                            key: "{g.id}",
                            class: "hg-goal",
                            onclick: move |_| { *EDITING.write() = None; ui.open(View::Widget("habits")); },
                            div { class: "hg-goal-top",
                                span { class: "hg-goal-title", "{g.title}" }
                                span { class: "pct", "{g.percent()}%" }
                            }
                            ProgressBar { pct: g.percent(), color: goals::pace_color(g.pace(today)) }
                        }
                    }
                }
            }
        }
    }
}

fn full() -> Element {
    let store = use_store();
    let ui = use_ui();
    let (all, gs) = store.read(|db| (habits::list(db, true), goal_model::list(db)));
    let (active, archived): (Vec<_>, Vec<_>) = all.into_iter().partition(|h| !h.archived);
    let n_archived = archived.len();
    let mut show_archived = use_signal(|| false);
    let (s_h, s_g) = (store.clone(), store.clone());

    rsx! {
        div { class: "hg-full",
            section { class: "panel",
                div { class: "panel-head",
                    h3 { "Habits" }
                    span { class: "muted", "{active.len()}" }
                    div { class: "grow" }
                    button { class: "btn", onclick: move |_| new_habit(&s_h, ui), Icon { d: icons::PLUS } " New habit" }
                }
                if active.is_empty() { div { class: "muted empty-line", "No habits yet" } }
                for h in active { habit::HabitBlock { key: "{h.id}", habit: h } }
                if n_archived > 0 {
                    button { class: "done-toggle", onclick: move |_| show_archived.toggle(),
                        if show_archived() { "Hide archived ({n_archived})" } else { "Show archived ({n_archived})" }
                    }
                    if show_archived() {
                        for h in archived { habit::HabitBlock { key: "{h.id}", habit: h } }
                    }
                }
            }
            section { class: "panel",
                div { class: "panel-head",
                    h3 { "Goals" }
                    span { class: "muted", "{gs.len()}" }
                    div { class: "grow" }
                    button { class: "btn", onclick: move |_| new_goal(&s_g, ui), Icon { d: icons::PLUS } " New goal" }
                }
                if gs.is_empty() { div { class: "muted empty-line", "No goals yet" } }
                div { class: "goal-list",
                    for g in gs { goals::GoalCard { key: "{g.id}", goal: g } }
                }
            }
        }
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::HABITS, "Habits & goals", Action::Open(View::Widget("habits"))).keywords("streak goal").shortcut("Ctrl+3"),
        Command::new("Create", icons::PLUS, "New habit", Action::Run(new_habit)).keywords("routine streak"),
        Command::new("Create", icons::PLUS, "New goal", Action::Run(new_goal)).keywords("target"),
    ]
}

fn search(db: &cc_core::Db, q: &str) -> Vec<Hit> {
    let today = cc_core::time::today();
    let mut out: Vec<Hit> = habits::search(db, q)
        .into_iter()
        .map(|h| {
            let s = h.stats(today);
            Hit { id: h.id, detail: habit::streak_text(&h, s.current), label: h.name }
        })
        .collect();
    out.extend(goal_model::search(db, q).into_iter().map(|g| Hit { id: -g.id, detail: format!("Goal · {}%", g.percent()), label: g.title }));
    out
}

fn open_item(id: i64) {
    *EDITING.write() = Some(id);
}
