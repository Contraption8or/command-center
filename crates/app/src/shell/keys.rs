//! Keyboard: in-app shortcuts (via a document-level listener in the webview, so they work
//! no matter which element has focus) and the system-wide quick-add hotkey.

use crate::state::{run_action, use_ui, Action, View};
use crate::store::use_store;
use crate::widgets::REGISTRY;
use dioxus::desktop::{use_window, HotKeyState, ShortcutHandle};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc, str::FromStr};

pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+Space";

const KEY_JS: &str = r#"
const send = (m) => dioxus.send(m);
document.addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (mod && !e.altKey && !e.shiftKey && k === 'k') { e.preventDefault(); send('palette'); return; }
  if (mod && !e.altKey && !e.shiftKey && k === 'j') { e.preventDefault(); send('quick'); return; }
  if (mod && !e.altKey && !e.shiftKey && k === 'n') { e.preventDefault(); send('new-note'); return; }
  if (mod && !e.altKey && !e.shiftKey && k === 'f') {
    e.preventDefault();
    const f = document.querySelector('[data-find]');
    if (f) { f.focus(); f.select(); }
    return;
  }
  if (mod && !e.altKey && e.key === ',') { e.preventDefault(); send('settings'); return; }
  if (mod && e.shiftKey && k === 'l') { e.preventDefault(); send('theme'); return; }
  if (mod && !e.altKey && !e.shiftKey && /^[0-9]$/.test(e.key)) { e.preventDefault(); send('go:' + e.key); return; }
  if (e.key === 'F5' || (mod && k === 'r') || (mod && k === 'p')) { e.preventDefault(); return; }
  const a = document.activeElement;
  const typing = a && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName);
  if (mod && !e.shiftKey && k === 'z' && !typing) { e.preventDefault(); send('undo'); return; }
  // Arrow keys walk between rows of the same list (rows carry data-row).
  if (!mod && !e.altKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp') && a && a.dataset.row !== undefined) {
    e.preventDefault();
    const rows = [...a.closest('[data-list]').querySelectorAll('[data-row]')];
    const i = rows.indexOf(a) + (e.key === 'ArrowDown' ? 1 : -1);
    if (rows[i]) rows[i].focus();
    return;
  }
  if (e.key === 'Escape') {
    const a = document.activeElement;
    if (a && a.closest('[data-esc-local]')) return;            // component handles its own Esc
    if (a && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName) && !a.closest('.palette')) { a.blur(); return; }
    send('esc');
  }
}, true);
// Right-click on any time (elements with data-ts) opens the copy-for-Discord menu.
document.addEventListener('contextmenu', (e) => {
  const el = e.target.closest('[data-ts]');
  if (!el || e.target.closest('input, textarea')) return;
  e.preventDefault();
  send('tsmenu:' + el.dataset.ts + ':' + e.clientX + ':' + e.clientY);
}, true);
// Clicks inside rendered Markdown: links open in the default browser, checkboxes toggle
// the note's source (handled by the notes widget).
document.addEventListener('click', (e) => {
  const box = e.target.closest('.md input[data-task]');
  if (box) {
    e.preventDefault();
    send('notes:task:' + box.closest('[data-note]').dataset.note + ':' + box.dataset.task);
    return;
  }
  const link = e.target.closest('a[href]');
  if (link) {
    e.preventDefault();
    send('open-url:' + link.getAttribute('href'));
  }
}, true);
// Allow drops anywhere in-page, and never let a dropped file navigate the window away.
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => e.preventDefault());
window.addEventListener('focus', () => send('tick'));
setInterval(() => send('tick'), 30000);
await new Promise(() => {});
"#;

