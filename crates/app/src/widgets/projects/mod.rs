//! Projects: overview card, status board and detail panel with linked tasks and notes.

mod detail;

use super::{Hit, ProgressBar, WidgetDef};
use crate::icons::{self, Icon};
use crate::state::{use_ui, Action, Command, Ui, View};
use crate::store::{use_store, Store};
use cc_core::model::projects::{self, status_label, Project, STATUSES};
use dioxus::prelude::*;

pub static DEF: WidgetDef = WidgetDef {
    id: "projects",
    title: "Projects",
    icon: icons::PROJECTS,
    cols: 8,
    rows: 2,
    card,
    full,
    commands,
    search: Some(search),
    open_item: Some(open_item),
    on_message: None,
};

/// Project open in the detail panel.
pub static SELECTED: GlobalSignal<Option<i64>> = Signal::global(|| None);
/// Set when a project was just created, so its name field takes focus.
static JUST_CREATED: GlobalSignal<Option<i64>> = Signal::global(|| None);
/// Board filter: `None` = all statuses.
static FILTER: GlobalSignal<Option<&'static str>> = Signal::global(|| None);

pub fn new_project(store: &Store, ui: Ui) {
    let status = FILTER().filter(|s| *s != "done").unwrap_or("planned");
    if let Ok(id) = store.write(|db| projects::add(db, "New project", status)) {
        *SELECTED.write() = Some(id);
        *JUST_CREATED.write() = Some(id);
        ui.open(View::Widget("projects"));
    }
}

fn card() -> Element {
    let store = use_store();
    let ui = use_ui();
    let list: Vec<Project> = store
        .read(projects::list)
        .into_iter()
        .filter(|p| matches!(p.status.as_str(), "active" | "planned" | "paused"))
        .collect();
    rsx! {
        div { class: "proj-rows",
            if list.is_empty() { div { class: "muted empty-line", "No active projects" } }
            for p in list {
                button {
                    key: "{p.id}",
                    class: "proj-row",
                    onclick: move |_| { *SELECTED.write() = Some(p.id); ui.open(View::Widget("projects")); },
                    span { class: "dot", style: "background: {p.color}" }
                    div { class: "proj-row-main",
                        div { class: "proj-row-name",
                            span { "{p.name}" }
                            span { class: "pill pill-{p.status}", "{status_label(&p.status)}" }
                        }
                        if !p.next_action.is_empty() {
                            div { class: "proj-row-next", Icon { d: icons::ARROW_RIGHT } "{p.next_action}" }
                        }
                    }
                    div { class: "proj-row-progress",
                        ProgressBar { pct: p.percent(), color: p.color.clone() }
                        span { class: "pct", "{p.percent()}%" }
                    }
                }
            }
        }
    }
}

fn full() -> Element {
    let store = use_store();
    let ui = use_ui();
    let all = store.read(projects::list);
    let filter = FILTER();
    let mut show_done = use_signal(|| false);
    let selected = SELECTED().and_then(|id| all.iter().find(|p| p.id == id).cloned());
    let s_new = store.clone();
    let has_detail = selected.is_some();

    let count = |s: &str| all.iter().filter(|p| p.status == s).count();
    let sections: Vec<&'static str> = match filter {
        Some(s) => vec![s],
        None => STATUSES.to_vec(),
    };

    rsx! {
        div { class: if has_detail { "projects-full with-detail" } else { "projects-full" },
            div { class: "projects-main",
                div { class: "projects-toolbar",
                    div { class: "seg small",
                        button { class: if filter.is_none() { "on" }, onclick: move |_| *FILTER.write() = None, "All {all.len()}" }
                        for s in STATUSES {
                            button {
                                class: if filter == Some(s) { "on" },
                                onclick: move |_| *FILTER.write() = Some(s),
                                "{status_label(s)} {count(s)}"
                            }
                        }
                    }
                    div { class: "grow" }
                    button { class: "btn primary", onclick: move |_| new_project(&s_new, ui), Icon { d: icons::PLUS } " New project" }
                }
                for s in sections {
                    {
                        let items: Vec<Project> = all.iter().filter(|p| p.status == s).cloned().collect();
                        let collapsed = s == "done" && filter.is_none() && !show_done();
                        let n_items = items.len();
                        rsx! {
                            if n_items > 0 || filter.is_some() {
                                section { key: "{s}", class: "proj-section",
                                    div { class: "proj-section-head",
                                        span { class: "pill pill-{s}", "{status_label(s)}" }
                                        span { class: "muted", "{n_items}" }
                                        if s == "done" && filter.is_none() {
                                            button { class: "done-toggle", onclick: move |_| show_done.toggle(),
                                                if show_done() { "Hide" } else { "Show" }
                                            }
                                        }
                                    }
                                    if !collapsed {
                                        div { class: "project-grid",
                                            for p in items { ProjectCard { key: "{p.id}", project: p } }
                                        }
                                    }
                                    if n_items == 0 && filter.is_some() {
                                        div { class: "muted empty-line", "No {status_label(s).to_lowercase()} projects" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(p) = selected {
                detail::ProjectDetail { key: "{p.id}", project: p, autofocus: JUST_CREATED() == SELECTED() }
            }
        }
    }
}

#[component]
fn ProjectCard(project: Project) -> Element {
    let p = project;
    let id = p.id;
    let sel = SELECTED() == Some(id);
    let pct = p.percent();
    let tasks_total = p.tasks_open + p.tasks_done;
    rsx! {
        div {
            class: if sel { "project-card sel" } else { "project-card" },
            style: "--pc: {p.color}",
            tabindex: 0,
            onclick: move |_| { let next = if SELECTED() == Some(id) { None } else { Some(id) }; *SELECTED.write() = next; },
            onkeydown: move |e| if e.key() == Key::Enter { *SELECTED.write() = Some(id); },
            div { class: "pc-head",
                span { class: "dot", style: "background: {p.color}" }
                span { class: "pc-name", "{p.name}" }
            }
            if !p.description.is_empty() { p { class: "pc-desc", "{p.description}" } }
            if !p.next_action.is_empty() {
                div { class: "pc-next", Icon { d: icons::ARROW_RIGHT } span { "{p.next_action}" } }
            }
            div { class: "pc-progress",
                ProgressBar { pct, color: p.color.clone() }
                span { class: "pct", "{pct}%" }
            }
            div { class: "pc-foot",
                if tasks_total > 0 { span { Icon { d: icons::TASKS } "{p.tasks_done}/{tasks_total}" } }
                if p.notes > 0 { span { Icon { d: icons::NOTES } "{p.notes}" } }
                if p.from_tasks { span { class: "muted", "auto" } }
            }
        }
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::PROJECTS, "Projects", Action::Open(View::Widget("projects"))).shortcut("Ctrl+4"),
        Command::new("Create", icons::PLUS, "New project", Action::Run(new_project)),
    ]
}

fn search(db: &cc_core::Db, q: &str) -> Vec<Hit> {
    projects::search(db, q)
        .into_iter()
        .map(|p| Hit { id: p.id, detail: status_label(&p.status).to_string(), label: p.name })
        .collect()
}

fn open_item(id: i64) {
    *SELECTED.write() = Some(id);
    *FILTER.write() = None;
}
