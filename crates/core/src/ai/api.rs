//! Claude Messages API over raw HTTP (there is no official Rust SDK): request building,
//! server-sent-event parsing, error mapping and retries.
//!
//! The assistant streams (`"stream": true`) so text appears as it is written. Every content
//! block is rebuilt exactly from the stream and sent back unchanged on the next request -
//! thinking blocks must be replayed verbatim, and the conversation is append-only.

use super::{tools, SYSTEM_PROMPT};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";

/// The endpoint, overridable with `COMMAND_CENTER_API_URL` (used to test against a local mock).
pub fn api_url() -> String {
    std::env::var("COMMAND_CENTER_API_URL").unwrap_or_else(|_| API_URL.to_string())
}
pub const API_VERSION: &str = "2026-10-21";
/// Server-side refusal fallback: if a safety classifier declines, the API retries on a
/// suitable model inside the same call (`"fallbacks": "default"`).
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
pub const MAX_TOKENS: u32 = 64_000;
pub const EFFORT: &str = "medium";

/// Models offered in Settings, with list prices per million tokens (input, output,
/// cache read, cache write).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

pub const MODELS: [ModelInfo; 2] = [
    ModelInfo { id: "claude-opus-5-5", label: "Claude Opus 5.5", input: 4.0, output: 20.0, cache_read: 0.20, cache_write: 5.0 },
    ModelInfo { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5", input: 2.0, output: 10.0, cache_read: 0.10, cache_write: 2.5 },
];
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

pub fn model_info(id: &str) -> ModelInfo {
    MODELS.iter().copied().find(|m| m.id == id).unwrap_or(MODELS[0])
}

pub fn request_body(model: &str, messages: &[Value]) -> Value {
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "system": [{ "type": "text", "text": SYSTEM_PROMPT }],
        "tools": tools(),
        "output_config": { "effort": EFFORT },
        // Automatic prompt caching: caches the prompt up to the newest message each turn.
        "cache_control": { "type": "ephemeral" },
        "fallbacks": "default",
        "messages": messages,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
    }
    /// Estimated cost in US dollars at list prices.
    pub fn cost(&self, m: &ModelInfo) -> f64 {
        (self.input as f64 * m.input + self.output as f64 * m.output + self.cache_read as f64 * m.cache_read + self.cache_write as f64 * m.cache_write) / 1e6
    }
    fn merge_json(&mut self, u: &Value) {
        let g = |k: &str| u.get(k).and_then(Value::as_u64);
        if let Some(v) = g("input_tokens") { self.input = v; }
        if let Some(v) = g("output_tokens") { self.output = v; }
        if let Some(v) = g("cache_read_input_tokens") { self.cache_read = v; }
        if let Some(v) = g("cache_creation_input_tokens") { self.cache_write = v; }
    }
}

/// Progress reported while a reply streams.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Text(String),
    Thinking,
    ToolStarted(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    Auth,
    Permission,
    Billing,
    NotFound,
    RateLimited,
    Overloaded,
    Server,
    BadRequest,
    Network,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ApiError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        ApiError { kind, message: message.into() }
    }
    pub fn retryable(&self) -> bool {
        matches!(self.kind, ErrorKind::RateLimited | ErrorKind::Overloaded | ErrorKind::Server | ErrorKind::Network)
    }
    /// Friendly text for the chat.
    pub fn user_message(&self) -> String {
        match self.kind {
            ErrorKind::Auth => "Your API key was rejected. Check it in Settings → Claude assistant.".into(),
            ErrorKind::Permission => format!("This API key isn't allowed to do that: {}", self.message),
            ErrorKind::Billing => "Your Anthropic account is out of credit or has a billing problem (console.anthropic.com → Billing).".into(),
            ErrorKind::NotFound => format!("Model not available to this key: {}", self.message),
            ErrorKind::RateLimited => "Rate limit reached. Wait a moment and try again.".into(),
            ErrorKind::Overloaded => "Claude is overloaded right now. Try again in a minute.".into(),
            ErrorKind::Server => format!("Anthropic's API had an error: {}", self.message),
            ErrorKind::BadRequest => format!("The request was rejected: {}", self.message),
            ErrorKind::Network => format!("Couldn't reach the API: {}", self.message),
            ErrorKind::Cancelled => "Stopped.".into(),
        }
    }
}

