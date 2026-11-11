//! "Is there a newer version?" on startup (unless turned off), plus "Check now" in Settings.

use crate::icons::{self, Icon};
use crate::state::{use_ui, Ui};
use crate::store::{use_store, Store};
use cc_core::update::{self, Release};
use dioxus::prelude::*;
use futures_util::StreamExt;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version compared against GitHub. Debug builds can pretend to be older with
/// `COMMAND_CENTER_VERSION_OVERRIDE` (to test the banner); release builds can't.
fn current_version() -> String {
    if cfg!(debug_assertions) {
        if let Ok(v) = std::env::var("COMMAND_CENTER_VERSION_OVERRIDE") {
            return v;
        }
    }
    VERSION.to_string()
}
const KEY_ENABLED: &str = "updates.check";
const KEY_SKIP: &str = "updates.skip";

/// A newer release waiting to be shown in the banner.
pub static AVAILABLE: GlobalSignal<Option<Release>> = Signal::global(|| None);
static CHECKING: GlobalSignal<bool> = Signal::global(|| false);

pub fn enabled(store: &Store) -> bool {
    store.setting(KEY_ENABLED).as_deref() != Some("0")
}

/// Run the check off the UI thread. `manual` = from the Settings button: reports "up to date"
/// and errors, and ignores "skip this version".
pub fn check(store: Store, ui: Ui, manual: bool) {
    if *CHECKING.peek() {
        return;
    }
    *CHECKING.write() = true;
    spawn(async move {
        let (tx, mut rx) = futures_channel::mpsc::unbounded();
        std::thread::spawn(move || {
            let _ = tx.unbounded_send(update::fetch_latest());
        });
        let result = rx.next().await;
        *CHECKING.write() = false;
        match result {
            Some(Ok(Some(r))) if update::is_newer(&current_version(), &r.tag) => {
                let skipped = store.setting(KEY_SKIP).as_deref() == Some(r.tag.as_str());
                if manual || !skipped {
                    *AVAILABLE.write() = Some(r);
                }
            }
            Some(Ok(_)) => {
                if manual {
                    ui.notify(format!("You're up to date (version {VERSION})"));
                }
            }
            Some(Err(e)) => {
                if manual {
                    ui.error(format!("Couldn't check for updates: {e}"));
                }
            }
            None => {}
        }
    });
}

/// Check once at startup.
pub fn use_startup_check() {
    let store = use_store();
    let ui = use_ui();
    use_hook(move || {
        if enabled(&store) {
            check(store.clone(), ui, false);
        }
    });
}

#[component]
pub fn UpdateBanner() -> Element {
    let store = use_store();
    let ui = use_ui();
    let Some(r) = AVAILABLE() else { return rsx! {} };
    let (page, page2) = (r.page_url.clone(), r.page_url.clone());
    let download = r.exe_url.clone().unwrap_or_else(|| r.page_url.clone());
    let tag = r.tag.clone();
    rsx! {
        div { class: "update-banner",
            Icon { d: icons::DOWNLOAD }
            span { "Command Center " strong { "{r.tag}" } " is available" span { class: "muted", " · you have v{current_version()}" } }
            div { class: "grow" }
            button { class: "btn", onclick: move |_| crate::shell::open_url(&page, ui), "What's new" }
            button { class: "btn primary", onclick: move |_| { crate::shell::open_url(&download, ui); let _ = &page2; }, "Download" }
            button { class: "icon-btn", title: "Skip this version",
                onclick: move |_| { store.set_setting(KEY_SKIP, &tag); *AVAILABLE.write() = None; },
                Icon { d: icons::CLOSE } }
        }
    }
}

/// Settings row: version, startup check toggle, "Check now".
#[component]
pub fn UpdateSettings() -> Element {
    let store = use_store();
    let ui = use_ui();
    let on = store.read(|db| db.setting(KEY_ENABLED)).as_deref() != Some("0");
    let (s1, s2, s3) = (store.clone(), store.clone(), store.clone());
    rsx! {
        div { class: "row",
            span { class: "label", "Updates" }
            div { class: "value",
                span { class: "muted", "Version {VERSION}" }
                div { class: "seg",
                    button { class: if on { "on" }, onclick: move |_| s1.set_setting(KEY_ENABLED, "1"), "Check on startup" }
                    button { class: if !on { "on" }, onclick: move |_| s2.set_setting(KEY_ENABLED, "0"), "Off" }
                }
                button { class: "btn", disabled: CHECKING(), onclick: move |_| check(s3.clone(), ui, true),
                    if CHECKING() { "Checking…" } else { "Check now" } }
            }
        }
    }
}
