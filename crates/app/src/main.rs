// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod layout;
mod shell;
mod state;
mod store;
mod updates;
mod widgets;
mod window_state;

use cc_core::{seed, Db};
use dioxus::desktop::Config;
use dioxus::prelude::*;
use std::cell::RefCell;

const CSS: &str = include_str!("../assets/style.css");
const CAL_CSS: &str = include_str!("../assets/calendar.css");
const AI_CSS: &str = include_str!("../assets/assistant.css");

thread_local! {
    static BOOT_DB: RefCell<Option<Db>> = const { RefCell::new(None) };
}

fn main() {
    let mut db = match Db::open_default() {
        Ok(db) => db,
        Err(e) => fatal(&format!("Could not open the database.\n\n{e:#}")),
    };
    let today = cc_core::time::today();
    if let Err(e) = seed::ensure(&mut db, today) {
        eprintln!("sample data not inserted: {e:#}");
    }
    if let Err(e) = cc_core::model::tasks::carry_over(&mut db, today) {
        eprintln!("carry-over failed: {e:#}");
    }
    if let Err(e) = cc_core::backup::auto_backup(&db, &cc_core::backup::backup_dir(&db), today) {
        eprintln!("automatic backup failed: {e:#}");
    }
    let webview_dir = db.path().parent().map(|p| p.join("webview")).unwrap_or_default();
    let window = window_state::builder(&db);
    // Paint the window in the saved theme's background so there is no flash on start.
    let bg = if db.setting("theme").as_deref() == Some("light") { (244, 245, 248, 255) } else { (14, 16, 20, 255) };
    BOOT_DB.with(|cell| *cell.borrow_mut() = Some(db));
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color(bg)
        .with_data_directory(webview_dir)
        .with_disable_context_menu(!cfg!(debug_assertions))
        // The native file-drop handler swallows HTML drag-and-drop (used to reorder lists).
        .with_disable_drag_drop_handler(true)
        .with_custom_head(format!("<style>{CSS}
{CAL_CSS}
{AI_CSS}</style>"));

    dioxus::LaunchBuilder::desktop().with_cfg(config).launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| {
        let db = BOOT_DB.with(|cell| cell.borrow_mut().take()).expect("database opened in main");
        store::Store::new(db)
    });
    rsx! { shell::Shell {} }
}

fn fatal(msg: &str) -> ! {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Command Center")
        .set_description(msg)
        .show();
    std::process::exit(1);
}
