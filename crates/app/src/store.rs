//! UI-side handle to the database.
//!
//! Reads are synchronous and run during render — local SQLite answers in microseconds, so
//! there is never a loading state. `read` subscribes the calling component to a revision
//! signal; `write` bumps it, so every component that read data re-renders after a change.

use cc_core::Db;
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub struct Store {
    db: Rc<RefCell<Db>>,
    rev: Signal<u64>,
}

impl Store {
    pub fn new(db: Db) -> Self {
        Self { db: Rc::new(RefCell::new(db)), rev: Signal::new(0) }
    }

    /// Read from the database and subscribe the current component to future writes.
    pub fn read<R>(&self, f: impl FnOnce(&Db) -> R) -> R {
        let _ = (self.rev)();
        f(&self.db.borrow())
    }

    /// Read without subscribing (event handlers, one-off lookups).
    pub fn peek<R>(&self, f: impl FnOnce(&Db) -> R) -> R {
        f(&self.db.borrow())
    }

    /// Mutate, then notify every subscribed component.
    pub fn write<R>(&self, f: impl FnOnce(&mut Db) -> anyhow::Result<R>) -> anyhow::Result<R> {
        let out = f(&mut self.db.borrow_mut());
        let mut rev = self.rev;
        rev += 1;
        out
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.peek(|db| db.setting(key))
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let _ = self.write(|db| db.set_setting(key, value));
    }
}

pub fn use_store() -> Store {
    use_context::<Store>()
}