pub fn use_key_bridge() {
    let store = use_store();
    let mut ui = use_ui();
    let win = use_window();
    use_future(move || {
        let store = store.clone();
        let win = win.clone();
        async move {
            let mut bridge = document::eval(KEY_JS);
            while let Ok(msg) = bridge.recv::<String>().await {
                match msg.as_str() {
                    "palette" => {
                        let open = !*ui.palette.peek();
                        ui.quick_add.set(None);
                        ui.palette.set(open);
                    }
                    "quick" => ui.open_quick_add(""),
                    "settings" => run_action(Action::Open(View::Settings), &store, ui),
                    "theme" => run_action(Action::ToggleTheme, &store, ui),
                    "undo" => ui.undo(&store),
                    m if m.starts_with("tsmenu:") => {
                        let p: Vec<&str> = m.split(':').collect();
                        if let [_, ts, x, y] = p.as_slice() {
                            if let (Ok(ts), Ok(x), Ok(y)) = (ts.parse(), x.parse(), y.parse()) {
                                ui.time_menu.set(Some((ts, x, y)));
                            }
                        }
                    }
                    "new-note" => crate::widgets::notes::new_note(&store, ui),
                    m if m.starts_with("open-url:") => open_url(&m["open-url:".len()..], ui),
                    "esc" => {
                        if ui.time_menu.peek().is_some() {
                            ui.time_menu.set(None);
                        } else if ui.quick_add.peek().is_some() {
                            ui.quick_add.set(None);
                            if *ui.summoned.peek() {
                                ui.summoned.set(false);
                                win.set_minimized(true);
                            }
                        } else if *ui.palette.peek() {
                            ui.palette.set(false);
                        } else if *ui.view.peek() != View::Overview {
                            ui.view.set(View::Overview);
                        }
                    }
                    "tick" => {
                        let minute = chrono::Utc::now().timestamp() / 60;
                        if *ui.clock.peek() != minute {
                            ui.clock.set(minute);
                        }
                        let now = cc_core::time::today();
                        if *ui.today.peek() != now {
                            // New day: roll unfinished tasks forward, then re-render.
                            let _ = store.write(|db| cc_core::model::tasks::carry_over(db, now));
                            let _ = store.peek(|db| cc_core::backup::auto_backup(db, &cc_core::backup::backup_dir(db), now));
                            ui.today.set(now);
                        }
                    }
                    m if m.starts_with("go:") => {
                        let n: usize = m[3..].parse().unwrap_or(0);
                        let target = if n == 0 { Some(View::Overview) } else { REGISTRY.get(n - 1).map(|w| View::Widget(w.id)) };
                        if let Some(v) = target {
                            ui.palette.set(false);
                            ui.quick_add.set(None);
                            ui.view.set(v);
                        }
                    }
                    // "<widget id>:<payload>" → that widget's on_message handler.
                    m => {
                        if let Some((wid, payload)) = m.split_once(':') {
                            if let Some(f) = crate::widgets::get(wid).and_then(|w| w.on_message) {
                                f(&store, ui, payload);
                            }
                        }
                    }
                }
            }
        }
    });
}

/// Open a link from a note in the default browser. Only web and mail links are followed.
pub(crate) fn open_url(url: &str, ui: crate::state::Ui) {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")) {
        ui.error("Only http, https and mailto links can be opened");
        return;
    }
    // url.dll's handler takes the URL as a single argument: no shell parsing involved.
    if let Err(e) = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn() {
        ui.error(format!("Could not open link: {e}"));
    }
}

/// Current hotkey text plus the last registration error, shared with the settings view.
#[derive(Clone, Copy)]
pub struct HotkeyState {
    pub accel: Signal<String>,
    pub error: Signal<Option<String>>,
}

pub fn use_global_quick_add() {
    let store = use_store();
    let mut ui = use_ui();
    let win = use_window();
    let state = use_context_provider(|| HotkeyState {
        accel: Signal::new(store.setting("hotkey.quick_add").unwrap_or_else(|| DEFAULT_HOTKEY.to_string())),
        error: Signal::new(None),
    });
    let handle: Rc<RefCell<Option<ShortcutHandle>>> = use_hook(|| Rc::new(RefCell::new(None)));

    use_effect(move || {
        let accel = state.accel.read().clone();
        let mut error = state.error;
        if let Some(old) = handle.borrow_mut().take() {
            win.remove_shortcut(old);
        }
        let hotkey = match global_hotkey::hotkey::HotKey::from_str(&accel) {
            Ok(h) => h,
            Err(e) => {
                let _ = e;
                error.set(Some("Not a valid shortcut".to_string()));
                return;
            }
        };
        let w = win.clone();
        let result = win.create_shortcut(hotkey, move |s| {
            if s == HotKeyState::Pressed {
                w.set_visible(true);
                w.set_minimized(false);
                w.set_focus();
                ui.summoned.set(true);
                ui.open_quick_add("");
            }
        });
        match result {
            Ok(h) => {
                *handle.borrow_mut() = Some(h);
                error.set(None);
            }
            Err(_) => error.set(Some(format!("{accel} is already used by another app"))),
        }
    });
    let _ = &mut ui;
}
