//! Note list and editor, shared by the overview card and the full view.

use super::{select, Mode, MODE, SELECTED};
use crate::icons::{self, Icon};
use crate::state::use_ui;
use crate::store::use_store;
use crate::widgets::ago;
use cc_core::markdown;
use cc_core::model::{notes::{self, Note}, project_options};
use dioxus::prelude::*;

#[component]
pub fn NoteList(notes: Vec<Note>, selected: Option<i64>, compact: bool) -> Element {
    let store = use_store();
    let pinned: Vec<Note> = notes.iter().filter(|n| n.pinned).cloned().collect();
    let rest: Vec<Note> = notes.iter().filter(|n| !n.pinned).cloned().collect();
    let show_heads = !compact && !pinned.is_empty();
    let item = move |n: Note| {
        let store = store.clone();
        let id = n.id;
        let sel = selected == Some(id);
        let title = n.title();
        let snippet = n.snippet(90);
        let when = ago(&n.updated_at);
        rsx! {
            button {
                key: "{id}",
                class: if sel { "note-item sel" } else { "note-item" },
                "data-row": "",
                onclick: move |_| select(&store, Some(id)),
                div { class: "note-item-top",
                    if n.pinned { span { class: "pin-mark", Icon { d: icons::PIN } } }
                    span { class: "note-item-title", "{title}" }
                    span { class: "note-item-when", "{when}" }
                }
                if !compact {
                    div { class: "note-item-snippet",
                        if let Some(color) = &n.project_color { span { class: "dot", style: "background: {color}" } }
                        "{snippet}"
                    }
                }
            }
        }
    };
    let item2 = item.clone();
    rsx! {
        div { class: "note-list", "data-list": "",
            if show_heads { div { class: "list-head", "Pinned" } }
            for n in pinned { {item(n)} }
            if show_heads && !rest.is_empty() { div { class: "list-head", "Notes" } }
            for n in rest { {item2(n)} }
        }
    }
}

#[component]
pub fn NoteEditor(note: Note, compact: bool) -> Element {
    let store = use_store();
    let ui = use_ui();
    let id = note.id;

    let mode = if compact { Mode::Write } else { MODE() };
    let projects = store.read(project_options);
    let proj_val = note.project_id.map(|p| p.to_string()).unwrap_or_default();
    let html = if mode != Mode::Write { markdown::to_html(&note.body) } else { String::new() };
    let edited = ago(&note.updated_at);

    let (s_body, s_pin, s_proj, s_mode, s_del) = (store.clone(), store.clone(), store.clone(), store.clone(), store.clone());
    let pinned = note.pinned;
    let empty = note.body.trim().is_empty();

    rsx! {
        div { class: if compact { "note-editor compact" } else { "note-editor" },
            if !compact {
                div { class: "note-toolbar",
                    button { class: if pinned { "icon-btn on" } else { "icon-btn" }, title: if pinned { "Unpin" } else { "Pin" },
                        onclick: move |_| { let _ = s_pin.write(|db| notes::set_pinned(db, id, !pinned)); },
                        Icon { d: icons::PIN }
                    }
                    select { class: "note-project",
                        onchange: move |e| { let _ = s_proj.write(|db| notes::set_project(db, id, e.value().parse().ok())); },
                        option { value: "", selected: proj_val.is_empty(), "No project" }
                        for (pid, name, _) in projects {
                            option { value: "{pid}", selected: proj_val == pid.to_string(), "{name}" }
                        }
                    }
                    span { class: "muted edited", "Edited {edited}" }
                    div { class: "grow" }
                    div { class: "seg small",
                        for m in [Mode::Write, Mode::Split, Mode::Preview] {
                            button {
                                class: if mode == m { "on" },
                                onclick: { let s = s_mode.clone(); move |_| { *MODE.write() = m; s.set_setting("notes.mode", m.as_str()); } },
                                "{m.label()}"
                            }
                        }
                    }
                    button { class: "icon-btn", title: "Delete note",
                        onclick: move |_| {
                            if let Ok(Some(n)) = s_del.write(|db| notes::delete(db, id)) {
                                *SELECTED.write() = None;
                                let title = n.title();
                                ui.notify_undo(format!("Deleted “{title}”"), move |s| {
                                    let _ = s.write(|db| notes::restore(db, &n));
                                    *SELECTED.write() = Some(n.id);
                                });
                            }
                        },
                        Icon { d: icons::TRASH }
                    }
                }
            }
            div { class: "note-panes mode-{mode.as_str()}",
                if mode != Mode::Preview {
                    textarea {
                        class: "note-text",
                        spellcheck: "true",
                        placeholder: "Write in Markdown",
                        value: "{note.body}",
                        // New, empty notes get the cursor straight away.
                        onmounted: move |e| async move {
                            if empty { let _ = e.set_focus(true).await; }
                        },
                        oninput: move |e| {

                            let _ = s_body.write(|db| notes::set_body(db, id, &e.value()));
                        },
                    }
                }
                if mode != Mode::Write {
                    div { class: "md", "data-note": "{id}", dangerous_inner_html: "{html}" }
                }
            }
        }
    }
}
