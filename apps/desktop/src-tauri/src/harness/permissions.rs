//! Permission requests: what the user is asked, what "allow for this session" remembers, and the
//! reply Claude Code expects.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::parser::{ControlKind, ControlRequest};
use crate::ipc::now_ms;

/// A tool use waiting for the user's answer.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct PermissionRequest {
    pub request_id: String,
    pub session_id: String,
    pub tool_name: String,
    /// Human friendly tool name, when Claude Code gives one.
    pub display_name: Option<String>,
    #[ts(type = "unknown")]
    pub input: Value,
    pub tool_use_id: Option<String>,
    pub description: Option<String>,
    /// A path outside the workspace that the tool wants to reach.
    pub blocked_path: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// The user's answer.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PermissionDecision {
    /// Allow this one use.
    Allow,
    /// Allow this use, and the same kind of use for the rest of the session.
    AllowSession,
    Deny {
        /// Shown to Claude, so it can change course.
        message: Option<String>,
    },
}

impl PermissionRequest {
    /// Builds the request from Claude Code's control request. `None` for other control requests.
    pub fn from_control(session_id: &str, request: &ControlRequest) -> Option<PermissionRequest> {
        let ControlKind::CanUseTool {
            tool_name,
            display_name,
            input,
            tool_use_id,
            description,
            blocked_path,
        } = &request.kind
        else {
            return None;
        };
        Some(PermissionRequest {
            request_id: request.request_id.clone(),
            session_id: session_id.to_string(),
            tool_name: tool_name.clone(),
            display_name: display_name.clone(),
            input: input.clone(),
            tool_use_id: tool_use_id.clone(),
            description: description.clone(),
            blocked_path: blocked_path.clone(),
            created_at: now_ms(),
        })
    }

    /// What "allow for this session" remembers. Shell commands are remembered one by one, so
    /// allowing `ls` never allows `rm -rf`. Every other tool is remembered by name.
    pub fn rule_key(&self) -> String {
        rule_key(&self.tool_name, &self.input)
    }
}

pub fn rule_key(tool_name: &str, input: &Value) -> String {
    if tool_name == "Bash" {
        let command = input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        format!("Bash:{command}")
    } else {
        tool_name.to_string()
    }
}

/// The `control_response` line that answers a request. Newline terminated.
pub fn response_line(request: &PermissionRequest, decision: &PermissionDecision) -> String {
    let inner = match decision {
        PermissionDecision::Allow | PermissionDecision::AllowSession => serde_json::json!({
            "behavior": "allow",
            "updatedInput": request.input,
        }),
        PermissionDecision::Deny { message } => serde_json::json!({
            "behavior": "deny",
            "message": message.clone().unwrap_or_else(|| "The user denied this action.".to_string()),
        }),
    };
    let line = serde_json::json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request.request_id,
            "response": inner,
        },
    });
    format!("{line}\n")
}

/// Requests waiting for an answer, and the uses the user allowed for the rest of the session.
#[derive(Default)]
pub struct Permissions {
    pending: Mutex<HashMap<String, PermissionRequest>>,
    allowed: Mutex<Vec<String>>,
}

impl Permissions {
    /// True when the user already allowed this kind of use during the session.
    pub fn is_allowed(&self, tool_name: &str, input: &Value) -> bool {
        let key = rule_key(tool_name, input);
        self.allowed
            .lock()
            .expect("permissions poisoned")
            .contains(&key)
    }

    pub fn add_pending(&self, request: PermissionRequest) {
        self.pending
            .lock()
            .expect("permissions poisoned")
            .insert(request.request_id.clone(), request);
    }

    /// Takes the request out and, for `AllowSession`, remembers the rule.
    pub fn resolve(
        &self,
        request_id: &str,
        decision: &PermissionDecision,
    ) -> Option<PermissionRequest> {
        let request = self
            .pending
            .lock()
            .expect("permissions poisoned")
            .remove(request_id)?;
        if *decision == PermissionDecision::AllowSession {
            let key = request.rule_key();
            let mut allowed = self.allowed.lock().expect("permissions poisoned");
            if !allowed.contains(&key) {
                allowed.push(key);
            }
        }
        Some(request)
    }

