//! Split view: two modules side by side, each in its own pane.
//!
//! Drag a module's icon in the side rail onto another module's icon (or onto the open
//! view) to show both. Drop an icon on a pane to replace it, drag the divider to resize,
//! and use a pane's buttons to give it the whole screen or close it.

use super::FullHost;
use crate::icons::{self, Icon};
use crate::state::{use_ui, Ui, View};
use crate::store::{use_store, Store};
use crate::widgets;
use dioxus::prelude::*;

const RATIO_KEY: &str = "split.ratio";
/// Left pane's share of the width.
pub static RATIO: GlobalSignal<f64> = Signal::global(|| 0.5);

fn clamp(r: f64) -> f64 {
    r.clamp(0.25, 0.75)
}

/// What a drop does to the current view. `None` = nothing changes.
pub fn apply_drop(view: View, msg: &str) -> Option<View> {
    let id = |s: &str| widgets::get(s).map(|w| w.id);
    let parts: Vec<&str> = msg.split(':').collect();
    match parts.as_slice() {
        // Icon dropped on another icon: the one you dropped on stays left.
        ["split", src, target] => {
            let (s, t) = (id(src)?, id(target)?);
            (s != t).then_some(View::Split(t, s))
        }
        // Icon dropped on a pane: it takes that pane (dropping the other pane's module swaps them).
        ["pane", side, src] => {
            let s = id(src)?;
            let View::Split(l, r) = view else { return None };
            let next = match *side {
                "left" if s == r => View::Split(r, l),
                "left" => View::Split(s, r),
                "right" if s == l => View::Split(r, l),
                _ => View::Split(l, s),
            };
            (next != view).then_some(next)
        }
        // Icon dropped on the open view: show it beside that module.
        ["beside", src] => {
            let s = id(src)?;
            match view {
                View::Widget(cur) if cur != s => Some(View::Split(cur, s)),
                View::Widget(_) | View::Split(..) => None,
                _ => Some(View::Widget(s)),
            }
        }
        _ => None,
    }
}

const SPLIT_JS: &str = r#"
const send = (m) => dioxus.send(m);
if (!window.__ccSplit) {
  window.__ccSplit = true;
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;

    // Divider: resize the panes live, report the final ratio.
    const gutter = e.target.closest('.split-gutter');
    if (gutter) {
      e.preventDefault();
      const split = gutter.closest('.split');
      const r = split.getBoundingClientRect();
      let ratio = null;
      split.classList.add('resizing');
      const mv = (ev) => {
        ratio = Math.min(.75, Math.max(.25, (ev.clientX - r.left) / r.width));
        split.style.setProperty('--split', ratio);
      };
      const up = () => {
        document.removeEventListener('mousemove', mv, true);
        document.removeEventListener('mouseup', up, true);
        split.classList.remove('resizing');
        if (ratio !== null) send('ratio:' + ratio.toFixed(3));
      };
      document.addEventListener('mousemove', mv, true);
      document.addEventListener('mouseup', up, true);
      return;
    }

    // Rail icon: drag it onto another icon, a pane, or the open view.
    const btn = e.target.closest('.rail-btn[data-tab]');
    if (!btn) return;
    const sx = e.clientX, sy = e.clientY, src = btn.dataset.tab;
    let ghost = null, over = null;
    const targetAt = (x, y) => {
      const el = document.elementFromPoint(x, y);
      if (!el) return null;
      const t = el.closest('.rail-btn[data-tab]');
      if (t) return t.dataset.tab === src ? null : { el: t, msg: 'split:' + src + ':' + t.dataset.tab };
      const p = el.closest('[data-pane]');
      if (p) return { el: p, msg: 'pane:' + p.dataset.pane + ':' + src };
      const c = el.closest('.content');
      if (c && !c.querySelector('[data-pane]')) return { el: c, msg: 'beside:' + src };
      return null;
    };
    const mv = (ev) => {
      if (!ghost) {
        if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return;
        ghost = btn.cloneNode(true);
        ghost.classList.remove('active');
        ghost.classList.add('rail-ghost');
        document.body.appendChild(ghost);
        document.body.classList.add('tab-dragging');
        btn.classList.add('drag-src');
      }
      ghost.style.left = (ev.clientX - 19) + 'px';
      ghost.style.top = (ev.clientY - 19) + 'px';
      const t = targetAt(ev.clientX, ev.clientY);
      const el = t && t.el;
      if (over !== el) {
        if (over) over.classList.remove('drop-over');
        if (el) el.classList.add('drop-over');
        over = el;
      }
    };
    const up = (ev) => {
      document.removeEventListener('mousemove', mv, true);
      document.removeEventListener('mouseup', up, true);
      if (!ghost) return; // a plain click
      ghost.remove();
      btn.classList.remove('drag-src');
      document.body.classList.remove('tab-dragging');
      if (over) over.classList.remove('drop-over');
      const t = targetAt(ev.clientX, ev.clientY);
      if (t) send(t.msg);
      // The drag ends in a click on whatever is under the pointer: swallow it.
      const kill = (c) => { c.stopPropagation(); c.preventDefault(); };
      document.addEventListener('click', kill, { capture: true, once: true });
      setTimeout(() => document.removeEventListener('click', kill, true), 60);
    };
    document.addEventListener('mousemove', mv, true);
    document.addEventListener('mouseup', up, true);
  }, true);
}
await new Promise(() => {});
"#;

