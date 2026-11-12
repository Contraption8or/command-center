//! Upcoming events: agenda card, 7/14-day agenda and event editor.

mod editor;
mod month;
mod tzpanel;

pub use tzpanel::load_zones;
mod week;

use super::{Hit, WidgetDef};
use crate::icons::{self, Icon};
use crate::state::{use_ui, Action, Command, Ui, View};
use crate::store::{use_store, Store};
use crate::widgets::tasks::day_label;
use cc_core::model::events::{self, Event, EventInput};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use dioxus::prelude::*;

pub static DEF: WidgetDef = WidgetDef {
    id: "events",
    title: "Upcoming",
    icon: icons::CALENDAR,
    cols: 4,
    rows: 2,
    card,
    full,
    commands,
    search: Some(search),
    open_item: Some(open_item),
    on_message: None,
};

pub static SELECTED: GlobalSignal<Option<i64>> = Signal::global(|| None);
pub(crate) static JUST_CREATED: GlobalSignal<Option<i64>> = Signal::global(|| None);
static RANGE: GlobalSignal<Option<i64>> = Signal::global(|| None);
/// Which calendar view is shown (loaded from settings on first use).
static VIEW: GlobalSignal<Option<CalView>> = Signal::global(|| None);
/// Day the week/month views are centred on (`None` = today).
pub(crate) static ANCHOR: GlobalSignal<Option<NaiveDate>> = Signal::global(|| None);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CalView {
    Agenda,
    Week,
    Month,
}

impl CalView {
    fn key(self) -> &'static str {
        match self {
            CalView::Agenda => "agenda",
            CalView::Week => "week",
            CalView::Month => "month",
        }
    }
    fn label(self) -> &'static str {
        match self {
            CalView::Agenda => "Agenda",
            CalView::Week => "Week",
            CalView::Month => "Month",
        }
    }
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("week") => CalView::Week,
            Some("month") => CalView::Month,
            _ => CalView::Agenda,
        }
    }
}

/// Switch the full view to the week grid (used by the month view's day links).
pub(crate) fn show_week() {
    *VIEW.write() = Some(CalView::Week);
}

