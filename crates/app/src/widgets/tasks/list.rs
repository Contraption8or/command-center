//! Task list building blocks shared by the overview card and the full view.

use crate::icons::{self, Icon};
use crate::state::{use_ui, View};
use crate::store::use_store;
use cc_core::model::{find_project, project_options, tasks::{self, Task}};
use cc_core::quick_add::{parse, QuickAdd};
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;

/// Task whose detail editor is open (shared so the card can open a task in the full view).
pub static EXPANDED: GlobalSignal<Option<i64>> = Signal::global(|| None);
static DRAGGING: GlobalSignal<Option<i64>> = Signal::global(|| None);
static OVER: GlobalSignal<Option<i64>> = Signal::global(|| None);

/// "Today", "Tomorrow", "Fri", or "Nov 20".
pub fn day_label(d: NaiveDate, today: NaiveDate) -> String {
    let diff = (d - today).num_days();
    match diff {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        2..=6 => d.format("%a").to_string(),
        _ if d.year() == today.year() => d.format("%b %-d").to_string(),
        _ => d.format("%b %-d, %Y").to_string(),
    }
}

fn focus_task(id: i64) {
    document::eval(&format!("setTimeout(() => document.querySelector('[data-task-id=\"{id}\"]')?.focus(), 0)"));
}