fn kind_for(status: u16, err_type: &str) -> ErrorKind {
    match (status, err_type) {
        (_, "authentication_error") | (401, _) => ErrorKind::Auth,
        (_, "permission_error") | (403, _) => ErrorKind::Permission,
        (_, "billing_error") | (402, _) => ErrorKind::Billing,
        (_, "not_found_error") | (404, _) => ErrorKind::NotFound,
        (_, "rate_limit_error") | (429, _) => ErrorKind::RateLimited,
        (_, "overloaded_error") | (529, _) => ErrorKind::Overloaded,
        (s, _) if s >= 500 => ErrorKind::Server,
        (_, "api_error") => ErrorKind::Server,
        _ => ErrorKind::BadRequest,
    }
}

/// Parse an error body `{"type":"error","error":{"type":..,"message":..}}`.
pub fn parse_error(status: u16, body: &str) -> ApiError {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let e = v.get("error").cloned().unwrap_or(Value::Null);
    let t = e.get("type").and_then(Value::as_str).unwrap_or("");
    let msg = e.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"));
    ApiError::new(kind_for(status, t), msg)
}

/// The finished reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    /// Content blocks exactly as produced (thinking, text, tool_use, fallback, …).
    pub content: Vec<Value>,
    pub stop_reason: String,
    pub usage: Usage,
    pub model: String,
    /// Indexes of tool_use blocks whose streamed input wasn't valid JSON.
    pub bad_tool_inputs: Vec<usize>,
}

/// Rebuilds a message from stream events.
#[derive(Default)]
pub struct Accumulator {
    blocks: Vec<Value>,
    partial: HashMap<usize, String>,
    stop_reason: Option<String>,
    usage: Usage,
    model: String,
    bad: Vec<usize>,
    error: Option<ApiError>,
    done: bool,
}

impl Accumulator {
    /// Handle one SSE event; returns progress to show, if any.
    pub fn handle(&mut self, event: &str, data: &str) -> Option<StreamEvent> {
        let v: Value = serde_json::from_str(data).ok()?;
        let idx = || v.get("index").and_then(Value::as_u64).map(|i| i as usize);
        match event {
            "message_start" => {
                let m = &v["message"];
                self.model = m.get("model").and_then(Value::as_str).unwrap_or_default().to_string();
                self.usage.merge_json(&m["usage"]);
                None
            }
            "content_block_start" => {
                let i = idx()?;
                let block = v["content_block"].clone();
                while self.blocks.len() <= i {
                    self.blocks.push(Value::Null);
                }
                let kind = block.get("type").and_then(Value::as_str).unwrap_or("").to_string();
                let name = block.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                self.blocks[i] = block;
                match kind.as_str() {
                    "thinking" | "redacted_thinking" => Some(StreamEvent::Thinking),
                    "tool_use" => Some(StreamEvent::ToolStarted(name)),
                    _ => None,
                }
            }
            "content_block_delta" => {
                let i = idx()?;
                let delta = &v["delta"];
                let block = self.blocks.get_mut(i)?;
                let append = |block: &mut Value, field: &str, s: &str| {
                    let cur = block.get(field).and_then(Value::as_str).unwrap_or("").to_string();
                    block[field] = Value::String(cur + s);
                };
                match delta.get("type").and_then(Value::as_str)? {
                    "text_delta" => {
                        let s = delta["text"].as_str().unwrap_or("");
                        append(block, "text", s);
                        Some(StreamEvent::Text(s.to_string()))
                    }
                    "thinking_delta" => {
                        append(block, "thinking", delta["thinking"].as_str().unwrap_or(""));
                        Some(StreamEvent::Thinking)
                    }
                    "signature_delta" => {
                        block["signature"] = delta["signature"].clone();
                        None
                    }
                    "input_json_delta" => {
                        self.partial.entry(i).or_default().push_str(delta["partial_json"].as_str().unwrap_or(""));
                        None
                    }
                    "citations_delta" => {
                        if let Some(arr) = block.get_mut("citations").and_then(Value::as_array_mut) {
                            arr.push(delta["citation"].clone());
                        } else {
                            block["citations"] = json!([delta["citation"].clone()]);
                        }
                        None
                    }
                    _ => None,
                }
            }
            "content_block_stop" => {
                let i = idx()?;
                if let Some(raw) = self.partial.remove(&i) {
                    let parsed = if raw.trim().is_empty() { Ok(json!({})) } else { serde_json::from_str::<Value>(&raw) };
                    match parsed {
                        Ok(input) if input.is_object() => self.blocks[i]["input"] = input,
                        _ => {
                            self.blocks[i]["input"] = json!({});
                            self.bad.push(i);
                        }
                    }
                }
                None
            }
            "message_delta" => {
                if let Some(r) = v["delta"].get("stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(r.to_string());
                }
                self.usage.merge_json(&v["usage"]);
                None
            }
            "message_stop" => {
                self.done = true;
                None
            }
            "error" => {
                let e = &v["error"];
                let t = e.get("type").and_then(Value::as_str).unwrap_or("");
                self.error = Some(ApiError::new(kind_for(0, t), e.get("message").and_then(Value::as_str).unwrap_or("stream error")));
                None
            }
            _ => None,
        }
    }

