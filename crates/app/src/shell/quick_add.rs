//! One-line quick-add box (Ctrl+J in the app, or the global hotkey from anywhere).

use crate::icons::{self, Icon};
use crate::state::{use_ui, Ui};
use crate::store::use_store;
use cc_core::model::{apply_quick_add, find_project, Created};
use cc_core::quick_add::{parse, QuickAdd as Entry};
use dioxus::desktop::{use_window, DesktopContext};
use dioxus::prelude::*;

#[component]
pub fn QuickAdd(prefill: String) -> Element {
    let store = use_store();
    let ui = use_ui();
    let win = use_window();
    let mut text = use_signal(|| prefill.clone());
    let today = *ui.today.read();

    let parsed = parse(&text.read(), today);
    let chips = match &parsed {
        None => rsx! {},
        Some(Entry::Note(_)) => rsx! { span { class: "chip kind", "Note" } },
        Some(Entry::Event(e)) => {
            let when = crate::widgets::tasks::day_label(e.date, today);
            let time = match (e.start, e.end) {
                (None, _) => "All day".to_string(),
                (Some(s), None) => cc_core::model::events::fmt_time(s, *ui.h12.read()),
                (Some(s), Some(x)) => cc_core::model::events::time_range(s, x, *ui.h12.read()),
            };
            rsx! { span { class: "chip kind", "Event" } span { class: "chip", "{when}" } span { class: "chip", "{time}" } }
        }
        Some(Entry::Task(t)) => {
            let when = if t.scheduled_for == today { "Today".to_string() } else { t.scheduled_for.format("%a %b %-d").to_string() };
            let due = t.due.map(|d| format!("Due {}", if d == today { "today".into() } else { d.format("%a %b %-d").to_string() }));
            let project = t.project_query.as_ref().map(|q| (q.clone(), store.peek(|db| find_project(db, q))));
            rsx! {
                span { class: "chip kind", "Task" }
                span { class: "chip", "{when}" }
                if let Some(d) = due { span { class: "chip", "{d}" } }
                if t.steps > 1 { span { class: "chip", "{t.steps} steps" } }
                match project {
                    Some((_, Some((_, name)))) => rsx! { span { class: "chip", "{name}" } },
                    Some((q, None)) => rsx! { span { class: "chip warn", "No project “{q}”" } },
                    None => rsx! {},
                }
            }
        }
    };

    let submit = {
        let store = store.clone();
        let win = win.clone();
        move || {
            let Some(entry) = parse(&text.peek(), *ui.today.peek()) else { return };
            match store.write(|db| apply_quick_add(db, &entry)) {
                Ok(Created::Task { project, .. }) => ui.notify(match project {
                    Some(p) => format!("Task added · {p}"),
                    None => "Task added".to_string(),
                }),
                Ok(Created::Note { .. }) => ui.notify("Note saved"),
                Ok(Created::Event { .. }) => ui.notify("Event added"),
                Err(e) => {
                    ui.error(format!("Could not save: {e}"));
                    return;
                }
            }
            dismiss(ui, &win);
        }
    };

    rsx! {
        div { class: "scrim", onclick: move |_| dismiss(ui, &win),
            div { class: "palette quick", onclick: move |e| e.stop_propagation(),
                div { class: "input-row",
                    Icon { d: icons::PLUS }
                    input {
                        placeholder: "Quick add",
                        spellcheck: "false",
                        value: "{text}",
                        onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                        oninput: move |e| text.set(e.value()),
                        onkeydown: move |e| if e.key() == Key::Enter { e.prevent_default(); submit(); },
                    }
                }
                div { class: "preview", {chips} }
            }
        }
    }
}

fn dismiss(mut ui: Ui, win: &DesktopContext) {
    ui.quick_add.set(None);
    if *ui.summoned.peek() {
        ui.summoned.set(false);
        win.set_minimized(true);
    }
}
