//! Project detail panel: fields, progress, linked tasks and notes.

use super::{JUST_CREATED, SELECTED};
use crate::icons::{self, Icon};
use crate::state::{use_ui, View};
use crate::store::use_store;
use crate::widgets::tasks::{AddTask, TaskList};
use crate::widgets::{ago, notes as notes_widget, ProgressBar};
use cc_core::model::projects::{self, status_label, Project, COLORS, STATUSES};
use cc_core::model::{notes, tasks};
use dioxus::prelude::*;

#[component]
pub fn ProjectDetail(project: Project, autofocus: bool) -> Element {
    let store = use_store();
    let ui = use_ui();
    let p = project;
    let id = p.id;
    // Text fields keep local state so typing is never interrupted by re-renders.
    let mut name = use_signal(|| p.name.clone());
    let mut next = use_signal(|| p.next_action.clone());
    let mut desc = use_signal(|| p.description.clone());
    let (linked_tasks, linked_notes) = store.read(|db| (tasks::for_project(db, id), notes::for_project(db, id)));
    let pct = p.percent();
    let total = p.tasks_open + p.tasks_done;
    let progress_text = if total == 0 { "No linked tasks yet".to_string() } else { format!("{} of {total} tasks done", p.tasks_done) };

    let st = store.clone();
    let w = move |f: Box<dyn FnOnce(&cc_core::Db) -> anyhow::Result<()>>| {
        let _ = st.write(|db| f(db));
    };
    let (w1, w2, w3, w4, w5, w6, w7) = (w.clone(), w.clone(), w.clone(), w.clone(), w.clone(), w.clone(), w);
    let s_note = store.clone();
    let s_open = store.clone();
    let s_del = store.clone();
    let project_name = p.name.clone();

    rsx! {
        aside { class: "panel project-detail", style: "--pc: {p.color}",
            div { class: "pd-head",
                span { class: "dot big", style: "background: {p.color}" }
                input {
                    class: "pd-name", value: "{name}", spellcheck: "false",
                    onmounted: move |e| async move {
                        if autofocus {
                            *JUST_CREATED.write() = None;
                            let _ = e.set_focus(true).await;
                            document::eval("document.activeElement?.select?.()");
                        }
                    },
                    oninput: move |e| { name.set(e.value()); let v = e.value(); w1(Box::new(move |db| projects::set_name(db, id, &v))); },
                }
                button { class: "icon-btn", title: "Close", onclick: move |_| *SELECTED.write() = None, Icon { d: icons::CLOSE } }
            }

            div { class: "pd-field",
                div { class: "seg small wrap",
                    for s in STATUSES {
                        button {
                            class: if p.status == s { "on" },
                            onclick: { let w = w2.clone(); move |_| w(Box::new(move |db| projects::set_status(db, id, s))) },
                            "{status_label(s)}"
                        }
                    }
                }
            }

            label { class: "pd-field",
                span { class: "pd-label", "Next action" }
                input { class: "pd-input", value: "{next}", placeholder: "What's the very next step?",
                    oninput: move |e| { next.set(e.value()); let v = e.value(); w3(Box::new(move |db| projects::set_next_action(db, id, &v))); } }
            }
            label { class: "pd-field",
                span { class: "pd-label", "Description" }
                textarea { class: "pd-input", rows: 3, value: "{desc}",
                    oninput: move |e| { desc.set(e.value()); let v = e.value(); w4(Box::new(move |db| projects::set_description(db, id, &v))); } }
            }

            div { class: "pd-field",
                div { class: "pd-label-row",
                    span { class: "pd-label", "Progress" }
                    div { class: "seg small",
                        button { class: if !p.from_tasks { "on" }, onclick: { let w = w5.clone(); move |_| w(Box::new(move |db| projects::set_from_tasks(db, id, false))) }, "Manual" }
                        button { class: if p.from_tasks { "on" }, onclick: { let w = w5.clone(); move |_| w(Box::new(move |db| projects::set_from_tasks(db, id, true))) }, "From tasks" }
                    }
                }
                div { class: "pd-progress",
                    if p.from_tasks {
                        ProgressBar { pct, color: p.color.clone() }
                    } else {
                        // The slider is drawn as the bar itself (filled up to --p).
                        input { r#type: "range", class: "pd-range", min: 0, max: 100, step: 5, value: "{p.progress}",
                            style: "--p: {pct}%",
                            oninput: move |e| if let Ok(v) = e.value().parse::<i64>() { w6(Box::new(move |db| projects::set_progress(db, id, v))); } }
                    }
                    span { class: "pct", "{pct}%" }
                }
                if p.from_tasks {
                    span { class: "muted small", "{progress_text}" }
                }
            }

            div { class: "pd-field",
                span { class: "pd-label", "Color" }
                div { class: "swatches",
                    for c in COLORS {
                        button {
                            class: if p.color == c { "swatch on" } else { "swatch" },
                            style: "background: {c}",
                            title: "{c}",
                            onclick: { let w = w7.clone(); move |_| w(Box::new(move |db| projects::set_color(db, id, c))) },
                        }
                    }
                }
            }

            div { class: "pd-section",
                div { class: "pd-section-head",
                    span { "Tasks" }
                    span { class: "muted", "{p.tasks_open} open" }
                }
                AddTask { compact: true, project: Some(id) }
                TaskList { tasks: linked_tasks, compact: false, reorderable: false }
            }

            div { class: "pd-section",
                div { class: "pd-section-head",
                    span { "Notes" }
                    span { class: "muted", "{linked_notes.len()}" }
                    div { class: "grow" }
                    button { class: "icon-btn", title: "New linked note",
                        onclick: move |_| {
                            if let Ok(nid) = s_note.write(|db| notes::add(db, &format!("# {project_name}\n\n"), Some(id))) {
                                notes_widget::select(&s_note, Some(nid));
                                ui.open(View::Widget("notes"));
                            }
                        },
                        Icon { d: icons::PLUS }
                    }
                }
                if linked_notes.is_empty() { div { class: "muted small", "No linked notes" } }
                for n in linked_notes {
                    button {
                        key: "{n.id}", class: "pd-note",
                        onclick: { let s = s_open.clone(); move |_| { notes_widget::select(&s, Some(n.id)); ui.open(View::Widget("notes")); } },
                        Icon { d: icons::NOTES }
                        span { class: "grow", "{n.title()}" }
                        span { class: "muted small", "{ago(&n.updated_at)}" }
                    }
                }
            }

            div { class: "pd-footer",
                span { class: "muted small", "Updated {ago(&p.updated_at)}" }
                div { class: "grow" }
                button { class: "btn danger",
                    onclick: move |_| {
                        if let Ok(Some(d)) = s_del.write(|db| projects::delete(db, id)) {
                            *SELECTED.write() = None;
                            let label = d.project.name.clone();
                            ui.notify_undo(format!("Deleted “{label}” (tasks and notes kept)"), move |s| {
                                let _ = s.write(|db| projects::restore(db, &d));
                                *SELECTED.write() = Some(d.project.id);
                            });
                        }
                    },
                    "Delete project"
                }
            }
        }
    }
}
