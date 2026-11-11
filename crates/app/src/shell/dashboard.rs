//! Dashboard edit mode: drag cards to reorder them (the grid reflows live), drag a card's
//! right edge, bottom edge or corner to resize it by grid slots, hide cards and bring
//! hidden ones back. Every change is saved as it happens.

use crate::icons::{self, Icon};
use crate::layout::{self, Slot};
use crate::state::{Ui, View};
use crate::store::{use_store, Store};
use crate::widgets;
use dioxus::prelude::*;

pub static EDITING: GlobalSignal<bool> = Signal::global(|| false);

/// Palette / Settings entry point.
pub fn start(_: &Store, ui: Ui) {
    *EDITING.write() = true;
    ui.open(View::Overview);
}

pub fn stop() {
    *EDITING.write() = false;
}

/// Apply `f` to the saved layout; save only if it reports a change.
fn edit(store: &Store, f: impl FnOnce(&mut Vec<Slot>) -> bool) {
    let mut slots = store.peek(layout::load);
    if f(&mut slots) {
        let _ = store.write(|db| layout::save(db, &slots));
    }
}

pub fn hide(store: &Store, id: &str) {
    edit(store, |s| layout::set_visible(s, id, false));
}

const DASH_JS: &str = r#"
const send = (m) => dioxus.send(m);
let grid = null;
for (let i = 0; i < 200 && !grid; i++) {
  grid = document.getElementById('dash-grid');
  if (!grid) await new Promise(r => setTimeout(r, 16));
}
if (grid) {
  // One set of listeners per edit session.
  if (grid._dashAbort) grid._dashAbort.abort();
  const ac = new AbortController();
  grid._dashAbort = ac;
  const gapOf = () => parseFloat(getComputedStyle(grid).columnGap) || 0;

  grid.addEventListener('mousedown', (ev) => {
    if (ev.button !== 0 || !grid.classList.contains('editing')) return;
    const card = ev.target.closest('[data-slot]');
    if (!card || ev.target.closest('button')) return;
    ev.preventDefault();
    const id = card.dataset.slot;
    const handle = ev.target.closest('[data-resize]');
    let move, up;

    if (handle) {
      // Resize: one grid slot per column / row width, measured from the card itself.
      const axis = handle.dataset.resize, g = gapOf();
      let last = card.dataset.cols + ':' + card.dataset.rows;
      card.classList.add('resizing');
      grid.classList.add('busy');
      move = (e) => {
        // Slot size from the card as it is now: rows shrink as the grid gains rows.
        const cr = card.getBoundingClientRect();
        let c = +card.dataset.cols, n = +card.dataset.rows;
        const uw = (cr.width + g) / c, uh = (cr.height + g) / n;
        if (axis.includes('x')) c = Math.max(3, Math.min(12, Math.round((e.clientX - cr.left + g) / uw)));
        if (axis.includes('y')) n = Math.max(1, Math.min(4, Math.round((e.clientY - cr.top + g) / uh)));
        const key = c + ':' + n;
        if (key !== last) { last = key; send('resize:' + id + ':' + key); }
      };
      up = () => { card.classList.remove('resizing'); grid.classList.remove('busy'); };
    } else {
      // Move: the card follows the pointer; hovering another card's half moves it there.
      const r0 = card.getBoundingClientRect();
      const ox = ev.clientX - r0.left, oy = ev.clientY - r0.top;
      let px = ev.clientX, py = ev.clientY, started = false, lastKey = '', cooldown = 0, raf = 0;
      const follow = () => {
        card.style.transform = '';
        const r = card.getBoundingClientRect();
        card.style.transform = `translate(${px - ox - r.left}px, ${py - oy - r.top}px)`;
        raf = requestAnimationFrame(follow);
      };
      move = (e) => {
        px = e.clientX; py = e.clientY;
        if (!started) {
          if (Math.hypot(px - ev.clientX, py - ev.clientY) < 4) return;
          started = true;
          card.classList.add('dragging');
          grid.classList.add('busy');
          raf = requestAnimationFrame(follow);
        }
        if (performance.now() < cooldown) return;
        for (const o of grid.querySelectorAll('[data-slot]')) {
          if (o === card) continue;
          const r = o.getBoundingClientRect();
          if (px < r.left || px > r.right || py < r.top || py > r.bottom) continue;
          const wide = r.width > grid.clientWidth * 0.75;
          const after = wide ? py > r.top + r.height / 2 : px > r.left + r.width / 2;
          const key = o.dataset.slot + ':' + (after ? 'after' : 'before');
          if (key !== lastKey) {
            lastKey = key;
            cooldown = performance.now() + 200;
            send('move:' + id + ':' + key);
          }
          return;
        }
      };
      up = () => {
        cancelAnimationFrame(raf);
        card.style.transform = '';
        card.classList.remove('dragging');
        grid.classList.remove('busy');
      };
    }
    const end = () => {
      document.removeEventListener('mousemove', move, true);
      document.removeEventListener('mouseup', end, true);
      up();
    };
    document.addEventListener('mousemove', move, true);
    document.addEventListener('mouseup', end, true);
  }, { signal: ac.signal });
}
await new Promise(() => {});
"#;

/// Runs the pointer script while edit mode is on and applies its messages.
#[component]
pub fn DashBridge() -> Element {
    let store = use_store();
    use_future(move || {
        let store = store.clone();
        async move {
            let mut bridge = document::eval(DASH_JS);
            while let Ok(msg) = bridge.recv::<String>().await {
                match msg.split(':').collect::<Vec<_>>().as_slice() {
                    ["move", id, target, side] => edit(&store, |s| layout::move_slot(s, id, target, *side == "after")),
                    ["resize", id, c, r] => {
                        if let (Ok(c), Ok(r)) = (c.parse(), r.parse()) {
                            edit(&store, |s| layout::resize(s, id, c, r));
                        }
                    }
                    _ => {}
                }
            }
        }
    });
    rsx! {}
}

/// Top-bar controls while editing: hidden cards to bring back, Reset and Done.
#[component]
pub fn EditBar() -> Element {
    let store = use_store();
    let slots = store.read(layout::load);
    let s_reset = store.clone();
    rsx! {
        div { class: "dash-bar",
            for s in slots.into_iter().filter(|s| !s.visible) {
                if let Some(def) = widgets::get(&s.id) {
                    {
                        let store = store.clone();
                        let id = s.id.clone();
                        rsx! {
                            button { key: "{id}", class: "btn dash-hidden", title: "Show on the dashboard",
                                onclick: move |_| edit(&store, |s| layout::set_visible(s, &id, true)),
                                Icon { d: icons::PLUS }
                                "{def.title}"
                            }
                        }
                    }
                }
            }
            button { class: "btn", onclick: move |_| { let _ = s_reset.write(|db| layout::reset(db)); }, "Reset" }
            button { class: "btn primary", onclick: move |_| stop(), "Done" }
        }
    }
}
