//! Conversation state and the request/tool loop.
//!
//! Rules the API imposes, and how this keeps them:
//! - History is append-only: assistant turns are stored exactly as streamed (thinking blocks
//!   included) and never edited. The only merge is into a trailing user message that has not
//!   received a reply yet.
//! - System prompt and tools never change mid-conversation; live data and "you applied X"
//!   notes travel inside user messages as a `<context>` block, re-sent only when they change.
//! - Every `tool_use` gets a `tool_result` in the very next user message.

use crate::state::Ui;
use crate::store::Store;
use cc_core::ai::{self, api, Change};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use dioxus::prelude::*;

/// Most model round-trips per user message (tool calls included).
const MAX_STEPS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Pending,
    Applied(String),
    Skipped,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    User(String),
    Assistant { text: String, streaming: bool },
    Proposal { summary: String, changes: Vec<(Change, Status)> },
    Notice { text: String, error: bool },
}

pub static TRANSCRIPT: GlobalSignal<Vec<Item>> = Signal::global(Vec::new);
pub static BUSY: GlobalSignal<bool> = Signal::global(|| false);
/// What the reply is doing right now ("Thinking", "Preparing suggestions").
pub static ACTIVITY: GlobalSignal<Option<&'static str>> = Signal::global(|| None);
pub static USAGE: GlobalSignal<api::Usage> = Signal::global(api::Usage::default);
/// Model this conversation runs on (fixed when it starts).
pub static MODEL: GlobalSignal<Option<String>> = Signal::global(|| None);
/// The last request failed; "Retry" re-sends without a new message.
pub static CAN_RETRY: GlobalSignal<bool> = Signal::global(|| false);

static MESSAGES: GlobalSignal<Vec<Value>> = Signal::global(Vec::new);
static NOTES: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
static LAST_SNAPSHOT: GlobalSignal<Option<u64>> = Signal::global(|| None);
static CANCEL: GlobalSignal<Option<Arc<AtomicBool>>> = Signal::global(|| None);

pub fn model_setting(store: &Store) -> String {
    store.setting("ai.model").unwrap_or_else(|| api::DEFAULT_MODEL.to_string())
}

pub fn new_chat() {
    if *BUSY.peek() {
        stop();
    }
    TRANSCRIPT.write().clear();
    MESSAGES.write().clear();
    NOTES.write().clear();
    *LAST_SNAPSHOT.write() = None;
    *USAGE.write() = api::Usage::default();
    *MODEL.write() = None;
    *CAN_RETRY.write() = false;
}

