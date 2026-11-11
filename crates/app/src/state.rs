//! App-wide UI state and the actions the command palette / shortcuts can trigger.

use crate::store::Store;
use chrono::NaiveDate;
use dioxus::prelude::*;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Overview,
    Widget(&'static str),
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
    pub fn parse(s: Option<&str>) -> Self {
        if s == Some("light") { Theme::Light } else { Theme::Dark }
    }
}

#[derive(Clone)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub id: u64,
    /// Offered as an "Undo" button (and Ctrl+Z) while the toast is visible.
    pub undo: Option<Rc<dyn Fn(&Store)>>,
}

impl PartialEq for Toast {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Copyable bundle of signals, provided as context at the root.
#[derive(Clone, Copy)]
pub struct Ui {
    pub view: Signal<View>,
    pub theme: Signal<Theme>,
    pub today: Signal<NaiveDate>,
    /// 12-hour clock (5:00 PM) instead of 24-hour (17:00).
    pub h12: Signal<bool>,
    /// Minutes since the epoch, refreshed every ~30 s (for "now" displays).
    pub clock: Signal<i64>,
    /// Open right-click time menu: (unix timestamp, x, y).
    pub time_menu: Signal<Option<(i64, f64, f64, Option<i64>)>>,
    pub palette: Signal<bool>,
    /// `Some(prefill)` while the quick-add box is open.
    pub quick_add: Signal<Option<String>>,
    /// True when quick-add was opened by the global hotkey: closing it re-minimizes the window.
    pub summoned: Signal<bool>,
    pub toast: Signal<Option<Toast>>,
}

impl Ui {
    fn show(mut self, text: String, error: bool, undo: Option<Rc<dyn Fn(&Store)>>) {
        let id = self.toast.peek().as_ref().map_or(0, |t| t.id + 1);
        self.toast.set(Some(Toast { text, error, id, undo }));
    }
    pub fn notify(self, text: impl Into<String>) {
        self.show(text.into(), false, None);
    }
    pub fn error(self, text: impl Into<String>) {
        self.show(text.into(), true, None);
    }
    pub fn notify_undo(self, text: impl Into<String>, undo: impl Fn(&Store) + 'static) {
        self.show(text.into(), false, Some(Rc::new(undo)));
    }
    /// Run the pending undo action, if the toast offering it is still up.
    pub fn undo(mut self, store: &Store) {
        let pending = self.toast.peek().as_ref().and_then(|t| t.undo.clone());
        if let Some(f) = pending {
            self.toast.set(None);
            f(store);
        }
    }
    pub fn open(mut self, view: View) {
        self.view.set(view);
    }
    pub fn open_quick_add(mut self, prefill: &str) {
        self.palette.set(false);
        self.quick_add.set(Some(prefill.to_string()));
    }
}

pub fn use_ui() -> Ui {
    use_context::<Ui>()
}

/// What a palette entry does. Widgets can add their own through `Action::Run`.
#[derive(Clone, Copy)]
pub enum Action {
    Open(View),
    QuickAdd(&'static str),
    ToggleTheme,
    Run(fn(&Store, Ui)),
    /// Open a widget's full view focused on one of its items (from palette search).
    OpenItem(&'static str, i64),
}

#[derive(Clone)]
pub struct Command {
    pub label: String,
    pub group: &'static str,
    pub icon: &'static str,
    /// Extra words the fuzzy search should match on.
    pub keywords: &'static str,
    pub shortcut: Option<&'static str>,
    pub action: Action,
}

impl Command {
    pub fn new(group: &'static str, icon: &'static str, label: impl Into<String>, action: Action) -> Self {
        Self { label: label.into(), group, icon, keywords: "", shortcut: None, action }
    }
    pub fn keywords(mut self, k: &'static str) -> Self {
        self.keywords = k;
        self
    }
    pub fn shortcut(mut self, s: &'static str) -> Self {
        self.shortcut = Some(s);
        self
    }
}

pub fn run_action(action: Action, store: &Store, mut ui: Ui) {
    match action {
        Action::Open(v) => {
            ui.palette.set(false);
            ui.open(v);
        }
        Action::QuickAdd(prefill) => ui.open_quick_add(prefill),
        Action::ToggleTheme => {
            ui.palette.set(false);
            let next = if *ui.theme.peek() == Theme::Dark { Theme::Light } else { Theme::Dark };
            ui.theme.set(next);
            store.set_setting("theme", next.as_str());
        }
        Action::Run(f) => {
            ui.palette.set(false);
            f(store, ui);
        }
        Action::OpenItem(widget, id) => {
            ui.palette.set(false);
            if let Some(f) = crate::widgets::get(widget).and_then(|w| w.open_item) {
                f(id);
            }
            ui.open(View::Widget(widget));
        }
    }
}
