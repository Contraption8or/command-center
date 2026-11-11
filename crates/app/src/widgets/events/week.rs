//! Week view: a Mon–Sun time grid. Drag on empty space to create an event, drag an event to
//! move it (also across days), drag its bottom edge to resize, click to open it.
//!
//! Pointer handling runs in a small script on the grid (exact pixel maths, a live preview
//! while dragging) and sends the result back here as `create:…`, `place:…` or `select:…`.

use super::{color_var, create_at, SELECTED};
use crate::state::use_ui;
use crate::store::use_store;
use cc_core::model::events::{self, Event};
use cc_core::model::habits::week_start;
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use dioxus::prelude::*;

/// Pixel height of one hour (also in CSS as --hour).
const HOUR_PX: i64 = 48;

const CAL_JS: &str = r#"
const SNAP = 15, MIN_PX = 4;
const send = (m) => dioxus.send(m);
let grid = null;
for (let i = 0; i < 200 && !grid; i++) {
  grid = document.getElementById('wk-grid');
  if (!grid) await new Promise(r => setTimeout(r, 16));
}
if (grid) {
  const scroll = document.getElementById('wk-scroll');
  const overlay = document.getElementById('wk-overlay');
  // Start the view around the working day (or an hour before now, if today is shown).
  const nowLine = grid.querySelector('.wk-now');
  const startMin = nowLine ? Math.max(0, +nowLine.dataset.min - 90) : 7.5 * 60;
  scroll.scrollTop = startMin / 1440 * grid.getBoundingClientRect().height;

  const minutesIn = (col, y) => {
    const r = col.getBoundingClientRect();
    return Math.max(0, Math.min(1440, (y - r.top) / r.height * 1440));
  };
  const snap = (m) => Math.round(m / SNAP) * SNAP;
  const colAt = (x, y) => { const el = document.elementFromPoint(x, y); return el && el.closest('.wk-col'); };
  const ghostFor = (col) => {
    const g = document.createElement('div');
    g.className = 'wk-ghost';
    overlay.appendChild(g);
    return g;
  };
  const placeGhost = (g, col, s, e) => {
    const gr = grid.getBoundingClientRect(), cr = col.getBoundingClientRect();
    const scale = gr.height / 1440;
    g.style.left = (cr.left - gr.left + 2) + 'px';
    g.style.width = (cr.width - 6) + 'px';
    g.style.top = (s * scale) + 'px';
    g.style.height = Math.max(8, (e - s) * scale - 2) + 'px';
    const fmt = (m) => { m = Math.min(m, 1440); const h = Math.floor(m / 60) % 24, mm = String(m % 60).padStart(2, '0');
      return grid.dataset.h12 === '1' ? `${(h % 12) || 12}:${mm} ${h < 12 ? 'AM' : 'PM'}` : `${h}:${mm}`; };
    g.textContent = fmt(s) + ' – ' + fmt(e);
  };

  grid.addEventListener('mousedown', (ev) => {
    if (ev.button !== 0) return;
    const block = ev.target.closest('.wk-ev');
    const col = ev.target.closest('.wk-col');
    if (!col) return;
    ev.preventDefault();
    const x0 = ev.clientX, y0 = ev.clientY;
    let moved = false, ghost = null, target = null;

    if (block) {
      const id = block.dataset.evId, s0 = +block.dataset.start, e0 = +block.dataset.end;
      const resize = ev.target.classList.contains('wk-resize');
      if (block.dataset.manual !== '1') { const up = () => { document.removeEventListener('mouseup', up); send('select:' + id); }; document.addEventListener('mouseup', up); return; }
      const grab = minutesIn(col, y0) - s0;
      let ns = s0, ne = e0, ncol = col;
      const move = (m) => {
        if (!moved && Math.hypot(m.clientX - x0, m.clientY - y0) < MIN_PX) return;
        if (!moved) { moved = true; ghost = ghostFor(col); block.classList.add('wk-dragging'); }
        if (resize) {
          ne = Math.max(s0 + SNAP, snap(minutesIn(col, m.clientY)));
        } else {
          ncol = colAt(m.clientX, m.clientY) || ncol;
          ns = Math.max(0, Math.min(1440 - (e0 - s0), snap(minutesIn(ncol, m.clientY) - grab)));
          ne = ns + (e0 - s0);
        }
        placeGhost(ghost, resize ? col : ncol, ns, ne);
      };
      const up = () => {
        document.removeEventListener('mousemove', move);
        document.removeEventListener('mouseup', up);
        if (ghost) ghost.remove();
        block.classList.remove('wk-dragging');
        if (!moved) send('select:' + id);
        else send(`place:${id}:${(resize ? col : ncol).dataset.date}:${ns}:${ne}`);
      };
      document.addEventListener('mousemove', move);
      document.addEventListener('mouseup', up);
      return;
    }

    // Empty space: drag out a new event (a plain click makes a one-hour event).
    const a = Math.floor(minutesIn(col, y0) / SNAP) * SNAP;
    let s = a, e = a + 60;
    const move = (m) => {
      if (!moved && Math.abs(m.clientY - y0) < MIN_PX) return;
      if (!moved) { moved = true; ghost = ghostFor(col); }
      const b = snap(minutesIn(col, m.clientY));
      s = Math.min(a, b); e = Math.max(a + SNAP, b, s + SNAP);
      placeGhost(ghost, col, s, e);
    };
    const up = () => {
      document.removeEventListener('mousemove', move);
      document.removeEventListener('mouseup', up);
      if (ghost) ghost.remove();
      if (!moved) { s = a; e = Math.min(1440, a + 60); }
      send(`create:${col.dataset.date}:${s}:${e}`);
    };
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', up);
  });
}
await new Promise(() => {});
"#;

