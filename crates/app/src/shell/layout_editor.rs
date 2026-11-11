//! Settings → Dashboard: reorder, resize and show/hide overview cards.

use crate::icons::{self, Icon};
use crate::layout::{self, Slot, HEIGHTS, WIDTHS};
use crate::store::use_store;
use crate::widgets;
use dioxus::prelude::*;

#[component]
pub fn LayoutEditor() -> Element {
    let store = use_store();
    let slots = store.read(layout::load);
    let n = slots.len();
    let s_reset = store.clone();
    let store_e = store.clone();
    let ui = crate::state::use_ui();

    // Apply a change to a copy of the layout and save it.
    let edit = {
        let store = store.clone();
        let slots = slots.clone();
        move |f: Box<dyn FnOnce(&mut Vec<Slot>)>| {
            let mut v = slots.clone();
            f(&mut v);
            let _ = store.write(|db| layout::save(db, &v));
        }
    };

    rsx! {
        div { class: "panel",
            div { class: "panel-head",
                h3 { "Dashboard" }
                div { class: "grow" }
                button { class: "btn", onclick: move |_| super::dashboard::start(&store_e, ui), "Edit on dashboard" }
                button { class: "btn", onclick: move |_| { let _ = s_reset.write(|db| layout::reset(db)); }, "Reset" }
            }
            div { class: "layout-list",
                for (i, s) in slots.iter().cloned().enumerate() {
                    {
                        let def = widgets::get(&s.id);
                        let title = def.map_or(s.id.clone(), |d| d.title.to_string());
                        let icon = def.map_or(icons::OVERVIEW, |d| d.icon);
                        let (e_up, e_down, e_vis, e_w, e_h) = (edit.clone(), edit.clone(), edit.clone(), edit.clone(), edit.clone());
                        rsx! {
                            div { key: "{s.id}", class: if s.visible { "layout-row" } else { "layout-row hidden" },
                                Icon { d: icon }
                                span { class: "layout-name", "{title}" }
                                div { class: "seg small",
                                    for (c, label) in WIDTHS {
                                        button {
                                            class: if s.cols == c { "on" },
                                            onclick: { let e = e_w.clone(); move |_| e(Box::new(move |v| v[i].cols = c)) },
                                            "{label}"
                                        }
                                    }
                                }
                                div { class: "seg small",
                                    for (r, label) in HEIGHTS {
                                        button {
                                            class: if s.rows == r { "on" },
                                            onclick: { let e = e_h.clone(); move |_| e(Box::new(move |v| v[i].rows = r)) },
                                            "{label}"
                                        }
                                    }
                                }
                                button { class: "icon-btn", title: "Move up", disabled: i == 0,
                                    onclick: move |_| e_up(Box::new(move |v| v.swap(i, i - 1))), Icon { d: icons::ARROW_UP } }
                                button { class: "icon-btn", title: "Move down", disabled: i + 1 == n,
                                    onclick: move |_| e_down(Box::new(move |v| v.swap(i, i + 1))), Icon { d: icons::ARROW_DOWN } }
                                button { class: if s.visible { "btn" } else { "btn primary" },
                                    onclick: move |_| e_vis(Box::new(move |v| v[i].visible = !v[i].visible)),
                                    if s.visible { "Hide" } else { "Show" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
