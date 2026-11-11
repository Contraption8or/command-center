//! Ctrl+K command palette: fuzzy search over every command the shell and widgets register.

use super::all_commands;
use crate::icons::{self, Icon};
use crate::state::{run_action, use_ui, Action, Command};
use crate::store::use_store;
use dioxus::prelude::*;

/// Subsequence match score; higher is better. `None` = no match.
pub fn fuzzy(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut ti = 0;
    let mut prev_match = false;
    for qc in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let mut found = false;
        while ti < t.len() {
            let tc = t[ti];
            let at_word = ti == 0 || !t[ti - 1].is_alphanumeric();
            ti += 1;
            if tc == qc {
                score += 1 + if prev_match { 3 } else { 0 } + if at_word { 4 } else { 0 };
                prev_match = true;
                found = true;
                break;
            }
            prev_match = false;
        }
        if !found {
            return None;
        }
    }
    Some(score - (t.len() as i32 / 10))
}

enum Item {
    Cmd(Command),
    Hit(Command, String),
    CreateTask(String),
}

#[component]
pub fn Palette() -> Element {
    let store = use_store();
    let mut ui = use_ui();
    let mut query = use_signal(String::new);
    let mut sel = use_signal(|| 0usize);

    let q = query.read().trim().to_string();
    let mut scored: Vec<(i32, usize, Command)> = all_commands()
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let label = fuzzy(&q, &c.label);
            let kw = fuzzy(&q, &format!("{} {}", c.label, c.keywords)).map(|s| s - 2);
            label.max(kw).map(|s| (s, i, c))
        })
        .collect();
    if !q.is_empty() {
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    let mut items: Vec<Item> = scored.into_iter().map(|(_, _, c)| Item::Cmd(c)).collect();
    if !q.is_empty() {
        // Content matches from widgets that support search (tasks, notes, …).
        for w in crate::widgets::REGISTRY {
            if let Some(search) = w.search {
                for hit in store.peek(|db| search(db, &q)).into_iter().take(5) {
                    let mut c = Command::new(w.title, w.icon, hit.label, Action::OpenItem(w.id, hit.id));
                    c.keywords = "";
                    items.push(Item::Hit(c, hit.detail));
                }
            }
        }
        items.push(Item::CreateTask(q.clone()));
    }
    let len = items.len();
    let cur = (*sel.read()).min(len.saturating_sub(1));
    let items = std::rc::Rc::new(items);

    let run = {
        let store = store.clone();
        let items = items.clone();
        move |i: usize| match items.get(i) {
            Some(Item::Cmd(c)) | Some(Item::Hit(c, _)) => run_action(c.action, &store, ui),
            Some(Item::CreateTask(text)) => ui.open_quick_add(text),
            None => {}
        }
    };
    let run_key = run.clone();

    // Group headers only when browsing (empty query); ranked results read better flat.
    let show_groups = q.is_empty();
    let mut last_group = "";
    let rows = items.iter().enumerate().map(|(i, item)| {
        let (label, icon, group, shortcut, detail, is_hit) = match item {
            Item::Cmd(c) => (c.label.clone(), c.icon, c.group, c.shortcut, String::new(), false),
            Item::Hit(c, d) => (c.label.clone(), c.icon, c.group, None, d.clone(), true),
            Item::CreateTask(t) => (format!("Add “{t}”"), icons::PLUS, "Create", None, String::new(), false),
        };
        // Search hits always get a header naming their widget.
        let header = ((show_groups || is_hit) && group != last_group).then_some(group);
        last_group = group;
        let run = run.clone();
        rsx! {
            if let Some(g) = header { div { class: "group", "{g}" } }
            div {
                class: if i == cur { "item sel" } else { "item" },
                onmouseenter: move |_| sel.set(i),
                onclick: move |_| run(i),
                Icon { d: icon }
                span { class: "grow", "{label}" }
                if !detail.is_empty() { span { class: "detail", "{detail}" } }
                if let Some(s) = shortcut { kbd { "{s}" } }
            }
        }
    });

    rsx! {
        div { class: "scrim", onclick: move |_| ui.palette.set(false),
            div { class: "palette", onclick: move |e| e.stop_propagation(),
                div { class: "input-row",
                    Icon { d: icons::SEARCH }
                    input {
                        placeholder: "Jump to or create…",
                        spellcheck: "false",
                        value: "{query}",
                        onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                        oninput: move |e| { query.set(e.value()); sel.set(0); },
                        onkeydown: move |e| match e.key() {
                            Key::ArrowDown => { e.prevent_default(); sel.set((cur + 1) % len.max(1)); }
                            Key::ArrowUp => { e.prevent_default(); sel.set((cur + len.max(1) - 1) % len.max(1)); }
                            Key::Enter => { e.prevent_default(); run_key(cur); }
                            _ => {}
                        },
                    }
                }
                div { class: "list",
                    if len == 0 { div { class: "empty", "No matches" } }
                    {rows}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn ranks_word_starts_higher() {
        assert!(fuzzy("set", "Settings") > fuzzy("set", "Reset layout"));
        assert!(fuzzy("tdt", "Toggle dark theme").is_some());
        assert!(fuzzy("xyz", "Settings").is_none());
    }
}
