//! Timezone panel: the selected event's time (or the current time) in each saved zone.
//! For a selected manual event, each zone's time can be edited directly ("make it 8 PM for
//! Alex"), which moves the event while keeping its length. Right-click any time to copy a
//! Discord timestamp (handled by the shell's time menu via `data-ts`).

use super::SELECTED;
use crate::icons::{self, Icon};
use crate::state::use_ui;
use crate::store::use_store;
use cc_core::model::events::{self, fmt_time, input_of, Event};
use cc_core::tz::{self, Zone};
use cc_core::Db;
use chrono::{Duration, Local, NaiveDateTime, NaiveTime};
use dioxus::prelude::*;

const KEY: &str = "tz.zones";

pub fn load_zones(db: &Db) -> Vec<Zone> {
    db.setting(KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(tz::default_zones)
}

fn save_zones(db: &Db, zones: &[Zone]) -> anyhow::Result<()> {
    db.set_setting(KEY, &serde_json::to_string(zones)?)
}

/// "Thu" / "+1 day" style tag when the zone's date differs from local.
fn day_tag(there: NaiveDateTime, here: NaiveDateTime) -> Option<String> {
    let diff = (there.date() - here.date()).num_days();
    (diff != 0).then(|| if diff > 0 { format!("+{diff} day") } else { format!("{diff} day") })
}

fn offset_text(hours: f64) -> String {
    if hours == 0.0 {
        "same as you".into()
    } else {
        let sign = if hours > 0.0 { "+" } else { "−" };
        let h = hours.abs();
        if h.fract() == 0.0 { format!("{sign}{}h", h as i64) } else { format!("{sign}{}h{:02}", h.trunc() as i64, (h.fract() * 60.0).round() as i64) }
    }
}

#[component]
pub fn TzPanel(event: Option<Event>) -> Element {
    let store = use_store();
    let ui = use_ui();
    let _ = ui.clock.read(); // refresh "now" every minute
    let h12 = *ui.h12.read();
    let zones = store.read(load_zones);
    let mut editing_zones = use_signal(|| false);
    let mut new_label = use_signal(String::new);
    let mut new_tz = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    // What we're converting: a timed event, or "now".
    let timed = event.clone().filter(|e| !e.all_day);
    let (base, base_end) = match &timed {
        Some(e) => (e.starts_at, e.ends_at),
        None => (Local::now().naive_local(), None),
    };
    let editable = timed.as_ref().is_some_and(|e| e.is_manual());
    let ts = tz::unix(base);
    let caption = match &timed {
        Some(e) => format!("{} · {}", e.title, base.format("%a %b %-d")),
        None => "Now".to_string(),
    };

    // Move the event so it starts at `wall` in `zone` (None = local), keeping its length.
    let retime = {
        let store = store.clone();
        let timed = timed.clone();
        move |zone: Option<chrono_tz::Tz>, wall: NaiveDateTime| {
            let Some(e) = timed.clone() else { return };
            let local = zone.map(|z| tz::from_zone(wall, z)).unwrap_or(wall);
            let dur = e.ends_at.map(|x| x - e.starts_at).unwrap_or(Duration::hours(1));
            let mut input = input_of(&e);
            let end = local + dur;
            input.date = local.date();
            input.start = Some(local.time());
            input.end = Some(end.time());
            input.end_date = (end.date() != local.date()).then_some(end.date());
            let _ = store.write(|db| events::update(db, e.id, &input));
            *SELECTED.write() = Some(e.id);
        }
    };

    let row = |label: String, sub: String, zone: Option<chrono_tz::Tz>, idx: Option<usize>| {
        let there = zone.map(|z| tz::to_zone(base, z)).unwrap_or(base);
        let there_end = base_end.map(|e| zone.map(|z| tz::to_zone(e, z)).unwrap_or(e));
        let time = match there_end {
            Some(e) if e.date() == there.date() => events::time_range(there.time(), e.time(), h12),
            _ => fmt_time(there.time(), h12),
        };
        let tag = day_tag(there, base);
        let date = there.format("%a %b %-d").to_string();
        let value = there.format("%H:%M").to_string();
        let retime = retime.clone();
        let store = store.clone();
        let zones_now = zones.clone();
        rsx! {
            div { class: if zone.is_none() { "tz-row you" } else { "tz-row" }, "data-ts": "{ts}", title: "Right-click to copy for Discord",
                div { class: "tz-who",
                    span { class: "tz-label", "{label}" }
                    span { class: "tz-sub", "{sub}" }
                }
                div { class: "tz-time",
                    if editable {
                        input { class: "tz-input", r#type: "time", value: "{value}",
                            onchange: move |e| if let Ok(t) = NaiveTime::parse_from_str(&e.value(), "%H:%M") { retime(zone, there.date().and_time(t)); } }
                    } else {
                        span { class: "tz-clock", "{time}" }
                    }
                    span { class: "tz-date", "{date}" if let Some(t) = &tag { span { class: "tz-tag", " · {t}" } } }
                }
                if let (true, Some(i)) = (editing_zones(), idx) {
                    button { class: "icon-btn", title: "Remove",
                        onclick: move |_| { let mut z = zones_now.clone(); z.remove(i); let _ = store.write(|db| save_zones(db, &z)); },
                        Icon { d: icons::CLOSE }
                    }
                }
            }
        }
    };

    let s_add = store.clone();
    let mut add = move || {
        let tz_name = new_tz.peek().trim().to_string();
        if tz::parse(&tz_name).is_none() {
            error.set(Some(format!("Unknown time zone “{tz_name}”. Pick one from the list, e.g. Europe/London.")));
            return;
        }
        let label = new_label.peek().trim().to_string();
        let label = if label.is_empty() { tz_name.rsplit('/').next().unwrap_or(&tz_name).replace('_', " ") } else { label };
        let mut z = s_add.peek(load_zones);
        z.push(Zone { label, tz: tz_name });
        let _ = s_add.write(|db| save_zones(db, &z));
        new_label.set(String::new());
        new_tz.set(String::new());
        error.set(None);
    };
    let mut add_k = add.clone();

    rsx! {
        aside { class: "panel tz-panel",
            div { class: "panel-head",
                h3 { "Timezones" }
                span { class: "muted small", "{caption}" }
                div { class: "grow" }
                button { class: if editing_zones() { "icon-btn on" } else { "icon-btn" }, title: "Add or remove zones",
                    onclick: move |_| editing_zones.toggle(), Icon { d: icons::SETTINGS } }
            }
            {row("You".into(), "local time".into(), None, None)}
            for (i, z) in zones.iter().enumerate() {
                {
                    let zone = tz::parse(&z.tz);
                    let sub = match zone {
                        Some(tzv) => format!("{} · {}", tz::abbreviation(base, tzv), offset_text(tz::offset_from_local_hours(base, tzv))),
                        None => "unknown zone".into(),
                    };
                    if zone.is_some() { row(z.label.clone(), sub, zone, Some(i)) } else { rsx! {} }
                }
            }
            if editing_zones() {
                div { class: "tz-add",
                    input { class: "pd-input", placeholder: "Name (e.g. Alex)", value: "{new_label}", oninput: move |e| new_label.set(e.value()) }
                    input { class: "pd-input", placeholder: "Zone (e.g. Europe/London)", list: "tz-names", value: "{new_tz}",
                        oninput: move |e| new_tz.set(e.value()),
                        onkeydown: move |e| if e.key() == Key::Enter { add_k(); } }
                    button { class: "btn", onclick: move |_| add(), "Add" }
                    datalist { id: "tz-names",
                        for n in tz::all_names() { option { value: "{n}" } }
                    }
                }
                if let Some(err) = error() { div { class: "chip warn", "{err}" } }
            }
            if timed.is_some() && !editable {
                div { class: "muted small", "Synced events are read-only." }
            }
        }
    }
}