/// Create a timed event and select it with its title ready to type.
pub(crate) fn create_at(store: &Store, date: NaiveDate, start_min: i64, end_min: i64) {
    let t = |m: i64| NaiveTime::from_num_seconds_from_midnight_opt((m.clamp(0, 1439) * 60) as u32, 0);
    let input = EventInput {
        title: "New event".into(),
        date,
        start: t(start_min),
        end: if end_min >= 1440 { t(0) } else { t(end_min) },
        end_date: None,
        location: String::new(),
        notes: String::new(),
    };
    if let Ok(id) = store.write(|db| events::add(db, &input)) {
        *SELECTED.write() = Some(id);
        *JUST_CREATED.write() = Some(id);
    }
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

/// Create an event on `day` at the next full hour (or 9:00 on other days), select it.
pub fn create_on(store: &Store, ui: Ui, day: NaiveDate) {
    let today = *ui.today.peek();
    let hour = if day == today { (now().hour() + 1).min(23) } else { 9 };
    let start = NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    let input = EventInput {
        title: "New event".into(),
        date: day,
        start: Some(start),
        end: NaiveTime::from_hms_opt((hour + 1).min(23), if hour == 23 { 59 } else { 0 }, 0),
        end_date: None,
        location: String::new(),
        notes: String::new(),
    };
    if let Ok(id) = store.write(|db| events::add(db, &input)) {
        *SELECTED.write() = Some(id);
        *JUST_CREATED.write() = Some(id);
        ui.open(View::Widget("events"));
    }
}

fn new_event(store: &Store, ui: Ui) {
    let today = *ui.today.peek();
    create_on(store, ui, today);
}

/// Group events by the days they appear on within `from..=to`.
fn by_day(list: &[Event], from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, Vec<Event>)> {
    let mut days: Vec<(NaiveDate, Vec<Event>)> = Vec::new();
    let mut d = from;
    while d <= to {
        days.push((d, Vec::new()));
        d += Duration::days(1);
    }
    for e in list {
        for day in e.days_in(from, to) {
            let i = (day - from).num_days() as usize;
            days[i].1.push(e.clone());
        }
    }
    days
}

#[component]
fn EventRow(event: Event, compact: bool) -> Element {
    let ui = use_ui();
    let n = now();
    let id = event.id;
    let sel = !compact && SELECTED() == Some(id);
    let class = format!(
        "ev-row{}{}{}{}",
        if event.done { " done" } else { "" },
        if event.is_now(n) { " now" } else { "" },
        if event.is_past(n) { " past" } else { "" },
        if sel { " sel" } else { "" }
    );
    let h12 = *ui.h12.read();
    let time = if compact && !event.all_day { events::fmt_time(event.starts_at.time(), h12) } else { event.time_label(h12) };
    rsx! {
        button {
            class: "{class}",
            "data-row": "",
            "data-ev-id": "{id}",
            "data-ts": "{cc_core::tz::unix(event.starts_at)}",
            onclick: move |_| {
                if compact {
                    *SELECTED.write() = Some(id);
                    ui.open(View::Widget("events"));
                } else {
                    let next = if SELECTED() == Some(id) { None } else { Some(id) };
                    *SELECTED.write() = next;
                }
            },
            if event.done {
                span { class: "ev-check", style: "{color_var(&event)}", Icon { d: icons::CHECK } }
            } else {
                span { class: "ev-dot", style: "{color_var(&event)}" }
            }
            span { class: if event.all_day { "ev-time allday" } else { "ev-time" }, "{time}" }
            span { class: "ev-title", "{event.title}" }
            if !event.location.is_empty() { span { class: "ev-loc", "{event.location}" } }
            if !event.is_manual() { span { class: "chip", title: "Synced from {event.source}", "{event.source}" } }
        }
    }
}

fn card() -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let to = today + Duration::days(6);
    let list = store.read(|db| events::between(db, today, to));
    let days: Vec<_> = by_day(&list, today, to).into_iter().filter(|(_, v)| !v.is_empty()).collect();
    rsx! {
        div { class: "agenda compact", "data-list": "",
            if days.is_empty() { div { class: "muted empty-line", "Nothing in the next 7 days" } }
            for (day, evs) in days {
                div { key: "{day}", class: "ag-day",
                    div { class: "ag-head", span { "{heading(day, today)}" } span { class: "muted", "{day.format(\"%b %-d\")}" } }
                    for e in evs { EventRow { key: "{e.id}", event: e, compact: true } }
                }
            }
        }
    }
}