/// Minutes after midnight on `day` covered by `e` (clipped to that day).
fn span_on(e: &Event, day: NaiveDate) -> (i64, i64) {
    let mins = |t: chrono::NaiveDateTime| {
        if t.date() < day { 0 } else if t.date() > day { 1440 } else { (t.hour() * 60 + t.minute()) as i64 }
    };
    let s = mins(e.starts_at);
    let end = e.ends_at.map(mins).unwrap_or(s + 60);
    (s, end.max(s + 15).min(1440))
}

/// Side-by-side lanes for overlapping events: (event, start, end, lane, lanes in cluster).
fn lay_out(mut items: Vec<(Event, i64, i64)>) -> Vec<(Event, i64, i64, usize, usize)> {
    items.sort_by_key(|(_, s, e)| (*s, -*e));
    let mut out = Vec::new();
    let mut cluster: Vec<(Event, i64, i64, usize)> = Vec::new();
    let mut lane_ends: Vec<i64> = Vec::new();
    let mut cluster_end = -1;
    let flush = |cluster: &mut Vec<(Event, i64, i64, usize)>, lanes: usize, out: &mut Vec<_>| {
        for (e, s, en, l) in cluster.drain(..) {
            out.push((e, s, en, l, lanes));
        }
    };
    for (e, s, en) in items {
        if s >= cluster_end && !cluster.is_empty() {
            let lanes = lane_ends.len();
            flush(&mut cluster, lanes, &mut out);
            lane_ends.clear();
        }
        let lane = lane_ends.iter().position(|end| *end <= s).unwrap_or_else(|| {
            lane_ends.push(0);
            lane_ends.len() - 1
        });
        lane_ends[lane] = en;
        cluster_end = cluster_end.max(en);
        cluster.push((e, s, en, lane));
    }
    let lanes = lane_ends.len();
    flush(&mut cluster, lanes, &mut out);
    out
}