    pub fn finish(self) -> Result<Reply, ApiError> {
        if let Some(e) = self.error {
            return Err(e);
        }
        if !self.done {
            return Err(ApiError::new(ErrorKind::Network, "the connection closed before the reply finished"));
        }
        Ok(Reply {
            content: self.blocks.into_iter().filter(|b| !b.is_null()).collect(),
            stop_reason: self.stop_reason.unwrap_or_else(|| "end_turn".into()),
            usage: self.usage,
            model: self.model,
            bad_tool_inputs: self.bad,
        })
    }
}

/// Feed SSE lines; calls `f(event, data)` at each blank-line boundary.
#[derive(Default)]
pub struct SseParser {
    event: String,
    data: String,
}

impl SseParser {
    pub fn line(&mut self, line: &str, mut f: impl FnMut(&str, &str)) {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !self.data.is_empty() {
                let ev = if self.event.is_empty() { "message" } else { &self.event };
                f(ev, &self.data);
            }
            self.event.clear();
            self.data.clear();
        } else if let Some(v) = line.strip_prefix("event:") {
            self.event = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("data:") {
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(v.strip_prefix(' ').unwrap_or(v));
        }
    }
}

/// Send one streaming request (with up to two retries for transient failures before any
/// output arrives). Blocking: call from a background thread.
pub fn stream_message(
    api_key: &str,
    body: &Value,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(StreamEvent),
) -> Result<Reply, ApiError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(120)))
        .build()
        .into();
    let mut attempt = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ApiError::new(ErrorKind::Cancelled, "stopped"));
        }
        let result = agent
            .post(api_url())
            .header("x-api-key", api_key)
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .header("content-type", "application/json")
            .send_json(body);
        let (err, wait) = match result {
            Err(e) => (ApiError::new(ErrorKind::Network, e.to_string()), None),
            Ok(resp) if resp.status().as_u16() != 200 => {
                let status = resp.status().as_u16();
                let retry_after = resp.headers().get("retry-after").and_then(|h| h.to_str().ok()).and_then(|s| s.parse::<u64>().ok());
                let text = resp.into_body().read_to_string().unwrap_or_default();
                (parse_error(status, &text), retry_after)
            }
            Ok(resp) => {
                let mut acc = Accumulator::default();
                let mut sse = SseParser::default();
                let reader = BufReader::new(resp.into_body().into_reader());
                for line in reader.lines() {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(ApiError::new(ErrorKind::Cancelled, "stopped"));
                    }
                    let line = line.map_err(|e| ApiError::new(ErrorKind::Network, e.to_string()))?;
                    sse.line(&line, |ev, data| {
                        if let Some(p) = acc.handle(ev, data) {
                            on_event(p);
                        }
                    });
                }
                sse.line("", |ev, data| {
                    acc.handle(ev, data);
                });
                return acc.finish();
            }
        };
        attempt += 1;
        if !err.retryable() || attempt > 2 {
            return Err(err);
        }
        let secs = wait.unwrap_or(if attempt == 1 { 2 } else { 6 }).min(30);
        for _ in 0..secs * 10 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ApiError::new(ErrorKind::Cancelled, "stopped"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorded-shape stream: thinking, text, and a tool call split across deltas.
    const STREAM: &str = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5","content":[],"usage":{"input_tokens":12,"cache_read_input_tokens":3000,"cache_creation_input_tokens":0,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig123"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Here's "}}

event: ping
data: {"type":"ping"}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"a plan."}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"propose_changes","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"summary\":\"Mo"}}

