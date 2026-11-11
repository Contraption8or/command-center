//! Overview layout: card order, size and visibility, stored in settings as JSON.
//! Widgets not mentioned in the saved layout (e.g. newly added ones) are appended with
//! their default size; ids that no longer exist are dropped.

use crate::widgets::REGISTRY;
use cc_core::Db;
use serde::{Deserialize, Serialize};

const KEY: &str = "layout.overview";
/// Widgets that start hidden on the overview (show them in Settings → Dashboard).
const HIDDEN_BY_DEFAULT: &[&str] = &["assistant"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub id: String,
    pub visible: bool,
    /// Width on the 12-column grid.
    pub cols: u8,
    /// Height in grid rows.
    pub rows: u8,
}

/// Width presets offered in Settings.
pub const WIDTHS: [(u8, &str); 4] = [(4, "Small"), (6, "Medium"), (8, "Large"), (12, "Full")];
pub const HEIGHTS: [(u8, &str); 3] = [(1, "Short"), (2, "Normal"), (3, "Tall")];

pub fn defaults() -> Vec<Slot> {
    REGISTRY.iter().map(|w| Slot { id: w.id.into(), visible: !HIDDEN_BY_DEFAULT.contains(&w.id), cols: w.cols, rows: w.rows }).collect()
}

pub fn load(db: &Db) -> Vec<Slot> {
    let saved: Vec<Slot> = db.setting(KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    merge(saved)
}

fn merge(saved: Vec<Slot>) -> Vec<Slot> {
    let mut out: Vec<Slot> = saved
        .into_iter()
        .filter(|s| REGISTRY.iter().any(|w| w.id == s.id))
        .map(|mut s| {
            s.cols = s.cols.clamp(3, 12);
            s.rows = s.rows.clamp(1, 4);
            s
        })
        .collect();
    for d in defaults() {
        if !out.iter().any(|s| s.id == d.id) {
            out.push(d);
        }
    }
    out
}

pub fn save(db: &Db, slots: &[Slot]) -> anyhow::Result<()> {
    db.set_setting(KEY, &serde_json::to_string(slots)?)
}

pub fn reset(db: &Db) -> anyhow::Result<()> {
    db.delete_setting(KEY)
}

/// Move `id` next to `target` (before or after it). Returns false if nothing changed.
pub fn move_slot(slots: &mut Vec<Slot>, id: &str, target: &str, after: bool) -> bool {
    if id == target {
        return false;
    }
    let Some(from) = slots.iter().position(|s| s.id == id) else { return false };
    let before = slots.clone();
    let slot = slots.remove(from);
    let Some(t) = slots.iter().position(|s| s.id == target) else {
        *slots = before;
        return false;
    };
    slots.insert(if after { t + 1 } else { t }, slot);
    *slots != before
}

/// Set a card's size, clamped to the grid. Returns false if nothing changed.
pub fn resize(slots: &mut [Slot], id: &str, cols: u8, rows: u8) -> bool {
    match slots.iter_mut().find(|s| s.id == id) {
        Some(s) => {
            let (c, r) = (cols.clamp(3, 12), rows.clamp(1, 4));
            let changed = (s.cols, s.rows) != (c, r);
            s.cols = c;
            s.rows = r;
            changed
        }
        None => false,
    }
}

pub fn set_visible(slots: &mut [Slot], id: &str, visible: bool) -> bool {
    match slots.iter_mut().find(|s| s.id == id) {
        Some(s) if s.visible != visible => {
            s.visible = visible;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_appends_new_and_drops_unknown() {
        let saved = vec![
            Slot { id: "notes".into(), visible: false, cols: 99, rows: 0 },
            Slot { id: "gone-widget".into(), visible: true, cols: 4, rows: 2 },
        ];
        let out = merge(saved);
        assert_eq!(out[0].id, "notes");
        assert_eq!((out[0].visible, out[0].cols, out[0].rows), (false, 12, 1));
        assert!(!out.iter().any(|s| s.id == "gone-widget"));
        assert_eq!(out.len(), REGISTRY.len());
    }

    #[test]
    fn move_and_resize() {
        let mut s = defaults();
        let ids = |s: &[Slot]| s.iter().map(|x| x.id.clone()).collect::<Vec<_>>();
        assert!(move_slot(&mut s, "notes", "tasks", false));
        assert_eq!(ids(&s)[0], "notes");
        assert!(!move_slot(&mut s, "notes", "tasks", false), "same spot is a no-op");
        assert!(move_slot(&mut s, "notes", "events", true));
        let pos = |s: &[Slot], id: &str| s.iter().position(|x| x.id == id).unwrap();
        assert_eq!(pos(&s, "notes"), pos(&s, "events") + 1);
        assert!(!move_slot(&mut s, "notes", "missing", true));
        assert_eq!(s.len(), REGISTRY.len());
        assert!(resize(&mut s, "notes", 1, 9));
        let n = &s[pos(&s, "notes")];
        assert_eq!((n.cols, n.rows), (3, 4));
        assert!(!resize(&mut s, "notes", 3, 4));
        assert!(set_visible(&mut s, "notes", false));
        assert!(!set_visible(&mut s, "notes", false));
    }
}
