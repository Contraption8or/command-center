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
}