#[component]
pub fn WeekView(anchor: NaiveDate) -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    let h12 = *ui.h12.read();
    let monday = week_start(anchor);
    let days: Vec<NaiveDate> = (0..7).map(|i| monday + Duration::days(i)).collect();
    let list = store.read(|db| events::between(db, monday, monday + Duration::days(6)));
    let now = Local::now().naive_local();
    let now_min = (now.hour() * 60 + now.minute()) as i64;

    // Pointer messages from the grid script.
    let s_js = store.clone();
    use_future(move || {
        let store = s_js.clone();
        async move {
            let mut bridge = document::eval(CAL_JS);
            while let Ok(msg) = bridge.recv::<String>().await {
                let p: Vec<&str> = msg.split(':').collect();
                let date = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
                match p.as_slice() {
                    ["create", d, s, e] => {
                        if let (Some(d), Ok(s), Ok(e)) = (date(d), s.parse(), e.parse()) {
                            create_at(&store, d, s, e);
                        }
                    }
                    ["place", id, d, s, e] => {
                        if let (Ok(id), Some(d), Ok(s), Ok(e)) = (id.parse(), date(d), s.parse(), e.parse()) {
                            let _ = store.write(|db| events::place(db, id, d, s, e));
                            *SELECTED.write() = Some(id);
                        }
                    }
                    ["select", id] => {
                        if let Ok(id) = id.parse::<i64>() {
                            let next = if SELECTED() == Some(id) { None } else { Some(id) };
                            *SELECTED.write() = next;
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    let hour_label = |h: u32| {
        if h12 { format!("{} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" }) } else { format!("{h}:00") }
    };
    let selected = SELECTED();

    rsx! {
        div { class: "wk", style: "--hour: {HOUR_PX}px",
            div { class: "wk-head",
                div { class: "wk-gutter" }
                for d in days.iter().copied() {
                    div { key: "{d}", class: if d == today { "wk-dayhead today" } else { "wk-dayhead" },
                        span { class: "wk-dow", "{d.format(\"%a\")}" }
                        span { class: "wk-dom", "{d.day()}" }
                    }
                }
            }
            div { class: "wk-allday",
                div { class: "wk-gutter small muted", "all-day" }
                for d in days.iter().copied() {
                    div { key: "{d}", class: "wk-allday-cell",
                        for e in list.iter().filter(|e| e.all_day && e.days_in(d, d).len() == 1).cloned() {
                            button {
                                key: "{e.id}",
                                class: if selected == Some(e.id) { "wk-chip sel" } else { "wk-chip" },
                                style: "{color_var(&e)}",
                                "data-ev-id": "{e.id}",
                                "data-manual": if e.is_manual() { "1" } else { "0" },
                                "data-ts": "{cc_core::tz::unix(e.starts_at)}",
                                onclick: move |_| { let next = if SELECTED() == Some(e.id) { None } else { Some(e.id) }; *SELECTED.write() = next; },
                                "{e.title}"
                            }
                        }
                    }
                }
            }
            div { class: "wk-scroll", id: "wk-scroll",
                div { class: "wk-body", id: "wk-grid", "data-h12": if h12 { "1" } else { "0" },
                    div { class: "wk-hours",
                        for h in 0..24u32 {
                            div { key: "{h}", class: "wk-hour", if h > 0 { span { "{hour_label(h)}" } } }
                        }
                    }
                    for d in days.iter().copied() {
                        {
                            let items: Vec<(Event, i64, i64)> = list
                                .iter()
                                .filter(|e| !e.all_day && (e.start_date() == d || (e.start_date() < d && e.ends_at.is_some_and(|x| x.date() >= d && x > d.and_hms_opt(0, 0, 0).unwrap()))))
                                .map(|e| { let (s, en) = span_on(e, d); (e.clone(), s, en) })
                                .collect();
                            let placed = lay_out(items);
                            rsx! {
                                div { key: "{d}", class: if d == today { "wk-col today" } else { "wk-col" }, "data-date": "{d}",
                                    for (e, s, en, lane, lanes) in placed {
                                        {
                                            let top = s * HOUR_PX / 60;
                                            let height = ((en - s) * HOUR_PX / 60).max(14);
                                            let w = 100.0 / lanes as f64;
                                            let left = w * lane as f64;
                                            let past = e.is_past(now);
                                            let class = format!(
                                                "wk-ev{}{}{}",
                                                if selected == Some(e.id) { " sel" } else { "" },
                                                if past { " past" } else { "" },
                                                if height < 34 { " short" } else { "" }
                                            );
                                            let time = e.time_label(h12);
                                            rsx! {
                                                div {
                                                    key: "{e.id}",
                                                    class: "{class}",
                                                    style: "top: {top}px; height: {height}px; left: calc({left}% + 2px); width: calc({w}% - 6px); {color_var(&e)}",
                                                    "data-ev-id": "{e.id}",
                                                    "data-start": "{s}",
                                                    "data-end": "{en}",
                                                    "data-manual": if e.is_manual() { "1" } else { "0" },
                                                    "data-ts": "{cc_core::tz::unix(e.starts_at)}",
                                                    title: "{e.title} · {time}",
                                                    div { class: "wk-ev-title", "{e.title}" }
                                                    if height >= 34 { div { class: "wk-ev-time", "{time}" } }
                                                    if e.is_manual() { div { class: "wk-resize" } }
                                                }
                                            }
                                        }
                                    }
                                    if d == today {
                                        div { class: "wk-now", "data-min": "{now_min}", style: "top: {now_min * HOUR_PX / 60}px" }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "wk-overlay", id: "wk-overlay" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn ev(id: i64) -> Event {
        Event {
            id,
            title: String::new(),
            starts_at: NaiveDateTime::default(),
            ends_at: None,
            all_day: false,
            location: String::new(),
            notes: String::new(),
            source: "manual".into(),
            color: None,
        }
    }

    #[test]
    fn overlapping_events_share_lanes() {
        let placed = lay_out(vec![(ev(1), 600, 660), (ev(2), 630, 690), (ev(3), 700, 760), (ev(4), 600, 620)]);
        let get = |id| placed.iter().find(|p| p.0.id == id).map(|p| (p.3, p.4)).unwrap();
        assert_eq!(get(1).1, 2, "at most two overlap at once; the 10:30 event reuses a freed lane");
        assert_ne!(get(1).0, get(2).0);
        assert_eq!(get(3), (0, 1), "a later, separate event gets the full width");
    }
}
