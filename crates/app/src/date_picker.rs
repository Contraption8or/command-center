//! Themed date picker. The native `<input type="date">` popup is drawn by the system and
//! only follows light/dark, so the app uses this instead: a button showing the date that
//! opens a small month calendar in the app's own colours.

use crate::icons::{self, Icon};
use crate::state::use_ui;
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;

#[component]
pub fn DatePicker(
    value: Option<NaiveDate>,
    onchange: EventHandler<Option<NaiveDate>>,
    /// Offer "Clear" (for optional dates like a due date).
    #[props(default)]
    clearable: bool,
    /// Earliest selectable day.
    #[props(default)]
    min: Option<NaiveDate>,
    #[props(default)]
    placeholder: Option<String>,
) -> Element {
    let ui = use_ui();
    let today = *ui.today.read();
    // Where the popup opens (client coordinates of the click), or None when closed.
    let mut open = use_signal(|| None::<(f64, f64)>);
    let mut month = use_signal(|| value.unwrap_or(today).with_day(1).unwrap());

    let label = match value {
        Some(d) if d.year() == today.year() => d.format("%a, %b %-d").to_string(),
        Some(d) => d.format("%b %-d, %Y").to_string(),
        None => placeholder.clone().unwrap_or_else(|| "No date".into()),
    };
    let mut pick = move |d: Option<NaiveDate>| {
        open.set(None);
        if d != value {
            onchange.call(d);
        }
    };

    rsx! {
        button {
            class: if value.is_some() { "date-field" } else { "date-field empty" },
            r#type: "button",
            onclick: move |e| {
                let p = e.client_coordinates();
                month.set(value.unwrap_or(today).with_day(1).unwrap());
                open.set(Some((p.x, p.y)));
            },
            Icon { d: icons::CALENDAR }
            span { "{label}" }
        }
        if let Some((x, y)) = open() {
            div { class: "dp-scrim",
                onclick: move |_| open.set(None),
                oncontextmenu: move |e| { e.prevent_default(); open.set(None); },
                div {
                    class: "dp-pop",
                    style: "left: max(8px, min({x - 24.0}px, calc(100vw - 268px))); top: min({y + 16.0}px, calc(100vh - 318px));",
                    onclick: move |e| e.stop_propagation(),
                    {
                        let first = month();
                        let start = cc_core::model::habits::week_start(first);
                        let title = first.format("%B %Y").to_string();
                        rsx! {
                            div { class: "dp-head",
                                span { class: "dp-title", "{title}" }
                                div { class: "grow" }
                                button { class: "icon-btn", title: "Previous month", onclick: move |_| month.set(crate::widgets::events::add_months(first, -1)), Icon { d: icons::ARROW_LEFT } }
                                button { class: "icon-btn", title: "Next month", onclick: move |_| month.set(crate::widgets::events::add_months(first, 1)), Icon { d: icons::ARROW_RIGHT } }
                            }
                            div { class: "dp-grid",
                                for w in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] { span { key: "{w}", class: "dp-dow", "{w}" } }
                                for i in 0..42i64 {
                                    {
                                        let d = start + Duration::days(i);
                                        let disabled = min.is_some_and(|m| d < m);
                                        let class = format!(
                                            "dp-day{}{}{}",
                                            if d.month() != first.month() { " other" } else { "" },
                                            if d == today { " today" } else { "" },
                                            if Some(d) == value { " sel" } else { "" },
                                        );
                                        rsx! {
                                            button { key: "{d}", class: "{class}", disabled, onclick: move |_| pick(Some(d)), "{d.day()}" }
                                        }
                                    }
                                }
                            }
                            div { class: "dp-foot",
                                if clearable { button { class: "dp-link", onclick: move |_| pick(None), "Clear" } }
                                div { class: "grow" }
                                if !min.is_some_and(|m| today < m) {
                                    button { class: "dp-link", onclick: move |_| pick(Some(today)), "Today" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