#[component]
pub fn TaskList(tasks: Vec<Task>, compact: bool, #[props(default = true)] reorderable: bool) -> Element {
    let mut show_done = use_signal(|| false);
    let open: Vec<Task> = tasks.iter().filter(|t| !t.done).cloned().collect();
    let done: Vec<Task> = tasks.iter().filter(|t| t.done).cloned().collect();
    // Manual order only makes sense within one day's list; elsewhere rows are not draggable.
    let order: Vec<i64> = if reorderable { open.iter().map(|t| t.id).collect() } else { Vec::new() };
    let n_done = done.len();

    rsx! {
        div { class: "task-list", "data-list": "",
            for t in open {
                TaskRow { key: "{t.id}", task: t.clone(), order: order.clone(), compact }
            }
            if n_done > 0 {
                if compact {
                    div { class: "done-summary", "{n_done} done" }
                } else {
                    button { class: "done-toggle", onclick: move |_| show_done.toggle(),
                        if show_done() { "Hide done ({n_done})" } else { "Show done ({n_done})" }
                    }
                    if show_done() {
                        for t in done {
                            TaskRow { key: "{t.id}", task: t.clone(), order: Vec::new(), compact }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TaskRow(task: Task, order: Vec<i64>, compact: bool) -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    let id = task.id;
    let is_done = task.done;
    let can_drag = !task.done && !order.is_empty();
    let expanded = !compact && EXPANDED() == Some(id);

    let drop_class = match (DRAGGING(), OVER()) {
        (Some(d), Some(o)) if o == id && d != id => {
            let from = order.iter().position(|&x| x == d);
            let to = order.iter().position(|&x| x == id);
            if from > to { " drop-above" } else { " drop-below" }
        }
        (Some(d), _) if d == id => " dragging",
        _ => "",
    };
    let class = format!(
        "task-row{}{}{}{}",
        if task.done { " done" } else { "" },
        if expanded { " expanded" } else { "" },
        if compact { " compact" } else { "" },
        drop_class
    );

    let due = task.due_date.map(|d| {
        let cls = if task.is_overdue(today) { "chip bad" } else if task.is_due_today(today) { "chip warn" } else { "chip" };
        (cls, day_label(d, today))
    });
    let carried = (!task.done && task.carried_count > 0).then_some(task.carried_count);

    // Step tasks advance one step per click; plain tasks toggle done.
    let has_steps = task.has_steps();
    let toggle = {
        let store = store.clone();
        let done = task.done;
        move || {
            let _ = store.write(|db| if has_steps && !done { tasks::step(db, id, 1) } else { tasks::set_done(db, id, !done) });
        }
    };
    let undo_step = {
        let store = store.clone();
        move || {
            let _ = store.write(|db| tasks::step(db, id, -1));
        }
    };
    let undo_k = undo_step.clone();
    let pct = (task.fraction() * 100.0).round() as i64;
    let steps_label = format!("{}/{}", if task.done { task.steps_total } else { task.steps_done }, task.steps_total);
    let delete = {
        let store = store.clone();
        move || {
            if let Ok(Some(t)) = store.write(|db| tasks::delete(db, id)) {
                if EXPANDED() == Some(id) {
                    *EXPANDED.write() = None;
                }
                ui.notify_undo(format!("Deleted “{}”", t.title), move |s| {
                    let _ = s.write(|db| tasks::restore(db, &t));
                });
            }
        }
    };
    let to_tomorrow = {
        let store = store.clone();
        move || {
            let _ = store.write(|db| tasks::reschedule(db, id, today + Duration::days(1)));
        }
    };
    let open_detail = move || {
        if compact {
            *EXPANDED.write() = Some(id);
            ui.open(View::Widget("tasks"));
        } else {
            let next = if EXPANDED() == Some(id) { None } else { Some(id) };
            *EXPANDED.write() = next;
        }
    };
    let reorder = {
        let store = store.clone();
        let order = order.clone();
        move |to: usize| {
            let new = tasks::moved(&order, id, to);
            if new != order {
                let _ = store.write(|db| tasks::reorder(db, &new));
            }
        }
    };

    let toggle_k = toggle.clone();
    let delete_k = delete.clone();
    let tomorrow_k = to_tomorrow.clone();
    let open_k = open_detail;
    let reorder_k = reorder.clone();
    let order_k = order.clone();
    let order_drop = order.clone();
    let toggle_c = toggle;
    let tomorrow_c = to_tomorrow;
    let delete_c = delete.clone();
    let open_c = open_detail;

    rsx! {
        div {
            class: "{class}",
            tabindex: 0,
            "data-row": "",
            "data-task-id": "{id}",
            draggable: can_drag,
            ondragstart: move |_| { *DRAGGING.write() = Some(id); },
            ondragend: move |_| { *DRAGGING.write() = None; *OVER.write() = None; },
            ondragover: move |e| {
                e.prevent_default();
                if DRAGGING().is_some_and(|d| order.contains(&d)) && OVER() != Some(id) {
                    *OVER.write() = Some(id);
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                let dragged = DRAGGING();
                *DRAGGING.write() = None;
                *OVER.write() = None;
                if let (Some(d), Some(to)) = (dragged, order_drop.iter().position(|&x| x == id)) {
                    if order_drop.contains(&d) && d != id {
                        let new = tasks::moved(&order_drop, d, to);
                        let _ = store.write(|db| tasks::reorder(db, &new));
                    }
                }
            },
            onkeydown: move |e| {
                let ctrl = e.modifiers().ctrl();
                match e.key() {
                    Key::Character(c) if c == " " && e.modifiers().shift() && has_steps => { e.prevent_default(); undo_k(); }
                    Key::Character(c) if c == " " => { e.prevent_default(); toggle_k(); }
                    Key::Character(c) if c == "t" && !ctrl && !is_done => { tomorrow_k(); }
                    Key::Enter => { e.prevent_default(); open_k(); }
                    Key::Delete => { delete_k(); }
                    Key::ArrowUp | Key::ArrowDown if ctrl && !is_done => {
                        e.prevent_default();
                        if let Some(i) = order_k.iter().position(|&x| x == id) {
                            let to = if e.key() == Key::ArrowUp { i.saturating_sub(1) } else { i + 1 };
                            (reorder_k.clone())(to);
                            focus_task(id);
                        }
                    }
                    _ => {}
                }
            },
            if can_drag { span { class: "grip", Icon { d: icons::GRIP } } } else { span { class: "grip-space" } }
            if has_steps && !task.done {
                button {
                    class: "check ring",
                    tabindex: -1,
                    title: "Check off a step (right-click to undo one)",
                    onclick: move |e| { e.stop_propagation(); toggle_c(); },
                    oncontextmenu: move |e| { e.prevent_default(); e.stop_propagation(); undo_step(); },
                    svg { view_box: "0 0 24 24",
                        circle { class: "ring-bg", cx: 12, cy: 12, r: 9 }
                        circle { class: "ring-fg", cx: 12, cy: 12, r: 9, "pathLength": 100, stroke_dasharray: "{pct} 100" }
                    }
                }
            } else {
                button {
                    class: "check",
                    tabindex: -1,
                    title: if task.done { "Mark not done" } else { "Mark done" },
                    onclick: move |e| { e.stop_propagation(); toggle_c(); },
                    svg { view_box: "0 0 24 24", path { d: "M6 12.5l4 4 8-9" } }
                }
            }
            div { class: "title", onclick: move |_| open_c(), "{task.title}" }
            div { class: "meta",
                if has_steps { span { class: if task.done { "chip" } else { "chip steps" }, title: "Steps done", "{steps_label} · {pct}%" } }
                if let (Some(name), Some(color)) = (&task.project_name, &task.project_color) {
                    if !compact {
                        span { class: "chip project",
                            span { class: "dot", style: "background: {color}" }
                            "{name}"
                        }
                    } else {
                        span { class: "dot", title: "{name}", style: "background: {color}" }
                    }
                }
                if let Some(n) = carried {
                    span { class: if n >= 3 { "chip warn" } else { "chip" }, title: "Carried over {n} day(s)",
                        Icon { d: icons::REFRESH } "{n}d"
                    }
                }
                if let Some((cls, label)) = due { span { class: "{cls}", title: "Due", Icon { d: icons::FLAG } "{label}" } }
            }
            if !compact && !task.done {
                div { class: "row-actions",
                    button { class: "icon-btn", tabindex: -1, title: "Move to tomorrow  (T)",
                        onclick: move |e| { e.stop_propagation(); tomorrow_c(); },
                        Icon { d: icons::ARROW_RIGHT }
                    }
                    button { class: "icon-btn", tabindex: -1, title: "Delete  (Del)",
                        onclick: move |e| { e.stop_propagation(); delete_c(); },
                        Icon { d: icons::TRASH }
                    }
                }
            }
        }
        if expanded {
            TaskDetail { key: "detail-{id}", task: task.clone(), on_delete: move |_| delete() }
        }
    }
}

#[component]
fn TaskDetail(task: Task, on_delete: EventHandler<()>) -> Element {
    let store = use_store();
    let ui = use_ui();
    let today = *ui.today.read();
    let id = task.id;
    let mut title = use_signal(|| task.title.clone());
    let mut notes = use_signal(|| task.notes.clone());
    let projects = store.read(project_options);
    let due_val = task.due_date.map(|d| d.to_string()).unwrap_or_default();
    let day_val = task.scheduled_for.to_string();
    let proj_val = task.project_id.map(|p| p.to_string()).unwrap_or_default();

    let (s1, s2, s3, s4, s5, s6) = (store.clone(), store.clone(), store.clone(), store.clone(), store.clone(), store.clone());
    let s_steps = store.clone();
    let steps_total = task.steps_total;
    let parse_date = |v: &str| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok();

    rsx! {
        div { class: "task-detail",
            input {
                class: "detail-title", value: "{title}", spellcheck: "false",
                oninput: move |e| { title.set(e.value()); let _ = s1.write(|db| tasks::set_title(db, id, &e.value())); },
                onkeydown: move |e| if e.key() == Key::Enter { *EXPANDED.write() = None; focus_task(id); },
            }
            textarea {
                class: "detail-notes", placeholder: "Notes", value: "{notes}", rows: 3,
                oninput: move |e| { notes.set(e.value()); let _ = s2.write(|db| tasks::set_notes(db, id, &e.value())); },
            }
            div { class: "detail-fields",
                label { "Steps"
                    input { r#type: "number", min: 1, max: 99, value: "{steps_total}", class: "steps-input",
                        onchange: move |e| if let Ok(n) = e.value().parse::<i64>() { let _ = s_steps.write(|db| tasks::set_steps(db, id, n)); } }
                }
                label { "Due"
                    input { r#type: "date", value: "{due_val}",
                        onchange: move |e| { let _ = s3.write(|db| tasks::set_due(db, id, parse_date(&e.value()))); } }
                }
                label { "On list"
                    input { r#type: "date", value: "{day_val}",
                        onchange: move |e| if let Some(d) = parse_date(&e.value()) { let _ = s4.write(|db| tasks::reschedule(db, id, d)); } }
                }
                label { "Project"
                    select {
                        onchange: move |e| { let _ = s5.write(|db| tasks::set_project(db, id, e.value().parse().ok())); },
                        option { value: "", selected: proj_val.is_empty(), "None" }
                        for (pid, name, _) in projects {
                            option { value: "{pid}", selected: proj_val == pid.to_string(), "{name}" }
                        }
                    }
                }
                div { class: "grow" }
                if task.scheduled_for <= today && !task.done {
                    button { class: "btn", onclick: move |_| { let _ = s6.write(|db| tasks::reschedule(db, id, today + Duration::days(1))); }, "Tomorrow" }
                }
                button { class: "btn danger", onclick: move |_| on_delete.call(()), "Delete" }
            }
        }
    }
}

/// Inline "add a task" field. Accepts the quick-add tokens (!due, @day, #project).
#[component]
pub fn AddTask(compact: bool, #[props(default)] project: Option<i64>) -> Element {
    let store = use_store();
    let ui = use_ui();
    let mut text = use_signal(String::new);
    let today = *ui.today.read();

    let hint = match parse(&text.read(), today) {
        Some(QuickAdd::Task(t)) => {
            let mut parts = Vec::new();
            if t.scheduled_for != today {
                parts.push(day_label(t.scheduled_for, today));
            }
            if let Some(d) = t.due {
                parts.push(format!("due {}", day_label(d, today).to_lowercase()));
            }
            if t.steps > 1 {
                parts.push(format!("{} steps", t.steps));
            }
            if let Some(q) = &t.project_query {
                parts.push(store.peek(|db| find_project(db, q)).map(|p| p.1).unwrap_or_else(|| format!("no project “{q}”")));
            }
            parts.join(" · ")
        }
        _ => String::new(),
    };

    rsx! {
        div { class: if compact { "add-task compact" } else { "add-task" },
            Icon { d: icons::PLUS }
            input {
                placeholder: "Add task",
                spellcheck: "false",
                value: "{text}",
                oninput: move |e| text.set(e.value()),
                onkeydown: move |e| {
                    if e.key() != Key::Enter { return; }
                    let raw = text.peek().clone();
                    // Everything typed here is a task, even if it starts with "n ".
                    let draft = match parse(&raw, today) {
                        Some(QuickAdd::Task(t)) => Some(t),
                        Some(QuickAdd::Note(_) | QuickAdd::Event(_)) => match parse(&format!("_ {raw}"), today) {
                            Some(QuickAdd::Task(mut t)) => { t.title = t.title[2..].to_string(); Some(t) }
                            _ => None,
                        },
                        None => None,
                    };
                    let Some(t) = draft else { return };
                    let project = t.project_query.as_deref().and_then(|q| store.peek(|db| find_project(db, q))).map(|p| p.0).or(project);
                    match store.write(|db| tasks::add_steps(db, &t.title, t.scheduled_for, t.due, project, t.steps)) {
                        Ok(_) => {
                            text.set(String::new());
                            if t.scheduled_for != today {
                                ui.notify(format!("Added to {}", day_label(t.scheduled_for, today)));
                            }
                        }
                        Err(err) => ui.error(format!("Could not add task: {err}")),
                    }
                },
            }
            if !hint.is_empty() { span { class: "add-hint", "{hint}" } }
        }
    }
}
