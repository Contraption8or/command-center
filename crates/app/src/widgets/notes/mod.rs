//! Quick notes: scratchpad card and full view with Markdown preview.

mod editor;

use super::{Hit, WidgetDef};
use crate::icons::{self, Icon};
use crate::state::{use_ui, Action, Command, Ui, View};
use crate::store::{use_store, Store};
use cc_core::model::notes;
use dioxus::prelude::*;
use editor::{NoteEditor, NoteList};

pub static DEF: WidgetDef = WidgetDef {
    id: "notes",
    title: "Notes",
    icon: icons::NOTES,
    cols: 12,
    rows: 2,
    card,
    full,
    commands,
    search: Some(search),
    open_item: Some(open_item),
    on_message: Some(on_message),
};

/// The note open in the editor (shared by card and full view).
pub static SELECTED: GlobalSignal<Option<i64>> = Signal::global(|| None);
static MODE: GlobalSignal<Mode> = Signal::global(|| Mode::Split);
static MODE_LOADED: GlobalSignal<bool> = Signal::global(|| false);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Write,
    Split,
    Preview,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Write => "write",
            Mode::Split => "split",
            Mode::Preview => "preview",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Mode::Write => "Write",
            Mode::Split => "Split",
            Mode::Preview => "Preview",
        }
    }
}

/// Switch the selected note. A note left completely empty is discarded on the way out.
pub fn select(store: &Store, id: Option<i64>) {
    let prev = *SELECTED.peek();
    if prev == id {
        return;
    }
    if let Some(p) = prev {
        let _ = store.write(|db| notes::delete_if_empty(db, p));
    }
    *SELECTED.write() = id;
}

/// Create an empty note, select it and open the full view (Ctrl+N).
pub fn new_note(store: &Store, ui: Ui) {
    if let Ok(id) = store.write(|db| notes::add(db, "", None)) {
        select(store, Some(id));
        ui.open(View::Widget("notes"));
    }
}

/// Selected note if it still exists, otherwise the first in `list`.
fn current(list: &[notes::Note]) -> Option<notes::Note> {
    let sel = SELECTED();
    sel.and_then(|id| list.iter().find(|n| n.id == id)).or_else(|| list.first()).cloned()
}

fn card() -> Element {
    let store = use_store();
    let list = store.read(|db| notes::list(db, ""));
    let cur = current(&list);
    let sel = cur.as_ref().map(|n| n.id);
    let s_new = store.clone();
    rsx! {
        div { class: "notes-card",
            div { class: "notes-card-list",
                button { class: "new-note", onclick: move |_| {
                    if let Ok(id) = s_new.write(|db| notes::add(db, "", None)) { select(&s_new, Some(id)); }
                }, Icon { d: icons::PLUS } "New note" }
                NoteList { notes: list.clone(), selected: sel, compact: true }
            }
            match cur {
                Some(n) => rsx! { NoteEditor { key: "{n.id}", note: n, compact: true } },
                None => rsx! { div { class: "muted empty-line", "No notes" } },
            }
        }
    }
}

fn full() -> Element {
    let store = use_store();
    let ui = use_ui();
    if !MODE_LOADED() {
        if let Some(m) = store.setting("notes.mode") {
            *MODE.write() = match m.as_str() { "write" => Mode::Write, "preview" => Mode::Preview, _ => Mode::Split };
        }
        *MODE_LOADED.write() = true;
    }
    let mut filter = use_signal(String::new);
    let list = store.read(|db| notes::list(db, &filter.read()));
    let cur = current(&list);
    let sel = cur.as_ref().map(|n| n.id);
    let s_new = store.clone();

    rsx! {
        div { class: "notes-full",
            aside { class: "panel notes-side",
                div { class: "notes-side-head",
                    div { class: "search-field",
                        Icon { d: icons::SEARCH }
                        input { "data-find": "", placeholder: "Search notes", spellcheck: "false", value: "{filter}",
                            oninput: move |e| filter.set(e.value()) }
                    }
                    button { class: "icon-btn", title: "New note  (Ctrl+N)", onclick: move |_| new_note(&s_new, ui), Icon { d: icons::PLUS } }
                }
                if list.is_empty() {
                    div { class: "muted empty-line", if filter.read().is_empty() { "No notes" } else { "No matches" } }
                }
                NoteList { notes: list.clone(), selected: sel, compact: false }
            }
            section { class: "panel notes-main",
                match cur {
                    Some(n) => rsx! { NoteEditor { key: "{n.id}", note: n, compact: false } },
                    None => rsx! { div { class: "muted empty-line", "No note selected" } },
                }
            }
        }
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::NOTES, "Notes", Action::Open(View::Widget("notes"))).keywords("scratchpad markdown").shortcut("Ctrl+6"),
        Command::new("Create", icons::PLUS, "New note", Action::Run(new_note)).keywords("scratchpad").shortcut("Ctrl+N"),
    ]
}

fn search(db: &cc_core::Db, q: &str) -> Vec<Hit> {
    notes::list(db, q)
        .into_iter()
        .map(|n| Hit { id: n.id, label: n.title(), detail: n.project_name.clone().unwrap_or_default() })
        .collect()
}

fn open_item(id: i64) {
    *SELECTED.write() = Some(id);
}

/// `task:<note id>:<checkbox index>` — a checkbox was clicked in a rendered preview.
fn on_message(store: &Store, _ui: Ui, payload: &str) {
    let mut parts = payload.split(':');
    if let (Some("task"), Some(Ok(id)), Some(Ok(idx))) =
        (parts.next(), parts.next().map(str::parse::<i64>), parts.next().map(str::parse::<usize>))
    {
        let _ = store.write(|db| notes::toggle_checkbox(db, id, idx));
    }
}
