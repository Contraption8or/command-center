//! Right-click menu for any time in the app (elements carrying `data-ts`): copy it as a
//! Discord timestamp, which every Discord user sees in their own time zone, or as plain
//! text listing your saved zones.

use crate::state::{use_ui, Ui};
use crate::store::use_store;
use crate::widgets::events::load_zones;
use cc_core::tz::{self, discord_tag, DiscordStyle};
use chrono::{Local, TimeZone};
use dioxus::prelude::*;

fn copy(ui: Ui, text: &str, what: &str) {
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
        Ok(()) => ui.notify(format!("Copied {what}")),
        Err(e) => ui.error(format!("Could not copy: {e}")),
    }
}

#[component]
pub fn TimeMenu(ts: i64, x: f64, y: f64) -> Element {
    let store = use_store();
    let mut ui = use_ui();
    let h12 = *ui.h12.read();
    let now = chrono::Utc::now().timestamp();
    let local = Local.timestamp_opt(ts, 0).single().map(|d| d.naive_local()).unwrap_or_default();
    let zones = store.read(load_zones);

    // Plain-text version for places that don't render Discord tags.
    let fmt = |t: chrono::NaiveDateTime| {
        if h12 { t.format("%a %b %-d, %-I:%M %p").to_string() } else { t.format("%a %b %-d, %H:%M").to_string() }
    };
    let mut lines = vec![format!("{} (my time)", fmt(local))];
    for z in &zones {
        if let Some(zone) = tz::parse(&z.tz) {
            let there = tz::to_zone(local, zone);
            lines.push(format!("{} ({}, {})", fmt(there), z.label, tz::abbreviation(local, zone)));
        }
    }
    let all_text = lines.join("\n");

    // Keep the menu on screen near the pointer.
    let style = format!("left: min({x}px, calc(100vw - 330px)); top: min({y}px, calc(100vh - 340px));");

    rsx! {
        div { class: "menu-scrim",
            onclick: move |_| ui.time_menu.set(None),
            oncontextmenu: move |e| { e.prevent_default(); ui.time_menu.set(None); },
            div { class: "time-menu", style: "{style}", onclick: move |e| e.stop_propagation(),
                div { class: "tm-head", "Copy for Discord" }
                for s in DiscordStyle::MENU {
                    {
                        let tag = discord_tag(ts, s);
                        let t2 = tag.clone();
                        rsx! {
                            button { key: "{tag}", class: "tm-item",
                                onclick: move |_| { copy(ui, &t2, &t2); let mut m = ui.time_menu; m.set(None); },
                                span { class: "tm-label", "{s.label()}" }
                                span { class: "tm-preview", "{s.preview(ts, now)}" }
                            }
                        }
                    }
                }
                div { class: "tm-sep" }
                button { class: "tm-item",
                    onclick: move |_| { copy(ui, &all_text, "times for all zones"); let mut m = ui.time_menu; m.set(None); },
                    span { class: "tm-label", "Copy as text for every zone" }
                    span { class: "tm-preview", "{zones.len() + 1} lines" }
                }
            }
        }
    }
}
