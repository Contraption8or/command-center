//! App frame: side rail, top bar, overview grid, full views and overlays.

mod keys;

pub(crate) use keys::open_url;
mod layout_editor;
mod palette;
mod quick_add;
mod settings;
mod time_menu;

use crate::icons::{self, Icon};
use crate::state::{use_ui, Command, Action, Theme, Ui, View};
use crate::store::use_store;
use crate::widgets::{self, REGISTRY};
use dioxus::prelude::*;

#[component]
pub fn Shell() -> Element {
    let store = use_store();
    let ui = use_context_provider(|| {
        let theme = Theme::parse(store.setting("theme").as_deref());
        Ui {
            view: Signal::new(View::Overview),
            theme: Signal::new(theme),
            today: Signal::new(cc_core::time::today()),
            h12: Signal::new(store.setting("clock").as_deref() != Some("24")),
            clock: Signal::new(chrono::Utc::now().timestamp() / 60),
            time_menu: Signal::new(None),
            palette: Signal::new(false),
            quick_add: Signal::new(None),
            summoned: Signal::new(false),
            toast: Signal::new(None),
        }
    });
    keys::use_key_bridge();
    keys::use_global_quick_add();
    crate::window_state::use_window_persistence();
    crate::window_state::use_ui_scale();
    crate::window_state::use_themed_titlebar();
    crate::updates::use_startup_check();

    let theme = ui.theme.read().as_str();
    rsx! {
        div { class: "app", "data-theme": theme,
            Rail {}
            div { class: "main",
                TopBar {}
                crate::updates::UpdateBanner {}
                div { class: "content",
                    match *ui.view.read() {
                        View::Overview => rsx! { Overview {} },
                        View::Widget(id) => rsx! { FullHost { key: "{id}", id } },
                        View::Settings => rsx! { settings::Settings {} },
                    }
                }
            }
            if *ui.palette.read() { palette::Palette {} }
            if let Some(prefill) = ui.quick_add.read().clone() { quick_add::QuickAdd { prefill } }
            if let Some((ts, x, y)) = *ui.time_menu.read() { time_menu::TimeMenu { ts, x, y } }
            if let Some(t) = ui.toast.read().clone() {
                div {
                    key: "{t.id}",
                    class: if t.error { "toast error" } else if t.undo.is_some() { "toast with-action" } else { "toast" },
                    // Drop the toast (and its undo) once the fade-out finishes.
                    onanimationend: move |_| {
                        let mut toast = ui.toast;
                        if toast.peek().as_ref().is_some_and(|cur| cur.id == t.id) {
                            toast.set(None);
                        }
                    },
                    span { "{t.text}" }
                    if t.undo.is_some() {
                        button { class: "toast-btn", onclick: move |_| ui.undo(&store), "Undo" kbd { "Ctrl Z" } }
                    }
                }
            }
        }
    }
}

#[component]
fn Rail() -> Element {
    let ui = use_ui();
    let view = *ui.view.read();
    rsx! {
        nav { class: "rail",
            div { class: "logo" }
            RailButton { icon: icons::OVERVIEW, tip: "Overview  Ctrl+0", active: view == View::Overview, to: View::Overview }
            for (i, w) in REGISTRY.iter().enumerate() {
                RailButton {
                    key: "{w.id}",
                    icon: w.icon,
                    tip: format!("{}  Ctrl+{}", w.title, i + 1),
                    active: view == View::Widget(w.id),
                    to: View::Widget(w.id),
                }
            }
            div { class: "spacer" }
            RailButton { icon: icons::SETTINGS, tip: "Settings  Ctrl+,", active: view == View::Settings, to: View::Settings }
        }
    }
}

#[component]
fn RailButton(icon: &'static str, tip: String, active: bool, to: View) -> Element {
    let ui = use_ui();
    rsx! {
        button {
            class: if active { "rail-btn active" } else { "rail-btn" },
            onclick: move |_| ui.open(to),
            Icon { d: icon }
            span { class: "tip", "{tip}" }
        }
    }
}