    /// Requests oldest first.
    pub fn pending(&self) -> Vec<PermissionRequest> {
        let mut list: Vec<_> = self
            .pending
            .lock()
            .expect("permissions poisoned")
            .values()
            .cloned()
            .collect();
        list.sort_by_key(|r| r.created_at);
        list
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().expect("permissions poisoned").len()
    }

    /// Forgets every waiting request, for when the process ended. Returns their ids.
    pub fn clear_pending(&self) -> Vec<String> {
        self.pending
            .lock()
            .expect("permissions poisoned")
            .drain()
            .map(|(id, _)| id)
            .collect()
    }

    pub fn allowed_rules(&self) -> Vec<String> {
        self.allowed.lock().expect("permissions poisoned").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::parser::{parse_line, Event};

    fn request(tool: &str, input: Value) -> PermissionRequest {
        PermissionRequest {
            request_id: format!("r-{tool}"),
            session_id: "s1".into(),
            tool_name: tool.into(),
            display_name: None,
            input,
            tool_use_id: Some("toolu_1".into()),
            description: None,
            blocked_path: None,
            created_at: now_ms(),
        }
    }

    #[test]
    fn built_from_a_real_shaped_control_request() {
        let line = r#"{"type":"control_request","request_id":"r-1","request":{"subtype":"can_use_tool","tool_name":"Write","display_name":"Write","input":{"file_path":"/w/c.txt","content":"x"},"tool_use_id":"toolu_1"}}"#;
        let Event::ControlRequest(control) = parse_line(line).unwrap().event else {
            panic!()
        };
        let request = PermissionRequest::from_control("s1", &control).unwrap();
        assert_eq!(
            (
                request.request_id.as_str(),
                request.session_id.as_str(),
                request.tool_name.as_str()
            ),
            ("r-1", "s1", "Write")
        );
        assert_eq!(request.input["file_path"], "/w/c.txt");
        assert_eq!(request.tool_use_id.as_deref(), Some("toolu_1"));

        let other =
            r#"{"type":"control_request","request_id":"r-2","request":{"subtype":"mcp_message"}}"#;
        let Event::ControlRequest(control) = parse_line(other).unwrap().event else {
            panic!()
        };
        assert!(PermissionRequest::from_control("s1", &control).is_none());
    }

    #[test]
    fn shell_commands_are_remembered_one_by_one_and_other_tools_by_name() {
        assert_eq!(
            rule_key("Bash", &serde_json::json!({"command": "  ls -la  "})),
            "Bash:ls -la"
        );
        assert_eq!(rule_key("Bash", &serde_json::json!({})), "Bash:");
        assert_eq!(
            rule_key("Write", &serde_json::json!({"file_path": "/a"})),
            "Write"
        );
        assert_eq!(
            rule_key("Edit", &serde_json::json!({"file_path": "/b"})),
            "Edit"
        );
    }

    #[test]
    fn allowing_a_command_for_the_session_never_allows_another() {
        let permissions = Permissions::default();
        let ls = request("Bash", serde_json::json!({"command": "ls"}));
        permissions.add_pending(ls.clone());
        assert!(!permissions.is_allowed("Bash", &ls.input));

        permissions
            .resolve(&ls.request_id, &PermissionDecision::AllowSession)
            .unwrap();
        assert!(permissions.is_allowed("Bash", &serde_json::json!({"command": "ls"})));
        assert!(permissions.is_allowed("Bash", &serde_json::json!({"command": " ls "})));
        assert!(!permissions.is_allowed("Bash", &serde_json::json!({"command": "rm -rf /"})));
        assert!(!permissions.is_allowed("Write", &serde_json::json!({})));
        assert_eq!(permissions.allowed_rules(), ["Bash:ls"]);
    }

    #[test]
    fn allowing_a_file_tool_covers_every_file() {
        let permissions = Permissions::default();
        let write = request("Write", serde_json::json!({"file_path": "/a"}));
        permissions.add_pending(write.clone());
        permissions
            .resolve(&write.request_id, &PermissionDecision::AllowSession)
            .unwrap();
        assert!(permissions.is_allowed("Write", &serde_json::json!({"file_path": "/anywhere"})));
        assert!(!permissions.is_allowed("Edit", &serde_json::json!({})));
    }

    #[test]
    fn a_single_allow_or_a_deny_remembers_nothing() {
        let permissions = Permissions::default();
        for (i, decision) in [
            PermissionDecision::Allow,
            PermissionDecision::Deny { message: None },
        ]
        .iter()
        .enumerate()
        {
            let mut r = request("Bash", serde_json::json!({"command": "ls"}));
            r.request_id = format!("r{i}");
            permissions.add_pending(r.clone());
            assert!(permissions.resolve(&r.request_id, decision).is_some());
        }
        assert!(permissions.allowed_rules().is_empty());
        assert!(!permissions.is_allowed("Bash", &serde_json::json!({"command": "ls"})));
    }

    #[test]
    fn requests_are_resolved_once_and_listed_oldest_first() {
        let permissions = Permissions::default();
        let mut first = request("Write", serde_json::json!({}));
        first.request_id = "a".into();
        first.created_at = 1;
        let mut second = request("Edit", serde_json::json!({}));
        second.request_id = "b".into();
        second.created_at = 2;
        permissions.add_pending(second);
        permissions.add_pending(first);
        assert_eq!(
            permissions
                .pending()
                .iter()
                .map(|r| r.request_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(permissions.pending_count(), 2);

        assert!(permissions
            .resolve("a", &PermissionDecision::Allow)
            .is_some());
        assert!(
            permissions
                .resolve("a", &PermissionDecision::Allow)
                .is_none(),
            "a request is answered once"
        );
        assert!(permissions
            .resolve("missing", &PermissionDecision::Allow)
            .is_none());

        assert_eq!(permissions.clear_pending(), ["b"]);
        assert_eq!(permissions.pending_count(), 0);
    }

    #[test]
    fn allow_replies_echo_the_input_back() {
        let r = request(
            "Write",
            serde_json::json!({"file_path": "/a", "content": "x"}),
        );
        for decision in [PermissionDecision::Allow, PermissionDecision::AllowSession] {
            let line = response_line(&r, &decision);
            assert!(line.ends_with('\n'));
            let v: Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(v["type"], "control_response");
            assert_eq!(v["response"]["subtype"], "success");
            assert_eq!(v["response"]["request_id"], "r-Write");
            assert_eq!(v["response"]["response"]["behavior"], "allow");
            assert_eq!(v["response"]["response"]["updatedInput"]["file_path"], "/a");
        }
    }

    #[test]
    fn deny_replies_carry_the_message_for_claude() {
        let r = request("Bash", serde_json::json!({"command": "rm -rf /"}));
        let line = response_line(
            &r,
            &PermissionDecision::Deny {
                message: Some("Never run that.".into()),
            },
        );
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["response"]["response"]["behavior"], "deny");
        assert_eq!(v["response"]["response"]["message"], "Never run that.");

        let default = response_line(&r, &PermissionDecision::Deny { message: None });
        let v: Value = serde_json::from_str(default.trim()).unwrap();
        assert_eq!(
            v["response"]["response"]["message"],
            "The user denied this action."
        );
    }

    #[test]
    fn decisions_deserialize_from_the_frontend_shape() {
        let allow: PermissionDecision = serde_json::from_str(r#"{"decision":"allow"}"#).unwrap();
        let session: PermissionDecision =
            serde_json::from_str(r#"{"decision":"allow_session"}"#).unwrap();
        let deny: PermissionDecision =
            serde_json::from_str(r#"{"decision":"deny","message":"no"}"#).unwrap();
        let bare_deny: PermissionDecision = serde_json::from_str(r#"{"decision":"deny"}"#).unwrap();
        assert_eq!(allow, PermissionDecision::Allow);
        assert_eq!(session, PermissionDecision::AllowSession);
        assert_eq!(
            deny,
            PermissionDecision::Deny {
                message: Some("no".into())
            }
        );
        assert_eq!(bare_deny, PermissionDecision::Deny { message: None });
    }
}
