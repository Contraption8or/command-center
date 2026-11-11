//! Month view: six Mon–Sun weeks. Click an event to open it; double-click a day to add an
//! event at 9:00; click a day number to open that week.

use super::{create_at, ANCHOR, SELECTED, show_week};
use crate::state::use_ui;
use crate::store::use_store;
use cc_core::model::events::{self, fmt_time};
use cc_core::model::habits::week_start;
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;

const MAX_PER_DAY: usize = 3;

#[component]
pub fn MonthView(anchor: NaiveDate) -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    let h12 = *ui.h12.read();
    let first = anchor.with_day(1).unwrap();
    let start = week_start(first);
    let end = start + Duration::days(41);
    let list = store.read(|db| events::between(db, start, end));
    let selected = SELECTED();

    rsx! {
        div { class: "month",
            div { class: "month-head",
                for d in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] { div { key: "{d}", "{d}" } }
            }
            div { class: "month-grid",
                for i in 0..42i64 {
                    {
                        let day = start + Duration::days(i);
                        let mut evs: Vec<_> = list.iter().filter(|e| !e.days_in(day, day).is_empty()).cloned().collect();
                        evs.sort_by_key(|e| (!e.all_day, e.starts_at));
                        let more = evs.len().saturating_sub(MAX_PER_DAY);
                        let class = format!(
                            "m-day{}{}",
                            if day.month() != first.month() { " other" } else { "" },
                            if day == today { " today" } else { "" }
                        );
                        let st = store.clone();
                        rsx! {
                            div {
                                key: "{day}",
                                class: "{class}",
                                ondoubleclick: move |_| create_at(&st, day, 9 * 60, 10 * 60),
                                button {
                                    class: "m-num",
                                    title: "Open this week",
                                    onclick: move |_| { *ANCHOR.write() = Some(day); show_week(); },
                                    "{day.day()}"
                                }
                                for e in evs.into_iter().take(MAX_PER_DAY) {
                                    button {
                                        key: "{e.id}",
                                        class: if selected == Some(e.id) { "m-ev sel" } else if e.all_day { "m-ev allday" } else { "m-ev" },
                                        style: "{super::color_var(&e)}",
                                        "data-ev-id": "{e.id}",
                                        "data-manual": if e.is_manual() { "1" } else { "0" },
                                        "data-ts": "{cc_core::tz::unix(e.starts_at)}",
                                        title: "{e.title}",
                                        ondoubleclick: move |ev| ev.stop_propagation(),
                                        onclick: move |_| { let next = if SELECTED() == Some(e.id) { None } else { Some(e.id) }; *SELECTED.write() = next; },
                                        if !e.all_day { span { class: "m-time", "{fmt_time(e.starts_at.time(), h12)}" } }
                                        span { class: "m-title", "{e.title}" }
                                    }
                                }
                                if more > 0 {
                                    button { class: "m-more", onclick: move |_| { *ANCHOR.write() = Some(day); show_week(); }, "+{more} more" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