/// Runs the drag script for the app's lifetime (called once from the rail).
pub fn use_split_bridge() {
    let store = use_store();
    let ui = use_ui();
    use_hook(|| {
        if let Some(r) = store.setting(RATIO_KEY).and_then(|v| v.parse::<f64>().ok()) {
            *RATIO.write() = clamp(r);
        }
    });
    use_future(move || {
        let store = store.clone();
        async move {
            let mut bridge = document::eval(SPLIT_JS);
            while let Ok(msg) = bridge.recv::<String>().await {
                handle(&store, ui, &msg);
            }
        }
    });
}

fn handle(store: &Store, ui: Ui, msg: &str) {
    if let Some(r) = msg.strip_prefix("ratio:").and_then(|v| v.parse::<f64>().ok()) {
        let r = clamp(r);
        *RATIO.write() = r;
        store.set_setting(RATIO_KEY, &format!("{r:.3}"));
        return;
    }
    // Copy the view out first: a borrow held across `open` would conflict with its write.
    let current = *ui.view.peek();
    if let Some(next) = apply_drop(current, msg) {
        ui.open(next);
    }
}

#[component]
pub fn SplitView(left: &'static str, right: &'static str) -> Element {
    let ratio = RATIO();
    rsx! {
        div { class: "split", style: "--split: {ratio}",
            Pane { side: "left", id: left, other: right }
            div { class: "split-gutter", title: "Drag to resize" }
            Pane { side: "right", id: right, other: left }
        }
    }
}

#[component]
fn Pane(side: &'static str, id: &'static str, other: &'static str) -> Element {
    let ui = use_ui();
    let Some(def) = widgets::get(id) else { return rsx! {} };
    rsx! {
        section { class: "split-pane", "data-pane": side,
            div { class: "pane-head",
                Icon { d: def.icon }
                span { class: "pane-title", "{def.title}" }
                div { class: "grow" }
                button { class: "icon-btn", title: "Show only {def.title}", onclick: move |_| ui.open(View::Widget(id)), Icon { d: icons::EXPAND } }
                button { class: "icon-btn", title: "Close this pane", onclick: move |_| ui.open(View::Widget(other)), Icon { d: icons::CLOSE } }
            }
            div { class: "pane-body", FullHost { id } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops() {
        let w = View::Widget;
        let s = View::Split;
        assert_eq!(apply_drop(View::Overview, "split:tasks:events"), Some(s("events", "tasks")));
        assert_eq!(apply_drop(View::Overview, "split:tasks:tasks"), None);
        assert_eq!(apply_drop(View::Overview, "split:tasks:nope"), None);
        assert_eq!(apply_drop(w("events"), "beside:tasks"), Some(s("events", "tasks")));
        assert_eq!(apply_drop(w("tasks"), "beside:tasks"), None);
        assert_eq!(apply_drop(View::Overview, "beside:notes"), Some(w("notes")));
        assert_eq!(apply_drop(s("events", "tasks"), "pane:left:notes"), Some(s("notes", "tasks")));
        assert_eq!(apply_drop(s("events", "tasks"), "pane:right:notes"), Some(s("events", "notes")));
        assert_eq!(apply_drop(s("events", "tasks"), "pane:left:tasks"), Some(s("tasks", "events")), "swap");
        assert_eq!(apply_drop(s("events", "tasks"), "pane:right:tasks"), None);
        assert_eq!(apply_drop(w("tasks"), "pane:left:notes"), None);
    }
}
