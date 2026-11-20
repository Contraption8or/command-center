//! Runs the reminder check every 20 seconds while the app is open and shows what's due
//! as desktop notifications (see `cc_core::reminders` for what fires when, and the wording).

use crate::state::{use_ui, Ui};
use crate::store::{use_store, Store};
use dioxus::prelude::*;
use futures_util::StreamExt;

pub const ENABLED_KEY: &str = "reminders.enabled";

pub fn enabled(store: &Store) -> bool {
    store.setting(ENABLED_KEY).as_deref() != Some("0")
}

pub fn use_reminders() {
    let store = use_store();
    let ui = use_ui();
    use_future(move || {
        let store = store.clone();
        async move {
            // A plain thread ticks the loop, so it keeps running when the window is hidden
            // (the webview's own timers get throttled then).
            let (tx, mut rx) = futures_channel::mpsc::unbounded::<()>();
            std::thread::spawn(move || loop {
                if tx.unbounded_send(()).is_err() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(20));
            });
            let mut warned = false;
            while rx.next().await.is_some() {
                if enabled(&store) {
                    check(&store, ui, &mut warned);
                }
            }
        }
    });
}

fn check(store: &Store, ui: Ui, warned: &mut bool) {
    let now = chrono::Local::now().naive_local();
    let h12 = *ui.h12.peek();
    for r in store.peek(|db| cc_core::reminders::due(db, now, h12)) {
        // Also inside the app and on the taskbar, so a reminder isn't missed when Windows
        // notifications are off or the toast is dismissed.
        ui.notify(format!("{} — {}", r.title, r.body));
        crate::notify::flash_taskbar();
        let _ = store.peek(|db| cc_core::reminders::mark_sent(db, &r.key));
        match crate::notify::show(&r.title, &r.body) {
            Ok(()) => {}
            Err(e) => {
                // Say so once per run instead of every 20 seconds.
                if !*warned {
                    *warned = true;
                    ui.error(format!("Couldn't show a reminder: {e}"));
                }
            }
        }
    }
}