fn full() -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    if RANGE().is_none() {
        let r = store.setting("events.range").and_then(|v| v.parse().ok()).unwrap_or(14);
        *RANGE.write() = Some(r);
    }
    if VIEW().is_none() {
        *VIEW.write() = Some(CalView::parse(store.setting("events.view").as_deref()));
    }
    let view = VIEW().unwrap_or(CalView::Agenda);
    let anchor = ANCHOR().unwrap_or(today);
    let range = RANGE().unwrap_or(14);
    let selected = SELECTED().and_then(|id| store.read(|db| events::get(db, id)));
    let (s_new, s_range, s_view) = (store.clone(), store.clone(), store.clone());

    // Title and step for the ‹ › buttons.
    let (title, step) = match view {
        CalView::Week => {
            let mon = cc_core::model::habits::week_start(anchor);
            let sun = mon + Duration::days(6);
            let t = if mon.month() == sun.month() {
                format!("{} – {}", mon.format("%B %-d"), sun.format("%-d, %Y"))
            } else {
                format!("{} – {}", mon.format("%b %-d"), sun.format("%b %-d, %Y"))
            };
            (t, 7)
        }
        CalView::Month => (anchor.format("%B %Y").to_string(), 0),
        CalView::Agenda => (String::new(), 0),
    };
    let shift = move |dir: i64| {
        let a = ANCHOR().unwrap_or(today);
        let next = if view == CalView::Month { add_months(a, dir) } else { a + Duration::days(step * dir) };
        *ANCHOR.write() = Some(next);
    };

    rsx! {
        div { class: "events-full with-side",
            div { class: "panel cal-main",
                div { class: "projects-toolbar",
                    div { class: "seg small",
                        for v in [CalView::Agenda, CalView::Week, CalView::Month] {
                            button {
                                class: if view == v { "on" },
                                onclick: { let s = s_view.clone(); move |_| { *VIEW.write() = Some(v); s.set_setting("events.view", v.key()); } },
                                "{v.label()}"
                            }
                        }
                    }
                    if view == CalView::Agenda {
                        div { class: "seg small",
                            for r in [7i64, 14] {
                                button {
                                    class: if range == r { "on" },
                                    onclick: { let s = s_range.clone(); move |_| { *RANGE.write() = Some(r); s.set_setting("events.range", &r.to_string()); } },
                                    "{r} days"
                                }
                            }
                        }
                    } else {
                        div { class: "cal-nav",
                            button { class: "icon-btn", title: "Previous", onclick: move |_| shift(-1), Icon { d: icons::ARROW_LEFT } }
                            button { class: "btn small-btn", onclick: move |_| *ANCHOR.write() = None, "Today" }
                            button { class: "icon-btn", title: "Next", onclick: move |_| shift(1), Icon { d: icons::ARROW_RIGHT } }
                            span { class: "cal-title", "{title}" }
                        }
                    }
                    div { class: "grow" }
                    button { class: "btn primary", onclick: move |_| new_event(&s_new, ui), Icon { d: icons::PLUS } " New event" }
                }
                match view {
                    CalView::Agenda => rsx! { AgendaView { range } },
                    CalView::Week => rsx! { week::WeekView { anchor } },
                    CalView::Month => rsx! { month::MonthView { anchor } },
                }
            }
            div { class: "cal-side",
                if let Some(e) = selected.clone() {
                    editor::EventEditor { key: "{e.id}", event: e.clone(), autofocus: JUST_CREATED() == Some(e.id) }
                }
                tzpanel::TzPanel { event: selected }
            }
        }
    }
}

#[component]
fn AgendaView(range: i64) -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let to = today + Duration::days(range - 1);
    let list = store.read(|db| events::between(db, today, to));
    let days = by_day(&list, today, to);
    rsx! {
        div { class: "agenda", "data-list": "",
            for (day, evs) in days {
                DayBlock { key: "{day}", day, events: evs }
            }
        }
    }
}

/// Same day-of-month `n` months away (clamped to the month's last day).
pub(crate) fn add_months(d: NaiveDate, n: i64) -> NaiveDate {
    let total = d.year() as i64 * 12 + d.month0() as i64 + n;
    let (y, m) = ((total.div_euclid(12)) as i32, (total.rem_euclid(12) + 1) as u32);
    let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
    let last = cc_core::seed::days_in_month(first);
    NaiveDate::from_ymd_opt(y, m, d.day().min(last)).unwrap()
}

#[component]
fn DayBlock(day: NaiveDate, events: Vec<Event>) -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    let weekend = matches!(chrono::Datelike::weekday(&day), chrono::Weekday::Sat | chrono::Weekday::Sun);
    rsx! {
        div { class: if weekend { "ag-day weekend" } else { "ag-day" },
            div { class: "ag-head",
                span { "{heading(day, today)}" }
                span { class: "muted", "{day.format(\"%b %-d\")}" }
                div { class: "grow" }
                button { class: "icon-btn ag-add", title: "Add event on this day", onclick: move |_| create_on(&store, ui, day), Icon { d: icons::PLUS } }
            }
            if events.is_empty() { div { class: "ag-empty" } }
            for e in events { EventRow { key: "{e.id}", event: e, compact: false } }
        }
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::CALENDAR, "Upcoming events", Action::Open(View::Widget("events"))).keywords("calendar agenda").shortcut("Ctrl+2"),
        Command::new("Create", icons::PLUS, "New event", Action::Run(new_event)).keywords("calendar meeting"),
    ]
}

