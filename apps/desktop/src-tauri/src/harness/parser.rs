//! Claude Code's `stream-json` output, line by line.
//!
//! Every line is one JSON object. `parse_line` turns it into a typed `Event`. Anything the app
//! does not know yet becomes `Event::Unknown` or `Event::System`, so a newer Claude Code can add
//! events without breaking sessions. The raw line is always stored as well, and it is the source
//! of truth: messages can be rebuilt from it at any time.

use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq)]
pub struct Init {
    pub session_id: String,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub permission_mode: Option<String>,
    pub version: Option<String>,
    pub tools: Vec<String>,
    pub slash_commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DeltaKind {
    Text,
    Thinking,
    /// Part of a tool call's input, streamed as JSON text.
    InputJson,
}

/// A piece of a message that is still being written.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta {
    pub block_index: u32,
    pub kind: DeltaKind,
    pub text: String,
    pub parent_tool_use_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    Thinking(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// A block type this version does not render, such as `redacted_thinking`.
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assistant {
    pub message_id: Option<String>,
    pub parent_tool_use_id: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UserPart {
    Text(String),
    ToolResult {
        tool_use_id: String,
        /// A string, or a list of content blocks (text, images).
        content: Value,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserEvent {
    pub parent_tool_use_id: Option<String>,
    pub parts: Vec<UserPart>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnResult {
    /// `success`, `error_during_execution`, `error_max_turns`, ...
    pub subtype: String,
    pub is_error: bool,
    pub result: Option<String>,
    /// What went wrong, when Claude Code says so in a list instead of in `result`.
    pub errors: Vec<String>,
    pub total_cost_usd: f64,
    pub duration_ms: u64,
    pub stop_reason: Option<String>,
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlKind {
    /// Claude Code asks whether it may use a tool.
    CanUseTool {
        tool_name: String,
        display_name: Option<String>,
        input: Value,
        tool_use_id: Option<String>,
        description: Option<String>,
        blocked_path: Option<String>,
    },
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlRequest {
    pub request_id: String,
    pub kind: ControlKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RateLimit {
    /// `allowed`, `allowed_warning` or `rejected`.
    pub status: String,
    /// Which window the status is about, for example `five_hour`.
    pub window: String,
    /// Unix seconds when the window resets.
    #[ts(type = "number | null")]
    pub resets_at: Option<i64>,
    pub five_hour_utilization: Option<f64>,
    pub seven_day_utilization: Option<f64>,
    pub using_overage: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Init(Init),
    /// `system` with subtype `status`.
    Status(String),
    Delta(Delta),
    /// `message_start`, `content_block_stop` and the other stream bookkeeping events.
    StreamMarker(String),
    Assistant(Assistant),
    User(UserEvent),
    Result(TurnResult),
    ControlRequest(ControlRequest),
    ControlResponse {
        request_id: String,
        success: bool,
        error: Option<String>,
    },
    RateLimit(RateLimit),
    /// Hooks, background tasks and other `system` events. The subtype is kept.
    System(String),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub event: Event,
    /// Short label for the `events` table, for example `assistant` or `stream_event:text_delta`.
    pub kind: String,
    /// Claude's session id, when the line carries one.
    pub session_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the line is not a JSON object")]
    NotAnObject,
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn strings_of(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parent_of(v: &Value) -> Option<String> {
    str_of(v, "parent_tool_use_id")
}

fn parse_assistant(v: &Value) -> Assistant {
    let message = v.get("message").unwrap_or(&Value::Null);
    let blocks = message
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .map(|b| match b.get("type").and_then(Value::as_str) {
                    Some("text") => Block::Text(str_of(b, "text").unwrap_or_default()),
                    Some("thinking") => Block::Thinking(str_of(b, "thinking").unwrap_or_default()),
                    Some("tool_use") => Block::ToolUse {
                        id: str_of(b, "id").unwrap_or_default(),
                        name: str_of(b, "name").unwrap_or_default(),
                        input: b.get("input").cloned().unwrap_or(Value::Null),
                    },
                    Some(other) => Block::Other(other.to_string()),
                    None => Block::Other("unknown".to_string()),
                })
                .collect()
        })
        .unwrap_or_default();
    Assistant {
        message_id: str_of(message, "id"),
        parent_tool_use_id: parent_of(v),
        blocks,
    }
}

fn parse_user(v: &Value) -> UserEvent {
    let content = v.get("message").and_then(|m| m.get("content"));
    let parts = match content {
        Some(Value::String(text)) => vec![UserPart::Text(text.clone())],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => Some(UserPart::Text(str_of(b, "text").unwrap_or_default())),
                Some("tool_result") => Some(UserPart::ToolResult {
                    tool_use_id: str_of(b, "tool_use_id").unwrap_or_default(),
                    content: b.get("content").cloned().unwrap_or(Value::Null),
                    is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                }),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    UserEvent {
        parent_tool_use_id: parent_of(v),
        parts,
    }
}

fn parse_result(v: &Value) -> TurnResult {
    let subtype = str_of(v, "subtype").unwrap_or_else(|| "success".to_string());
    TurnResult {
        is_error: v
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(subtype != "success"),
        subtype,
        result: str_of(v, "result"),
        errors: strings_of(v, "errors"),
        total_cost_usd: v
            .get("total_cost_usd")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        duration_ms: v.get("duration_ms").and_then(Value::as_u64).unwrap_or(0),
        stop_reason: str_of(v, "stop_reason"),
        terminal_reason: str_of(v, "terminal_reason"),
    }
}

fn parse_stream_event(v: &Value) -> (Event, String) {
    let event = v.get("event").unwrap_or(&Value::Null);
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    if event_type == "content_block_delta" {
        let delta = event.get("delta").unwrap_or(&Value::Null);
        let block_index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
        let parsed = match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => Some((DeltaKind::Text, str_of(delta, "text"))),
            Some("thinking_delta") => Some((DeltaKind::Thinking, str_of(delta, "thinking"))),
            Some("input_json_delta") => Some((DeltaKind::InputJson, str_of(delta, "partial_json"))),
            _ => None,
        };
        if let Some((kind, Some(text))) = parsed {
            let label = format!("stream_event:{event_type}");
            return (
                Event::Delta(Delta {
                    block_index,
                    kind,
                    text,
                    parent_tool_use_id: parent_of(v),
                }),
                label,
            );
        }
    }
    (
        Event::StreamMarker(event_type.clone()),
        format!("stream_event:{event_type}"),
    )
}

fn parse_control_request(v: &Value) -> Event {
    let request = v.get("request").unwrap_or(&Value::Null);
    let request_id = str_of(v, "request_id").unwrap_or_default();
    let kind = match request.get("subtype").and_then(Value::as_str) {
        Some("can_use_tool") => ControlKind::CanUseTool {
            tool_name: str_of(request, "tool_name").unwrap_or_default(),
            display_name: str_of(request, "display_name"),
            input: request.get("input").cloned().unwrap_or(Value::Null),
            tool_use_id: str_of(request, "tool_use_id"),
            description: str_of(request, "description"),
            blocked_path: str_of(request, "blocked_path"),
        },
        Some(other) => ControlKind::Other(other.to_string()),
        None => ControlKind::Other("unknown".to_string()),
    };
    Event::ControlRequest(ControlRequest { request_id, kind })
}

fn parse_rate_limit(v: &Value) -> Event {
    let info = v.get("rate_limit_info").unwrap_or(&Value::Null);
    let window = |name: &str| info.get("unifiedWindows").and_then(|w| w.get(name));
    Event::RateLimit(RateLimit {
        status: str_of(info, "status").unwrap_or_else(|| "allowed".to_string()),
        window: str_of(info, "rateLimitType").unwrap_or_default(),
        resets_at: info.get("resetsAt").and_then(Value::as_i64),
        five_hour_utilization: window("five_hour")
            .and_then(|w| w.get("utilization"))
            .and_then(Value::as_f64),
        seven_day_utilization: window("seven_day")
            .and_then(|w| w.get("utilization"))
            .and_then(Value::as_f64),
        using_overage: info
            .get("isUsingOverage")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Parses one output line. Blank lines are the caller's to skip.
pub fn parse_line(line: &str) -> Result<Parsed, ParseError> {
    let v: Value = serde_json::from_str(line)?;
    if !v.is_object() {
        return Err(ParseError::NotAnObject);
    }
    let kind_of_type = str_of(&v, "type").unwrap_or_default();
    let session_id = str_of(&v, "session_id");

    let (event, kind) = match kind_of_type.as_str() {
        "system" => {
            let subtype = str_of(&v, "subtype").unwrap_or_default();
            let event = match subtype.as_str() {
                "init" => Event::Init(Init {
                    session_id: session_id.clone().unwrap_or_default(),
                    model: str_of(&v, "model"),
                    cwd: str_of(&v, "cwd"),
                    permission_mode: str_of(&v, "permissionMode"),
                    version: str_of(&v, "claude_code_version"),
                    tools: strings_of(&v, "tools"),
                    slash_commands: strings_of(&v, "slash_commands"),
                }),
                "status" => Event::Status(str_of(&v, "status").unwrap_or_default()),
                _ => Event::System(subtype.clone()),
            };
            (event, format!("system:{subtype}"))
        }
        "stream_event" => parse_stream_event(&v),
        "assistant" => (
            Event::Assistant(parse_assistant(&v)),
            "assistant".to_string(),
        ),
        "user" => (Event::User(parse_user(&v)), "user".to_string()),
        "result" => {
            let result = parse_result(&v);
            let kind = format!("result:{}", result.subtype);
            (Event::Result(result), kind)
        }
        "control_request" => {
            let event = parse_control_request(&v);
            let kind = match &event {
                Event::ControlRequest(ControlRequest {
                    kind: ControlKind::CanUseTool { .. },
                    ..
                }) => "control_request:can_use_tool".to_string(),
                Event::ControlRequest(ControlRequest {
                    kind: ControlKind::Other(s),
                    ..
                }) => {
                    format!("control_request:{s}")
                }
                _ => "control_request".to_string(),
            };
            (event, kind)
        }
        "control_response" => {
            let response = v.get("response").unwrap_or(&Value::Null);
            let success = response.get("subtype").and_then(Value::as_str) == Some("success");
            (
                Event::ControlResponse {
                    request_id: str_of(response, "request_id").unwrap_or_default(),
                    success,
                    error: str_of(response, "error"),
                },
                "control_response".to_string(),
            )
        }
        "rate_limit_event" => (parse_rate_limit(&v), "rate_limit_event".to_string()),
        // Claude Code's `result` line carries no `type` in some versions.
        "" if v.get("total_cost_usd").is_some() => {
            let result = parse_result(&v);
            let kind = format!("result:{}", result.subtype);
            (Event::Result(result), kind)
        }
        other => (
            Event::Unknown(other.to_string()),
            format!("unknown:{other}"),
        ),
    };
    Ok(Parsed {
        event,
        kind,
        session_id,
    })
}

// Messages

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Thinking,
    ToolCall,
    ToolResult,
    System,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Text => "text",
            MessageKind::Thinking => "thinking",
            MessageKind::ToolCall => "tool_call",
            MessageKind::ToolResult => "tool_result",
            MessageKind::System => "system",
        }
    }
}

/// A message ready to be stored. `content` is JSON: `{text}`, `{id, name, input}`,
/// `{tool_use_id, content, is_error}` or `{text, ...}` for system notes.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMessage {
    pub role: Role,
    pub kind: MessageKind,
    pub content: Value,
}

/// What the interrupt notice in the transcript looks like.
pub const INTERRUPT_NOTICE: &str = "[Request interrupted by user]";

fn with_parent(mut content: Value, parent: &Option<String>) -> Value {
    if let (Some(parent), Some(obj)) = (parent, content.as_object_mut()) {
        obj.insert("parent_tool_use_id".into(), Value::String(parent.clone()));
    }
    content
}

/// The chat messages an event adds. Most events add none.
pub fn normalize(event: &Event) -> Vec<NewMessage> {
    match event {
        Event::Assistant(a) => a
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(text) => Some(NewMessage {
                    role: Role::Assistant,
                    kind: MessageKind::Text,
                    content: with_parent(serde_json::json!({ "text": text, "message_id": a.message_id }), &a.parent_tool_use_id),
                }),
                Block::Thinking(text) => Some(NewMessage {
                    role: Role::Assistant,
                    kind: MessageKind::Thinking,
                    content: with_parent(serde_json::json!({ "text": text, "message_id": a.message_id }), &a.parent_tool_use_id),
                }),
                Block::ToolUse { id, name, input } => Some(NewMessage {
                    role: Role::Assistant,
                    kind: MessageKind::ToolCall,
                    content: with_parent(serde_json::json!({ "id": id, "name": name, "input": input }), &a.parent_tool_use_id),
                }),
                Block::Other(_) => None,
            })
            .collect(),
        Event::User(u) => u
            .parts
            .iter()
            .filter_map(|part| match part {
                UserPart::ToolResult { tool_use_id, content, is_error } => Some(NewMessage {
                    role: Role::User,
                    kind: MessageKind::ToolResult,
                    content: with_parent(
                        serde_json::json!({ "tool_use_id": tool_use_id, "content": content, "is_error": is_error }),
                        &u.parent_tool_use_id,
                    ),
                }),
                UserPart::Text(text) if text.trim() == INTERRUPT_NOTICE => Some(NewMessage {
                    role: Role::System,
                    kind: MessageKind::System,
                    content: serde_json::json!({ "text": "Interrupted", "interrupted": true }),
                }),
                // Other user text is what we sent ourselves, already stored when it was sent.
                UserPart::Text(_) => None,
            })
            .collect(),
        Event::Result(r) if r.is_error => vec![NewMessage {
            role: Role::System,
            kind: MessageKind::System,
            content: serde_json::json!({
                "text": r.result.clone().unwrap_or_else(|| format!("The turn ended with {}", r.subtype)),
                "error": true,
                "subtype": r.subtype,
            }),
        }],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(fixture: &str) -> Vec<Parsed> {
        fixture.lines().map(|l| parse_line(l).unwrap()).collect()
    }

    fn kinds(parsed: &[Parsed]) -> Vec<&str> {
        parsed.iter().map(|p| p.kind.as_str()).collect()
    }

    fn messages(parsed: &[Parsed]) -> Vec<NewMessage> {
        parsed.iter().flat_map(|p| normalize(&p.event)).collect()
    }

    const TOOL_TURN: &str = include_str!("fixtures/tool_turn.jsonl");
    const STREAMING: &str = include_str!("fixtures/streaming_turn.jsonl");
    const INTERRUPT: &str = include_str!("fixtures/interrupt.jsonl");
    const BACKGROUND: &str = include_str!("fixtures/steer_background_task.jsonl");

    #[test]
    fn a_turn_with_a_tool_call() {
        let parsed = lines(TOOL_TURN);
        assert_eq!(
            kinds(&parsed),
            [
                "system:init",
                "assistant",
                "rate_limit_event",
                "user",
                "assistant",
                "rate_limit_event",
                "result:success"
            ]
        );

        let Event::Init(init) = &parsed[0].event else {
            panic!("init expected")
        };
        assert_eq!(init.model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(init.cwd.as_deref(), Some("/work/roster"));
        assert_eq!(init.permission_mode.as_deref(), Some("acceptEdits"));
        assert_eq!(init.version.as_deref(), Some("2.1.293"));
        assert!(init.tools.contains(&"Read".to_string()));
        assert_eq!(init.slash_commands, ["compact", "context", "review"]);
        assert!(!init.session_id.is_empty());
        assert_eq!(
            parsed[0].session_id.as_deref(),
            Some(init.session_id.as_str())
        );

        let Event::Assistant(call) = &parsed[1].event else {
            panic!("assistant expected")
        };
        let Block::ToolUse { name, input, .. } = &call.blocks[0] else {
            panic!("tool use expected")
        };
        assert_eq!(name, "Read");
        assert_eq!(input["file_path"], "/work/roster/a.txt");

        let Event::User(user) = &parsed[3].event else {
            panic!("user expected")
        };
        let UserPart::ToolResult {
            content, is_error, ..
        } = &user.parts[0]
        else {
            panic!("tool result expected")
        };
        assert_eq!(content, "1\thi\n2\t");
        assert!(!is_error);

        let Event::Result(result) = &parsed[6].event else {
            panic!("result expected")
        };
        assert_eq!(result.subtype, "success");
        assert!(!result.is_error);
        assert!(result.total_cost_usd > 0.0);
        assert_eq!(result.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(result.terminal_reason.as_deref(), Some("completed"));
    }

    #[test]
    fn a_turn_becomes_messages_in_order() {
        let msgs = messages(&lines(TOOL_TURN));
        assert_eq!(
            msgs.iter().map(|m| (m.role, m.kind)).collect::<Vec<_>>(),
            [
                (Role::Assistant, MessageKind::ToolCall),
                (Role::User, MessageKind::ToolResult),
                (Role::Assistant, MessageKind::Text),
            ]
        );
        assert_eq!(msgs[0].content["name"], "Read");
        assert_eq!(msgs[1].content["tool_use_id"], msgs[0].content["id"]);
        assert_eq!(msgs[1].content["is_error"], false);
        assert_eq!(msgs[2].content["text"], "done");
    }

    #[test]
    fn rate_limit_information_is_extracted() {
        let parsed = lines(TOOL_TURN);
        let Event::RateLimit(limit) = &parsed[2].event else {
            panic!("rate limit expected")
        };
        assert_eq!(limit.status, "allowed");
        assert_eq!(limit.window, "five_hour");
        assert_eq!(limit.resets_at, Some(1_791_438_600));
        assert_eq!(limit.five_hour_utilization, Some(0.02));
        assert_eq!(limit.seven_day_utilization, Some(0.0));
        assert!(!limit.using_overage);
        assert!(normalize(&parsed[2].event).is_empty());
    }

    #[test]
    fn streaming_gives_deltas_and_then_one_final_message() {
        let parsed = lines(STREAMING);
        let deltas: Vec<&Delta> = parsed
            .iter()
            .filter_map(|p| {
                if let Event::Delta(d) = &p.event {
                    Some(d)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(deltas.len(), 1);
        assert_eq!(
            (
                deltas[0].kind,
                deltas[0].text.as_str(),
                deltas[0].block_index
            ),
            (DeltaKind::Text, "one", 0)
        );

        let markers: Vec<&str> = parsed
            .iter()
            .filter_map(|p| {
                if let Event::StreamMarker(m) = &p.event {
                    Some(m.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            markers,
            [
                "message_start",
                "content_block_start",
                "content_block_stop",
                "message_delta",
                "message_stop"
            ]
        );
        assert!(parsed
            .iter()
            .any(|p| p.kind == "stream_event:content_block_delta"));
        assert!(parsed.iter().any(|p| matches!(p.event, Event::Status(_))));

        let msgs = messages(&parsed);
        assert_eq!(
            msgs.len(),
            1,
            "deltas never become messages, only the final assistant event does"
        );
        assert_eq!(msgs[0].content["text"], "one");
        assert_eq!(msgs[0].kind, MessageKind::Text);
    }

    #[test]
    fn an_interrupt_is_acknowledged_and_noted_in_the_transcript() {
        let parsed = lines(INTERRUPT);
        let response = parsed
            .iter()
            .find_map(|p| {
                if let Event::ControlResponse {
                    request_id,
                    success,
                    ..
                } = &p.event
                {
                    Some((request_id.clone(), *success))
                } else {
                    None
                }
            })
            .expect("a control response");
        assert_eq!(response, ("req1".to_string(), true));

        let msgs = messages(&parsed);
        let notice = msgs
            .iter()
            .find(|m| m.role == Role::System)
            .expect("an interrupt notice");
        assert_eq!(notice.content["text"], "Interrupted");
        assert_eq!(notice.content["interrupted"], true);
        // The turn that follows is an ordinary one.
        assert!(msgs
            .iter()
            .any(|m| m.kind == MessageKind::Text && m.content["text"] == "after-interrupt"));
    }

    #[test]
    fn the_interrupted_turn_ends_with_a_result_that_is_not_an_error() {
        let parsed = lines(INTERRUPT);
        let results: Vec<&TurnResult> = parsed
            .iter()
            .filter_map(|p| {
                if let Event::Result(r) = &p.event {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].stop_reason, None);
        assert_eq!(results[0].total_cost_usd, 0.0);
    }

    #[test]
    fn background_tasks_are_typed_system_events() {
        let parsed = lines(BACKGROUND);
        let systems: Vec<&str> = parsed
            .iter()
            .filter_map(|p| {
                if let Event::System(s) = &p.event {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(systems, ["task_started", "task_notification"]);
        assert!(parsed.iter().any(|p| p.kind == "system:task_started"));
        let msgs = messages(&parsed);
        assert!(msgs
            .iter()
            .any(|m| m.kind == MessageKind::ToolCall && m.content["name"] == "Bash"));
        assert!(msgs.iter().any(|m| m.kind == MessageKind::ToolResult));
    }

    #[test]
    fn every_fixture_line_parses_and_is_known() {
        for fixture in [TOOL_TURN, STREAMING, INTERRUPT, BACKGROUND] {
            for parsed in lines(fixture) {
                // Acknowledgements of our own control requests carry no session id.
                if parsed.kind != "control_response" {
                    assert!(parsed.session_id.is_some(), "{}", parsed.kind);
                }
                assert!(
                    !matches!(parsed.event, Event::Unknown(_)),
                    "{}",
                    parsed.kind
                );
            }
        }
    }

    #[test]
    fn a_permission_request_names_the_tool_and_its_input() {
        let line = r#"{"type":"control_request","request_id":"r-1","request":{"subtype":"can_use_tool","tool_name":"Write","display_name":"Write","input":{"file_path":"/work/roster/c.txt","content":"x"},"tool_use_id":"toolu_1","description":"Create c.txt","blocked_path":"/work/roster"}}"#;
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.kind, "control_request:can_use_tool");
        let Event::ControlRequest(request) = parsed.event else {
            panic!("control request expected")
        };
        assert_eq!(request.request_id, "r-1");
        let ControlKind::CanUseTool {
            tool_name,
            input,
            tool_use_id,
            description,
            blocked_path,
            display_name,
        } = request.kind
        else {
            panic!("can_use_tool expected")
        };
        assert_eq!(tool_name, "Write");
        assert_eq!(display_name.as_deref(), Some("Write"));
        assert_eq!(input["file_path"], "/work/roster/c.txt");
        assert_eq!(tool_use_id.as_deref(), Some("toolu_1"));
        assert_eq!(description.as_deref(), Some("Create c.txt"));
        assert_eq!(blocked_path.as_deref(), Some("/work/roster"));
    }

    #[test]
    fn a_minimal_permission_request_is_still_understood() {
        let line = r#"{"type":"control_request","request_id":"r-2","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}"#;
        let Event::ControlRequest(request) = parse_line(line).unwrap().event else {
            panic!()
        };
        let ControlKind::CanUseTool {
            tool_name,
            tool_use_id,
            description,
            ..
        } = request.kind
        else {
            panic!()
        };
        assert_eq!(tool_name, "Bash");
        assert_eq!((tool_use_id, description), (None, None));
    }

    #[test]
    fn other_control_requests_are_kept_by_subtype() {
        let line = r#"{"type":"control_request","request_id":"r-3","request":{"subtype":"mcp_message","server_name":"x"}}"#;
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.kind, "control_request:mcp_message");
        assert!(
            matches!(parsed.event, Event::ControlRequest(ControlRequest { kind: ControlKind::Other(ref s), .. }) if s == "mcp_message")
        );
    }

    #[test]
    fn a_failed_control_response_carries_its_error() {
        let line = r#"{"type":"control_response","response":{"subtype":"error","request_id":"req9","error":"nothing to interrupt"}}"#;
        let Event::ControlResponse {
            request_id,
            success,
            error,
        } = parse_line(line).unwrap().event
        else {
            panic!()
        };
        assert_eq!((request_id.as_str(), success), ("req9", false));
        assert_eq!(error.as_deref(), Some("nothing to interrupt"));
    }

    #[test]
    fn thinking_text_and_tool_calls_in_one_message_become_separate_rows() {
        let line = r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"thinking","thinking":"hmm","signature":"s"},{"type":"text","text":"Let me look."},{"type":"tool_use","id":"toolu_9","name":"Grep","input":{"pattern":"todo"}},{"type":"redacted_thinking","data":"x"}]},"parent_tool_use_id":null,"session_id":"s1"}"#;
        let msgs = normalize(&parse_line(line).unwrap().event);
        assert_eq!(
            msgs.iter().map(|m| m.kind).collect::<Vec<_>>(),
            [
                MessageKind::Thinking,
                MessageKind::Text,
                MessageKind::ToolCall
            ]
        );
        assert_eq!(msgs[0].content["text"], "hmm");
        assert_eq!(msgs[0].content["message_id"], "msg_1");
        assert_eq!(msgs[2].content["input"]["pattern"], "todo");
    }

    #[test]
    fn subagent_messages_remember_which_tool_call_they_belong_to() {
        let line = r#"{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"inside the subagent"}]},"parent_tool_use_id":"toolu_task","session_id":"s1"}"#;
        let msgs = normalize(&parse_line(line).unwrap().event);
        assert_eq!(msgs[0].content["parent_tool_use_id"], "toolu_task");
    }

    #[test]
    fn tool_results_can_be_errors_or_lists_of_blocks() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"content":[{"type":"text","text":"Permission denied"}]}]},"parent_tool_use_id":null,"session_id":"s1"}"#;
        let msgs = normalize(&parse_line(line).unwrap().event);
        assert_eq!(msgs[0].content["is_error"], true);
        assert_eq!(msgs[0].content["content"][0]["text"], "Permission denied");
    }

    #[test]
    fn user_text_we_sent_ourselves_is_not_stored_twice() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hello there"},"session_id":"s1"}"#;
        assert!(normalize(&parse_line(line).unwrap().event).is_empty());
    }

    #[test]
    fn a_failed_turn_leaves_a_system_note() {
        let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"The model overloaded","session_id":"s1","total_cost_usd":0.01,"duration_ms":1200}"#;
        let parsed = parse_line(line).unwrap();
        assert_eq!(parsed.kind, "result:error_during_execution");
        let Event::Result(result) = &parsed.event else {
            panic!()
        };
        assert!(result.is_error);
        assert_eq!(result.duration_ms, 1200);
        let msgs = normalize(&parsed.event);
        assert_eq!(msgs[0].kind, MessageKind::System);
        assert_eq!(msgs[0].content["error"], true);
        assert_eq!(msgs[0].content["text"], "The model overloaded");
    }

    #[test]
    fn a_result_without_a_type_field_is_still_a_result() {
        let line =
            r#"{"subtype":"success","session_id":"s1","total_cost_usd":0.5,"duration_ms":10}"#;
        assert!(matches!(parse_line(line).unwrap().event, Event::Result(_)));
    }

    #[test]
    fn unknown_events_are_tolerated() {
        let parsed =
            parse_line(r#"{"type":"something_new","session_id":"s1","data":[1,2,3]}"#).unwrap();
        assert_eq!(parsed.kind, "unknown:something_new");
        assert!(normalize(&parsed.event).is_empty());
        let system =
            parse_line(r#"{"type":"system","subtype":"future_thing","session_id":"s1"}"#).unwrap();
        assert!(matches!(system.event, Event::System(ref s) if s == "future_thing"));
    }

    #[test]
    fn the_errors_of_a_failed_result_are_kept() {
        let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"session_id":"s1","errors":["No conversation found with session ID: s1"],"duration_ms":0}"#;
        let parsed = parse_line(line).unwrap();
        let Event::Result(result) = parsed.event else {
            panic!("a result");
        };
        assert_eq!(result.result, None);
        assert_eq!(
            result.errors,
            vec!["No conversation found with session ID: s1".to_string()]
        );
    }

    #[test]
    fn broken_lines_are_errors() {
        assert!(matches!(parse_line("not json"), Err(ParseError::Json(_))));
        assert!(matches!(parse_line("[1,2]"), Err(ParseError::NotAnObject)));
        assert!(matches!(
            parse_line("\"text\""),
            Err(ParseError::NotAnObject)
        ));
    }

    #[test]
    fn tool_input_streams_are_deltas_too() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_"}},"session_id":"s1","parent_tool_use_id":null}"#;
        let Event::Delta(d) = parse_line(line).unwrap().event else {
            panic!()
        };
        assert_eq!(
            (d.kind, d.block_index, d.text.as_str()),
            (DeltaKind::InputJson, 1, "{\"file_")
        );
        let thinking = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"let me"}},"session_id":"s1"}"#;
        let Event::Delta(d) = parse_line(thinking).unwrap().event else {
            panic!()
        };
        assert_eq!(d.kind, DeltaKind::Thinking);
        // A delta type we do not know falls back to a marker instead of failing.
        let odd = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"citations_delta"}},"session_id":"s1"}"#;
        assert!(matches!(
            parse_line(odd).unwrap().event,
            Event::StreamMarker(_)
        ));
    }
}
