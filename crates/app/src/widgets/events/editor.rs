//! Event editor panel. Changes save immediately; synced events are shown read-only.

use super::{JUST_CREATED, SELECTED};
use crate::icons::{self, Icon};
use crate::state::use_ui;
use crate::store::use_store;
use cc_core::model::events::{self, input_of, Event, EventInput};
use chrono::{NaiveDate, NaiveTime};
use dioxus::prelude::*;

#[component]
pub fn EventEditor(event: Event, autofocus: bool) -> Element {
    let store = use_store();
    let ui = use_ui();
    let h12 = *ui.h12.read();
    let id = event.id;
    let base = input_of(&event);
    let mut title = use_signal(|| event.title.clone());
    let mut location = use_signal(|| event.location.clone());
    let mut notes = use_signal(|| event.notes.clone());
    let mut error = use_signal(|| None::<String>);

    if !event.is_manual() {
        return rsx! {
            aside { class: "panel event-editor",
                div { class: "pd-head",
                    span { class: "pd-name static", "{event.title}" }
                    button { class: "icon-btn", onclick: move |_| *SELECTED.write() = None, Icon { d: icons::CLOSE } }
                }
                div { class: "muted", "{event.start_date().format(\"%A, %B %-d\")} · {event.time_label(h12)}" }
                if !event.location.is_empty() { div { "{event.location}" } }
                if !event.notes.is_empty() { div { class: "muted small", "{event.notes}" } }
                div { class: "muted small", "Synced from {event.source}. Edit it there." }
            }
        };
    }

    // Apply one change on top of the stored values and save.
    let save = {
        let store = store.clone();
        let base = base.clone();
        move |change: Box<dyn FnOnce(&mut EventInput)>| {
            let mut input = base.clone();
            input.title = title.peek().clone();
            input.location = location.peek().clone();
            input.notes = notes.peek().clone();
            change(&mut input);
            match store.write(|db| events::update(db, id, &input)) {
                Ok(()) => error.set(None),
                Err(e) => error.set(Some(e.to_string())),
            }
        }
    };
    let (mut sv1, mut sv2, mut sv3, mut sv4, mut sv5, mut sv6, mut sv7, mut sv8) =
        (save.clone(), save.clone(), save.clone(), save.clone(), save.clone(), save.clone(), save.clone(), save);
    let pd = |v: &str| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok();
    let pt = |v: &str| NaiveTime::parse_from_str(v, "%H:%M").ok();
    let all_day = base.start.is_none();
    let date_v = base.date.to_string();
    let start_v = base.start.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
    let end_v = base.end.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
    let end_date_v = base.end_date.filter(|d| *d > base.date).map(|d| d.to_string()).unwrap_or_default();
    let s_del = store.clone();

    rsx! {
        aside { class: "panel event-editor",
            div { class: "pd-head",
                input {
                    class: "pd-name", value: "{title}", spellcheck: "false",
                    onmounted: move |e| async move {
                        if autofocus {
                            *JUST_CREATED.write() = None;
                            let _ = e.set_focus(true).await;
                            document::eval("document.activeElement?.select?.()");
                        }
                    },
                    oninput: move |e| { title.set(e.value()); if !e.value().trim().is_empty() { sv1(Box::new(|_| {})); } },
                }
                button { class: "icon-btn", title: "Close", onclick: move |_| *SELECTED.write() = None, Icon { d: icons::CLOSE } }
            }

            div { class: "ev-fields",
                label { class: "pd-field",
                    span { class: "pd-label", "Date" }
                    input { class: "pd-input", r#type: "date", value: "{date_v}",
                        onchange: move |e| if let Some(d) = pd(&e.value()) { sv2(Box::new(move |i| i.date = d)); } }
                }
                label { class: "pd-field check",
                    input { r#type: "checkbox", checked: all_day,
                        onchange: move |e| {
                            let on = e.checked();
                            sv3(Box::new(move |i| {
                                if on { i.start = None; i.end = None; }
                                else { i.start = NaiveTime::from_hms_opt(9, 0, 0); i.end = NaiveTime::from_hms_opt(10, 0, 0); i.end_date = None; }
                            }));
                        } }
                    span { "All day" }
                }
            }
            if all_day {
                label { class: "pd-field",
                    span { class: "pd-label", "Until (optional)" }
                    input { class: "pd-input", r#type: "date", value: "{end_date_v}", min: "{date_v}",
                        onchange: move |e| { let d = pd(&e.value()); sv4(Box::new(move |i| i.end_date = d)); } }
                }
            } else {
                div { class: "ev-fields",
                    label { class: "pd-field",
                        span { class: "pd-label", "Start" }
                        input { class: "pd-input", r#type: "time", value: "{start_v}",
                            onchange: move |e| if let Some(t) = pt(&e.value()) {
                                sv5(Box::new(move |i| {
                                    // Keep the same duration when the start moves.
                                    if let (Some(s), Some(en)) = (i.start, i.end) {
                                        let dur = en - s;
                                        if dur > chrono::Duration::zero() { i.end = Some(t + dur); }
                                    }
                                    i.start = Some(t);
                                    i.end_date = None;
                                }));
                            } }
                    }
                    label { class: "pd-field",
                        span { class: "pd-label", "End" }
                        input { class: "pd-input", r#type: "time", value: "{end_v}",
                            onchange: move |e| { let t = pt(&e.value()); sv6(Box::new(move |i| { i.end = t; i.end_date = None; })); } }
                    }
                }
            }
            label { class: "pd-field",
                span { class: "pd-label", "Location" }
                input { class: "pd-input", value: "{location}", placeholder: "Discord, YouTube, an address…",
                    oninput: move |e| { location.set(e.value()); sv7(Box::new(|_| {})); } }
            }
            label { class: "pd-field",
                span { class: "pd-label", "Notes" }
                textarea { class: "pd-input", rows: 4, value: "{notes}",
                    oninput: move |e| { notes.set(e.value()); sv8(Box::new(|_| {})); } }
            }
            if let Some(err) = error() { div { class: "chip warn", "{err}" } }
            div { class: "pd-footer",
                span { class: "muted small", "data-ts": "{cc_core::tz::unix(event.starts_at)}", title: "Right-click to copy for Discord",
                    "{event.start_date().format(\"%A, %B %-d\")} · {event.time_label(h12)}" }
                div { class: "grow" }
                button { class: "btn danger",
                    onclick: move |_| {
                        if let Ok(Some(ev)) = s_del.write(|db| events::delete(db, id)) {
                            *SELECTED.write() = None;
                            let label = ev.title.clone();
                            ui.notify_undo(format!("Deleted “{label}”"), move |s| {
                                let _ = s.write(|db| events::restore(db, &ev));
                                *SELECTED.write() = Some(ev.id);
                            });
                        }
                    },
                    "Delete"
                }
            }
        }
    }
}