event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"ve it\",\"changes\":[{\"action\":\"delete_task\",\"id\":3}]}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":2}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":95}}

event: message_stop
data: {"type":"message_stop"}

"#;

    fn run(stream: &str) -> (Result<Reply, ApiError>, Vec<StreamEvent>) {
        let mut acc = Accumulator::default();
        let mut sse = SseParser::default();
        let mut events = Vec::new();
        for line in stream.lines() {
            sse.line(line, |ev, data| {
                if let Some(p) = acc.handle(ev, data) {
                    events.push(p);
                }
            });
        }
        sse.line("", |ev, data| {
            acc.handle(ev, data);
        });
        (acc.finish(), events)
    }

    #[test]
    fn rebuilds_blocks_from_stream() {
        let (reply, events) = run(STREAM);
        let r = reply.unwrap();
        assert_eq!(r.stop_reason, "tool_use");
        assert_eq!(r.model, "claude-opus-5-5");
        assert_eq!(r.usage, Usage { input: 12, output: 95, cache_read: 3000, cache_write: 0 });
        assert_eq!(r.content.len(), 3);
        assert_eq!(r.content[0], json!({"type": "thinking", "thinking": "", "signature": "sig123"}));
        assert_eq!(r.content[1], json!({"type": "text", "text": "Here's a plan."}));
        assert_eq!(r.content[2]["input"], json!({"summary": "Move it", "changes": [{"action": "delete_task", "id": 3}]}));
        assert!(r.bad_tool_inputs.is_empty());
        assert_eq!(events.iter().filter(|e| matches!(e, StreamEvent::Text(_))).count(), 2);
        assert!(events.contains(&StreamEvent::ToolStarted("propose_changes".into())));
    }

    #[test]
    fn truncated_tool_input_and_stream_errors() {
        let truncated = STREAM.replace(r#"]}"}}"#, r#""}}"#);
        let (reply, _) = run(&truncated);
        assert_eq!(reply.unwrap().bad_tool_inputs, vec![2]);

        let overloaded = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"m\",\"usage\":{}}}\n\nevent: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        let err = run(overloaded).0.unwrap_err();
        assert_eq!(err.kind, ErrorKind::Overloaded);
        assert!(err.retryable());

        let cut = STREAM.split("event: message_delta").next().unwrap();
        assert_eq!(run(cut).0.unwrap_err().kind, ErrorKind::Network);
    }

    #[test]
    fn http_errors_map_to_kinds() {
        assert_eq!(parse_error(401, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#).kind, ErrorKind::Auth);
        assert_eq!(parse_error(429, "{}").kind, ErrorKind::RateLimited);
        assert_eq!(parse_error(529, "").kind, ErrorKind::Overloaded);
        assert_eq!(parse_error(400, r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#).message, "bad");
        assert!(!parse_error(400, "").retryable());
    }

    #[test]
    fn request_shape() {
        let b = request_body("claude-opus-5-5", &[json!({"role": "user", "content": "hi"})]);
        assert_eq!(b["model"], "claude-opus-5-5");
        assert_eq!(b["stream"], true);
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["output_config"]["effort"], "medium");
        assert!(b.get("thinking").is_none(), "thinking is always on for Opus 5.5; the field is omitted");
        assert_eq!(b["tools"][0]["name"], "propose_changes");
        let u = Usage { input: 1_000_000, output: 100_000, cache_read: 0, cache_write: 0 };
        assert!((u.cost(&model_info("claude-opus-5-5")) - 6.0).abs() < 1e-9);
    }
}
