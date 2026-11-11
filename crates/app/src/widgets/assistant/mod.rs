//! Claude assistant: chat about your plans; Claude proposes changes, you apply them.

pub mod chat;
pub mod key;

use super::WidgetDef;
use crate::icons::{self, Icon};
use crate::state::{use_ui, Action, Command, Ui, View};
use crate::store::{use_store, Store};
use cc_core::ai::{self, api};
use chat::{Item, Status, ACTIVITY, BUSY, CAN_RETRY, MODEL, TRANSCRIPT, USAGE};
use dioxus::prelude::*;

pub static DEF: WidgetDef = WidgetDef {
    id: "assistant",
    title: "Claude",
    icon: icons::SPARKLE,
    cols: 4,
    rows: 2,
    card,
    full,
    commands,
    search: None,
    open_item: None,
    on_message: None,
};

/// Text waiting in the composer (kept while you switch views).
static DRAFT: GlobalSignal<String> = Signal::global(String::new);

const STARTERS: [&str; 4] = [
    "Plan my day",
    "Plan my week around my events",
    "What's overdue or slipping, and what should I drop?",
    "Help me fit editing time around my deadlines",
];

fn ask(_: &Store, ui: Ui) {
    ui.open(View::Widget("assistant"));
}

fn new_chat(_: &Store, ui: Ui) {
    chat::new_chat();
    ui.open(View::Widget("assistant"));
}

fn card() -> Element {
    let store = use_store();
    let ui = use_ui();
    let last = TRANSCRIPT.read().iter().rev().find_map(|i| match i {
        Item::Assistant { text, .. } => Some(text.clone()),
        _ => None,
    });
    let mut text = use_signal(String::new);
    rsx! {
        div { class: "ai-card",
            div { class: "ai-card-input",
                Icon { d: icons::SPARKLE }
                input {
                    placeholder: "Ask Claude",
                    value: "{text}",
                    oninput: move |e| text.set(e.value()),
                    onkeydown: move |e| if e.key() == Key::Enter {
                        let t = text.peek().clone();
                        text.set(String::new());
                        ui.open(View::Widget("assistant"));
                        chat::send(&store, ui, &t);
                    },
                }
            }
            match last {
                Some(t) => rsx! { div { class: "ai-card-last md", dangerous_inner_html: "{cc_core::markdown::to_html(&t)}" } },
                None => rsx! { div { class: "muted small", "No conversation yet" } },
            }
        }
    }
}

fn full() -> Element {
    let has_key = use_signal(|| key::load().is_some());
    if !has_key() {
        return rsx! { KeySetup { on_saved: move |_| { let mut k = has_key; k.set(true); } } };
    }
    rsx! { Chat {} }
}

#[component]
fn KeySetup(on_saved: EventHandler<()>) -> Element {
    let mut value = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut save = move || {
        let v = value.peek().clone();
        match key::save(&v) {
            Ok(()) => {
                value.set(String::new());
                on_saved.call(());
            }
            Err(e) => error.set(Some(e)),
        }
    };
    let mut save_k = save.clone();
    rsx! {
        div { class: "full",
            div { class: "panel ai-setup",
                div { class: "ai-setup-icon", Icon { d: icons::SPARKLE } }
                h3 { "Connect Claude" }
                p { class: "muted", "Paste an Anthropic API key from console.anthropic.com. It's stored in Windows Credential Manager, not in your data or backups." }
                div { class: "ai-setup-row",
                    input { class: "text-input", r#type: "password", placeholder: "sk-ant-…", value: "{value}", spellcheck: "false",
                        oninput: move |e| value.set(e.value()),
                        onkeydown: move |e| if e.key() == Key::Enter { save_k(); } }
                    button { class: "btn primary", onclick: move |_| save(), "Save key" }
                }
                if let Some(e) = error() { div { class: "chip warn", "{e}" } }
            }
        }
    }
}