fn search(db: &cc_core::Db, q: &str) -> Vec<Hit> {
    let today = cc_core::time::today();
    let h12 = db.setting("clock").as_deref() != Some("24");
    events::search(db, q, today)
        .into_iter()
        .map(|e| Hit { id: e.id, detail: format!("{} · {}", day_label(e.start_date(), today), e.time_label(h12)), label: e.title })
        .collect()
}

fn open_item(id: i64) {
    *SELECTED.write() = Some(id);
}

/// "Today", "Tomorrow", otherwise the weekday ("Friday"); the date is shown beside it.
fn heading(day: NaiveDate, today: NaiveDate) -> String {
    match (day - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        _ => day.format("%A").to_string(),
    }
}

/// Inline CSS for an event's colour (`--ec`), empty for the default accent.
pub(crate) fn color_var(e: &Event) -> String {
    e.color.as_deref().map(|c| format!("--ec: {c};")).unwrap_or_default()
}

/// Colour swatches for one event: "Default" (the theme accent) plus the shared palette.
#[component]
pub(crate) fn EventColors(id: i64, color: Option<String>) -> Element {
    let store = use_store();
    let set = move |c: Option<&'static str>| {
        let _ = store.write(|db| events::set_color(db, id, c));
    };
    let set2 = set.clone();
    rsx! {
        div { class: "swatches",
            button {
                class: if color.is_none() { "swatch default on" } else { "swatch default" },
                title: "Default",
                onclick: move |_| set2(None),
            }
            for c in cc_core::model::projects::COLORS {
                {
                    let set = set.clone();
                    rsx! {
                        button {
                            key: "{c}",
                            class: if color.as_deref() == Some(c) { "swatch on" } else { "swatch" },
                            style: "background: {c}",
                            title: "{c}",
                            onclick: move |_| set(Some(c)),
                        }
                    }
                }
            }
        }
    }
}

/// Delete a manual event, with an undo toast. Returns whether anything was deleted.
pub(crate) fn delete_with_undo(store: &crate::store::Store, ui: crate::state::Ui, id: i64) -> bool {
    let Ok(Some(ev)) = store.write(|db| events::delete(db, id)) else { return false };
    if SELECTED.peek().as_ref() == Some(&id) {
        *SELECTED.write() = None;
    }
    let label = ev.title.clone();
    ui.notify_undo(format!("Deleted “{label}”"), move |s| {
        if let Err(e) = s.write(|db| events::restore(db, &ev)) {
            ui.error(format!("Could not restore: {e}"));
            return;
        }
        *SELECTED.write() = Some(ev.id);
    });
    true
}

/// Check an event off (adds it to today's completed tasks) or back on, with Undo.
pub(crate) fn toggle_done(store: &Store, ui: Ui, id: i64, done: bool) {
    let title = store.peek(|db| events::get(db, id)).map(|e| e.title).unwrap_or_default();
    if done {
        if store.write(|db| events::mark_undone(db, id)).is_ok() {
            ui.notify_undo(format!("“{title}” is no longer done"), move |s| {
                let today = cc_core::time::today();
                let _ = s.write(|db| events::mark_done(db, id, today));
            });
        }
    } else {
        let today = *ui.today.peek();
        if store.write(|db| events::mark_done(db, id, today)).is_ok() {
            ui.notify_undo(format!("Checked off “{title}”"), move |s| {
                let _ = s.write(|db| events::mark_undone(db, id));
            });
        }
    }
}
