//! Today's tasks: overview card and full view (today + upcoming days).

mod list;

pub use list::{day_label, AddTask, TaskList};

use super::{Hit, WidgetDef};
use crate::icons;
use crate::state::{use_ui, Action, Command, View};
use crate::store::use_store;
use cc_core::model::tasks::{self, Task};
use dioxus::prelude::*;


pub static DEF: WidgetDef = WidgetDef {
    id: "tasks",
    title: "Tasks",
    icon: icons::TASKS,
    cols: 4,
    rows: 2,
    card,
    full,
    commands,
    search: Some(search),
    open_item: Some(open_item),
    on_message: None,
};

fn card() -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let list = store.read(|db| tasks::for_day(db, today));
    rsx! {
        div { class: "card-scroll",
            AddTask { compact: true }
            TaskList { tasks: list, compact: true }
        }
    }
}

fn full() -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let (today_list, upcoming) = store.read(|db| (tasks::for_day(db, today), tasks::upcoming(db, today)));
    let open = today_list.iter().filter(|t| !t.done).count();
    let done = today_list.len() - open;

    // Group upcoming by day, preserving order.
    let mut groups: Vec<(chrono::NaiveDate, Vec<Task>)> = Vec::new();
    for t in upcoming {
        match groups.last_mut() {
            Some((d, v)) if *d == t.scheduled_for => v.push(t),
            _ => groups.push((t.scheduled_for, vec![t])),
        }
    }

    rsx! {
        div { class: "tasks-full",
            section { class: "panel",
                div { class: "panel-head",
                    h3 { "Today" }
                    span { class: "muted", "{open} open · {done} done" }
                }
                AddTask { compact: false }
                TaskList { tasks: today_list, compact: false }
            }
            section { class: "panel",
                div { class: "panel-head", h3 { "Upcoming" } }
                if groups.is_empty() {
                    div { class: "muted empty-line", "Nothing scheduled" }
                }
                for (day, list) in groups {
                    div { key: "{day}", class: "day-group",
                        div { class: "day-head",
                            span { "{day_label(day, today)}" }
                            span { class: "muted", "{day.format(\"%b %-d\")}" }
                        }
                        TaskList { tasks: list, compact: false }
                    }
                }
            }
        }
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::TASKS, "Today's tasks", Action::Open(View::Widget("tasks"))).keywords("todo list"),
        Command::new("Create", icons::PLUS, "New task", Action::QuickAdd("")).keywords("todo add"),
    ]
}

fn search(db: &cc_core::Db, q: &str) -> Vec<Hit> {
    let today = cc_core::time::today();
    tasks::search(db, q)
        .into_iter()
        .map(|t| Hit {
            id: t.id,
            detail: t.project_name.clone().map_or_else(|| day_label(t.scheduled_for, today), |p| format!("{} · {p}", day_label(t.scheduled_for, today))),
            label: t.title,
        })
        .collect()
}

fn open_item(id: i64) {
    *list::EXPANDED.write() = Some(id);
}