#[component]
fn Chat() -> Element {
    let store = use_store();
    let ui = use_ui();
    let items = TRANSCRIPT.read().clone();
    let busy = BUSY();
    let activity = ACTIVITY();
    let usage = USAGE();
    let model = api::model_info(&MODEL().unwrap_or_else(|| chat::model_setting(&store)));
    let cost = usage.cost(&model);
    let tokens = usage.input + usage.output + usage.cache_read + usage.cache_write;
    let (s_send, s_retry, s_chip, s_click) = (store.clone(), store.clone(), store.clone(), store.clone());

    // Keep the newest message in view.
    let count = items.len();
    let last_len = match items.last() {
        Some(Item::Assistant { text, .. }) => text.len(),
        _ => 0,
    };
    use_effect(use_reactive!(|(count, last_len)| {
        let _ = (count, last_len);
        document::eval("const t = document.getElementById('ai-thread'); if (t) t.scrollTop = t.scrollHeight;");
    }));

    rsx! {
        div { class: "ai-full",
            div { class: "ai-head",
                Icon { d: icons::SPARKLE }
                span { class: "ai-model", "{model.label}" }
                if tokens > 0 {
                    span { class: "muted small", title: "Tokens this chat (cached reads are billed at a tenth of the input price)",
                        "{fmt_k(tokens)} tokens ({fmt_k(usage.cache_read)} cached) · ≈ ${cost:.2}" }
                }
                div { class: "grow" }
                button { class: "btn", onclick: move |_| new_chat(&store, ui), Icon { d: icons::PLUS } " New chat" }
            }
            div { class: "ai-thread", id: "ai-thread",
                if items.is_empty() {
                    div { class: "ai-empty",
                        div { class: "ai-empty-icon", Icon { d: icons::SPARKLE } }
                        div { class: "ai-starters",
                            for s in STARTERS {
                                button { class: "ai-starter", onclick: { let st = s_chip.clone(); move |_| chat::send(&st, ui, s) }, "{s}" }
                            }
                        }
                    }
                }
                for (i, item) in items.into_iter().enumerate() {
                    {
                        match item {
                            Item::User(t) => rsx! { div { key: "{i}", class: "ai-msg user", "{t}" } },
                            Item::Assistant { text, streaming } => rsx! {
                                div { key: "{i}", class: if streaming { "ai-msg assistant streaming" } else { "ai-msg assistant" },
                                    div { class: "md", dangerous_inner_html: "{cc_core::markdown::to_html(&text)}" }
                                }
                            },
                            Item::Proposal { summary, changes } => rsx! { ProposalCard { key: "{i}", item: i, summary, changes } },
                            Item::Notice { text, error } => rsx! { div { key: "{i}", class: if error { "ai-notice error" } else { "ai-notice" }, "{text}" } },
                        }
                    }
                }
                if let Some(a) = activity {
                    div { class: "ai-activity", span { class: "ai-dots" } "{a}…" }
                }
            }
            div { class: "ai-composer",
                textarea {
                    class: "ai-input",
                    rows: 2,
                    placeholder: "Ask Claude to plan, sort or reschedule",
                    value: "{DRAFT}",
                    onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                    oninput: move |e| *DRAFT.write() = e.value(),
                    onkeydown: {
                        let st = s_send.clone();
                        move |e: KeyboardEvent| if e.key() == Key::Enter && !e.modifiers().shift() {
                            e.prevent_default();
                            let t = DRAFT.peek().clone();
                            if !t.trim().is_empty() && !BUSY() { DRAFT.write().clear(); chat::send(&st, ui, &t); }
                        }
                    },
                }
                if busy {
                    button { class: "btn", onclick: move |_| chat::stop(), "Stop" }
                } else {
                    if CAN_RETRY() {
                        button { class: "btn", onclick: move |_| chat::retry(&s_retry, ui), "Retry" }
                    }
                    button { class: "btn primary",
                        onclick: move |_| { let t = DRAFT.peek().clone(); DRAFT.write().clear(); chat::send(&s_click, ui, &t); },
                        "Send"
                    }
                }
            }
        }
    }
}