#[component]
fn TopBar() -> Element {
    let mut ui = use_ui();
    let view = *ui.view.read();
    let title = match view {
        View::Overview => "Overview",
        View::Settings => "Settings",
        View::Widget(id) => widgets::get(id).map_or("", |w| w.title),
    };
    let date = ui.today.read().format("%A, %B %-d").to_string();
    rsx! {
        header { class: "topbar",
            if view != View::Overview {
                button { class: "back-btn", onclick: move |_| ui.open(View::Overview), Icon { d: icons::ARROW_LEFT } }
            }
            h1 { "{title}" }
            span { class: "date", "{date}" }
            div { class: "grow" }
            button { class: "search-btn", onclick: move |_| ui.palette.set(true),
                Icon { d: icons::SEARCH }
                span { "Search or create" }
                kbd { "Ctrl K" }
            }
        }
    }
}

#[component]
fn Overview() -> Element {
    let store = use_store();
    let slots = store.read(crate::layout::load);
    rsx! {
        div { class: "grid",
            for s in slots.into_iter().filter(|s| s.visible) {
                if let Some(def) = widgets::get(&s.id) {
                    CardHost { key: "{def.id}", id: def.id, cols: s.cols, rows: s.rows }
                }
            }
        }
    }
}

/// Draws the card frame for a widget. Keyed by id, so each host always calls the same
/// widget function and its hooks stay stable.
#[component]
fn CardHost(id: &'static str, cols: u8, rows: u8) -> Element {
    let ui = use_ui();
    let def = widgets::get(id).expect("registered widget");
    rsx! {
        section {
            class: "card",
            style: "grid-column: span {cols}; grid-row: span {rows};",
            div { class: "card-head",
                h2 { "{def.title}" }
                div { class: "grow" }
                button { class: "open", title: "Open", onclick: move |_| ui.open(View::Widget(id)), Icon { d: icons::EXPAND } }
            }
            div { class: "card-body", {(def.card)()} }
        }
    }
}

#[component]
fn FullHost(id: &'static str) -> Element {
    match widgets::get(id) {
        Some(def) => (def.full)(),
        None => rsx! {},
    }
}

/// Palette entries owned by the shell itself.
pub fn shell_commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::OVERVIEW, "Overview", Action::Open(View::Overview)).shortcut("Ctrl+0").keywords("home dashboard"),
        Command::new("Go to", icons::SETTINGS, "Settings", Action::Open(View::Settings)).shortcut("Ctrl+,").keywords("preferences"),
        Command::new("App", icons::OVERVIEW, "Customize dashboard", Action::Open(View::Settings)).keywords("layout cards hide reorder resize widgets"),
        Command::new("Create", icons::PLUS, "Quick add", Action::QuickAdd("")).shortcut("Ctrl+J"),
        Command::new("App", icons::SUN, "Toggle light / dark", Action::ToggleTheme).shortcut("Ctrl+Shift+L").keywords("theme mode"),
        Command::new("Data", icons::DOWNLOAD, "Export backup…", Action::Run(settings::export_backup)).keywords("save file"),
        Command::new("Data", icons::UPLOAD, "Restore from backup…", Action::Run(settings::restore_backup)).keywords("import load"),
        Command::new("Data", icons::PROJECTS, "Open data folder", Action::Run(settings::open_data_folder)).keywords("location explorer"),
        Command::new("Data", icons::TRASH, "Clear sample data…", Action::Run(settings::clear_sample)).keywords("seed demo remove"),
        Command::new("Data", icons::REFRESH, "Reload sample data…", Action::Run(settings::reload_sample)).keywords("seed demo"),
    ]
}

pub fn all_commands() -> Vec<Command> {
    let mut out = shell_commands();
    for w in REGISTRY {
        out.extend((w.commands)());
    }
    out
}