pub fn stop() {
    if let Some(c) = CANCEL.peek().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

/// Append blocks as user content, merging into a trailing unanswered user message.
fn push_user(blocks: Vec<Value>) {
    let mut msgs = MESSAGES.write();
    if let Some(last) = msgs.last_mut().filter(|m| m["role"] == "user") {
        if let Some(arr) = last["content"].as_array_mut() {
            arr.extend(blocks);
            return;
        }
    }
    msgs.push(json!({ "role": "user", "content": blocks }));
}

/// Send a user message (with fresh context if anything changed) and run the reply loop.
pub fn send(store: &Store, ui: Ui, text: &str) {
    let text = text.trim().to_string();
    if text.is_empty() || *BUSY.peek() {
        return;
    }
    let h12 = *ui.h12.peek();
    let now = chrono::Local::now().naive_local();
    let snap = store.peek(|db| ai::snapshot(db, now, h12));
    // Hash without the clock line so the snapshot is only re-sent when the data changes.
    let mut hasher = DefaultHasher::new();
    snap.lines().skip(1).collect::<Vec<_>>().hash(&mut hasher);
    let hash = hasher.finish();
    let notes: Vec<String> = std::mem::take(&mut *NOTES.write());

    let mut ctx = String::new();
    if *LAST_SNAPSHOT.peek() != Some(hash) {
        ctx.push_str(&snap);
        *LAST_SNAPSHOT.write() = Some(hash);
    } else {
        ctx.push_str(snap.lines().next().unwrap_or_default());
        ctx.push_str("\n(Data unchanged since the last snapshot.)\n");
    }
    if !notes.is_empty() {
        ctx.push_str("\nSince your last reply the user:\n");
        for n in &notes {
            ctx.push_str(&format!("- {n}\n"));
        }
    }
    push_user(vec![
        json!({ "type": "text", "text": format!("<context>\n{ctx}</context>") }),
        json!({ "type": "text", "text": text }),
    ]);
    TRANSCRIPT.write().push(Item::User(text));
    if MODEL.peek().is_none() {
        *MODEL.write() = Some(model_setting(store));
    }
    run(store.clone(), ui);
}

pub fn retry(store: &Store, ui: Ui) {
    if !*BUSY.peek() && *CAN_RETRY.peek() {
        run(store.clone(), ui);
    }
}

enum Msg {
    Event(api::StreamEvent),
    Done(Result<api::Reply, api::ApiError>),
}

fn run(store: Store, ui: Ui) {
    let Some((key, _)) = super::key::load() else {
        TRANSCRIPT.write().push(Item::Notice { text: "Add your Anthropic API key in Settings → Claude assistant.".into(), error: true });
        return;
    };
    *BUSY.write() = true;
    *CAN_RETRY.write() = false;
    let model = MODEL.peek().clone().unwrap_or_else(|| api::DEFAULT_MODEL.into());
    spawn(async move {
        for _ in 0..MAX_STEPS {
            let body = api::request_body(&model, &MESSAGES.peek());
            let cancel = Arc::new(AtomicBool::new(false));
            *CANCEL.write() = Some(cancel.clone());
            let (tx, mut rx) = futures_channel::mpsc::unbounded::<Msg>();
            let key = key.clone();
            std::thread::spawn(move || {
                let tx2 = tx.clone();
                let r = api::stream_message(&key, &body, &cancel, |ev| {
                    let _ = tx2.unbounded_send(Msg::Event(ev));
                });
                let _ = tx.unbounded_send(Msg::Done(r));
            });

            TRANSCRIPT.write().push(Item::Assistant { text: String::new(), streaming: true });
            *ACTIVITY.write() = Some("Thinking");
            let mut result = None;
            while let Some(m) = rx.next().await {
                match m {
                    Msg::Event(api::StreamEvent::Text(t)) => {
                        *ACTIVITY.write() = None;
                        if let Some(Item::Assistant { text, .. }) = TRANSCRIPT.write().last_mut() {
                            text.push_str(&t);
                        }
                    }
                    Msg::Event(api::StreamEvent::Thinking) => {
                        if ACTIVITY.peek().is_none() {
                            *ACTIVITY.write() = Some("Thinking");
                        }
                    }
                    Msg::Event(api::StreamEvent::ToolStarted(name)) => {
                        *ACTIVITY.write() = Some(if name == "propose_changes" { "Preparing suggestions" } else { "Reading notes" });
                    }
                    Msg::Done(r) => result = Some(r),
                }
            }
            *ACTIVITY.write() = None;
            finish_streaming_item();

            let reply = match result {
                Some(Ok(r)) => r,
                Some(Err(e)) => {
                    let cancelled = e.kind == api::ErrorKind::Cancelled;
                    TRANSCRIPT.write().push(Item::Notice { text: e.user_message(), error: !cancelled });
                    *CAN_RETRY.write() = true;
                    break;
                }
                None => break,
            };
            USAGE.write().add(&reply.usage);
            MESSAGES.write().push(json!({ "role": "assistant", "content": reply.content.clone() }));

            let tool_uses: Vec<(usize, Value)> = reply
                .content
                .iter()
                .enumerate()
                .filter(|(_, b)| b["type"] == "tool_use")
                .map(|(i, b)| (i, b.clone()))
                .collect();
            match reply.stop_reason.as_str() {
                "tool_use" => {
                    let results: Vec<Value> = tool_uses.iter().map(|(i, b)| run_tool(&store, ui, b, reply.bad_tool_inputs.contains(i))).collect();
                    push_user(results);
                    continue;
                }
                stop => {
                    // A reply cut off mid-tool-call still needs results; nothing is run.
                    if !tool_uses.is_empty() {
                        push_user(tool_uses.iter().map(|(_, b)| tool_result(b, "Not run: the reply ended before the call was complete.", true)).collect());
                    }
                    match stop {
                        "max_tokens" => notice("The reply hit its length limit and was cut off.", true),
                        "refusal" => notice("Claude declined this request.", true),
                        _ => {}
                    }
                    break;
                }
            }
        }
        *BUSY.write() = false;
        *CANCEL.write() = None;
    });
}

fn notice(text: &str, error: bool) {
    TRANSCRIPT.write().push(Item::Notice { text: text.into(), error });
}

/// Close the streaming bubble; drop it if no text arrived (e.g. a tool-only turn).
fn finish_streaming_item() {
    let mut t = TRANSCRIPT.write();
    if let Some(Item::Assistant { text, streaming }) = t.last_mut() {
        *streaming = false;
        if text.trim().is_empty() {
            t.pop();
        }
    }
}

fn tool_result(block: &Value, content: &str, is_error: bool) -> Value {
    let mut r = json!({ "type": "tool_result", "tool_use_id": block["id"].clone(), "content": content });
    if is_error {
        r["is_error"] = json!(true);
    }
    r
}

fn run_tool(store: &Store, _ui: Ui, block: &Value, bad_input: bool) -> Value {
    if bad_input {
        return tool_result(block, "INVALID_JSON: the input wasn't complete, valid JSON. Call the tool again.", true);
    }
    let input = &block["input"];
    match block["name"].as_str().unwrap_or("") {
        "propose_changes" => match ai::parse_proposal(input) {
            Ok(p) => {
                let n = p.changes.len();
                TRANSCRIPT.write().push(Item::Proposal {
                    summary: p.summary,
                    changes: p.changes.into_iter().map(|c| (c, Status::Pending)).collect(),
                });
                tool_result(
                    block,
                    &format!("Shown to the user as {n} proposed change(s). They will apply or skip each one; the next context block says what they decided. Don't assume anything was applied."),
                    false,
                )
            }
            Err(e) => tool_result(block, &format!("{e:#}"), true),
        },
        "read_note" => match store.peek(|db| ai::read_note(db, input)) {
            Ok(text) => tool_result(block, &text, false),
            Err(e) => tool_result(block, &format!("{e:#}"), true),
        },
        other => tool_result(block, &format!("Unknown tool {other}"), true),
    }
}

/// Apply or skip one proposed change (`item` = transcript index, `idx` = change index).
pub fn decide(store: &Store, ui: Ui, item: usize, idx: usize, apply: bool) {
    let change = match TRANSCRIPT.peek().get(item) {
        Some(Item::Proposal { changes, .. }) => match changes.get(idx) {
            Some((c, Status::Pending)) => c.clone(),
            _ => return,
        },
        _ => return,
    };
    let h12 = *ui.h12.peek();
    let today = *ui.today.peek();
    let status = if apply {
        match store.write(|db| ai::apply(db, &change, today)) {
            Ok(msg) => {
                NOTES.write().push(format!("applied: {msg}"));
                Status::Applied(msg)
            }
            Err(e) => {
                NOTES.write().push(format!("tried to apply \"{}\" but it failed: {e}", store.peek(|db| ai::describe(db, &change, h12))));
                Status::Failed(e.to_string())
            }
        }
    } else {
        NOTES.write().push(format!("skipped: {}", store.peek(|db| ai::describe(db, &change, h12))));
        Status::Skipped
    };
    if let Some(Item::Proposal { changes, .. }) = TRANSCRIPT.write().get_mut(item) {
        changes[idx].1 = status;
    }
}

/// Apply or skip every change still pending in a proposal.
pub fn decide_all(store: &Store, ui: Ui, item: usize, apply: bool) {
    let n = match TRANSCRIPT.peek().get(item) {
        Some(Item::Proposal { changes, .. }) => changes.len(),
        _ => 0,
    };
    for i in 0..n {
        decide(store, ui, item, i, apply);
    }
}