#[component]
fn ProposalCard(item: usize, summary: String, changes: Vec<(ai::Change, Status)>) -> Element {
    let store = use_store();
    let ui = use_ui();
    let h12 = *ui.h12.read();
    let pending = changes.iter().filter(|(_, s)| *s == Status::Pending).count();
    let (s_all, s_skip) = (store.clone(), store.clone());
    rsx! {
        div { class: "ai-proposal",
            div { class: "ai-prop-head",
                span { class: "ai-prop-title", "{summary}" }
                div { class: "grow" }
                if pending > 1 {
                    button { class: "btn", onclick: move |_| chat::decide_all(&s_skip, ui, item, false), "Skip all" }
                    button { class: "btn primary", onclick: move |_| chat::decide_all(&s_all, ui, item, true), "Apply all {pending}" }
                }
            }
            for (idx, (c, status)) in changes.into_iter().enumerate() {
                {
                    let text = store.read(|db| ai::describe(db, &c, h12));
                    let st = store.clone();
                    let st2 = store.clone();
                    rsx! {
                        div { key: "{idx}", class: "ai-change",
                            div { class: "ai-change-main",
                                span { class: "ai-change-text", "{text}" }
                                if let Some(r) = &c.reason { span { class: "muted small", "{r}" } }
                            }
                            match status {
                                Status::Pending => rsx! {
                                    button { class: "btn", onclick: move |_| chat::decide(&st2, ui, item, idx, false), "Skip" }
                                    button { class: "btn primary", onclick: move |_| chat::decide(&st, ui, item, idx, true), "Apply" }
                                },
                                Status::Applied(_) => rsx! { span { class: "chip ok", "Applied" } },
                                Status::Skipped => rsx! { span { class: "chip", "Skipped" } },
                                Status::Failed(e) => rsx! { span { class: "chip warn", title: "{e}", "Failed" } },
                            }
                        }
                    }
                }
            }
        }
    }
}

fn fmt_k(n: u64) -> String {
    if n >= 1000 { format!("{:.1}k", n as f64 / 1000.0) } else { n.to_string() }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("Go to", icons::SPARKLE, "Ask Claude", Action::Run(ask)).keywords("assistant ai plan chat").shortcut("Ctrl+7"),
        Command::new("Create", icons::SPARKLE, "New chat with Claude", Action::Run(new_chat)).keywords("assistant ai"),
    ]
}

/// Settings panel section: API key and model.
#[component]
pub fn AssistantSettings() -> Element {
    let store = use_store();
    let mut loaded = use_signal(key::load);
    let mut value = use_signal(String::new);
    let mut msg = use_signal(|| None::<(String, bool)>);
    let current = chat::model_setting(&store);
    let s_model = store.clone();
    rsx! {
        div { class: "panel",
            h3 { "Claude assistant" }
            div { class: "row",
                span { class: "label", "API key" }
                div { class: "value",
                    match loaded() {
                        Some((k, key::Source::CredentialManager)) => rsx! { span { class: "path", "{key::masked(&k)}" } span { class: "muted small", "Windows Credential Manager" } },
                        Some((k, key::Source::Environment)) => rsx! { span { class: "path", "{key::masked(&k)}" } span { class: "muted small", "from ANTHROPIC_API_KEY" } },
                        None => rsx! { span { class: "muted", "Not set" } },
                    }
                }
            }
            div { class: "row",
                span { class: "label", "" }
                div { class: "value",
                    input { class: "text-input", r#type: "password", placeholder: "sk-ant-…", value: "{value}", spellcheck: "false", oninput: move |e| value.set(e.value()) }
                    button { class: "btn", onclick: move |_| {
                        let v = value.peek().clone();
                        match key::save(&v) {
                            Ok(()) => { value.set(String::new()); loaded.set(key::load()); msg.set(Some(("Saved".into(), false))); }
                            Err(e) => msg.set(Some((e, true))),
                        }
                    }, "Save" }
                    if matches!(loaded(), Some((_, key::Source::CredentialManager))) {
                        button { class: "btn danger", onclick: move |_| {
                            match key::remove() { Ok(()) => { loaded.set(key::load()); msg.set(Some(("Removed".into(), false))); } Err(e) => msg.set(Some((e, true))) }
                        }, "Remove" }
                    }
                    if let Some((m, err)) = msg() { span { class: if err { "chip warn" } else { "chip ok" }, "{m}" } }
                }
            }
            div { class: "row",
                span { class: "label", "Model" }
                div { class: "value",
                    div { class: "seg",
                        for m in api::MODELS {
                            button {
                                class: if current == m.id { "on" },
                                title: "${m.input} / ${m.output} per million input / output tokens",
                                onclick: { let s = s_model.clone(); move |_| s.set_setting("ai.model", m.id) },
                                "{m.label}"
                            }
                        }
                    }
                    span { class: "muted small", "applies to new chats" }
                }
            }
        }
    }
}
