//! A small MCP client over Streamable HTTP: JSON-RPC in a POST, the answer as JSON or as an event
//! stream. Only what Circles needs: start a session, list tools, call a tool.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

/// Biggest reply read, so one huge answer cannot fill memory.
const BODY_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum McpError {
    /// 401: the access token is not good (any more).
    Unauthorized,
    /// The session ended on the server's side. Starting a new one fixes it.
    SessionLost,
    Network(String),
    Protocol(String),
}

impl McpError {
    pub fn message(&self) -> String {
        match self {
            Self::Unauthorized => "Atlassian did not accept the sign-in.".into(),
            Self::SessionLost => "The connection to Atlassian was lost.".into(),
            Self::Network(e) => format!("Could not reach Atlassian: {e}"),
            Self::Protocol(e) => {
                format!("Atlassian answered in a way Circles does not understand: {e}")
            }
        }
    }
}

/// What a tool call gave back.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub text: String,
    pub is_error: bool,
}

pub struct McpClient {
    http: reqwest::Client,
    url: String,
    session: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl McpClient {
    pub fn new(url: &str) -> Result<Self, McpError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("circles")
            .build()
            .map_err(|e| McpError::Network(e.without_url().to_string()))?;
        Ok(Self {
            http,
            url: url.to_string(),
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
        })
    }

    async fn post(&self, token: &str, body: &Value, expect_reply: bool) -> Result<Value, McpError> {
        let mut request = self
            .http
            .post(&self.url)
            .bearer_auth(token)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18")
            .json(body);
        if let Some(session) = self.session.lock().unwrap().clone() {
            request = request.header("mcp-session-id", session);
        }
        let response = request
            .send()
            .await
            .map_err(|e| McpError::Network(e.without_url().to_string()))?;
        let status = response.status();
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock().unwrap() = Some(session.to_string());
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(McpError::Unauthorized);
        }
        let is_stream = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.starts_with("text/event-stream"));
        let bytes = response
            .bytes()
            .await
            .map_err(|e| McpError::Network(e.without_url().to_string()))?;
        if bytes.len() > BODY_LIMIT {
            return Err(McpError::Protocol("the answer was too large".into()));
        }
        // A session that is gone is a 404, and some servers use 400 for a missing one.
        if (status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::BAD_REQUEST)
            && self.session.lock().unwrap().is_some()
        {
            return Err(McpError::SessionLost);
        }
        if !status.is_success() {
            let text = String::from_utf8_lossy(&bytes);
            return Err(McpError::Protocol(format!(
                "{}: {}",
                status.as_u16(),
                text.chars().take(200).collect::<String>()
            )));
        }
        if !expect_reply || bytes.is_empty() {
            return Ok(Value::Null);
        }
        let text = String::from_utf8_lossy(&bytes);
        let want = body["id"].clone();
        let messages: Vec<Value> = if is_stream {
            text.lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .filter_map(|d| serde_json::from_str(d.trim()).ok())
                .collect()
        } else {
            serde_json::from_str::<Value>(&text)
                .map(|v| match v {
                    Value::Array(items) => items,
                    other => vec![other],
                })
                .map_err(|_| McpError::Protocol("the answer was not JSON".into()))?
        };
        messages
            .into_iter()
            .find(|m| m["id"] == want)
            .ok_or_else(|| McpError::Protocol("no answer to the request".into()))
    }

    async fn rpc(&self, token: &str, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let reply = self
            .post(
                token,
                &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
                true,
            )
            .await?;
        if let Some(error) = reply.get("error").filter(|e| !e.is_null()) {
            return Err(McpError::Protocol(
                error["message"].as_str().unwrap_or("an error").to_string(),
            ));
        }
        Ok(reply["result"].clone())
    }

    /// Starts a session. Done again after `SessionLost`.
    pub async fn initialize(&self, token: &str) -> Result<(), McpError> {
        *self.session.lock().unwrap() = None;
        self.rpc(
            token,
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "circles", "version": env!("CARGO_PKG_VERSION") },
            }),
        )
        .await?;
        self.post(
            token,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            false,
        )
        .await?;
        Ok(())
    }

    /// Every tool the server lists, over all pages.
    pub async fn list_tools(&self, token: &str) -> Result<Vec<Value>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = cursor
                .as_ref()
                .map_or_else(|| json!({}), |c| json!({ "cursor": c }));
            let page = self.rpc(token, "tools/list", params).await?;
            tools.extend(page["tools"].as_array().cloned().unwrap_or_default());
            match page["nextCursor"].as_str() {
                Some(next) if !next.is_empty() => cursor = Some(next.to_string()),
                _ => break,
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(
        &self,
        token: &str,
        name: &str,
        arguments: &Value,
    ) -> Result<ToolResult, McpError> {
        let result = self
            .rpc(
                token,
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await?;
        let text = result["content"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(ToolResult {
            text,
            is_error: result["isError"].as_bool().unwrap_or(false),
        })
    }
}
