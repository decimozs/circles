//! The tool server Claude Code starts: Circles itself, run with `--circles-mcp`. It speaks MCP over
//! stdin and stdout and passes every call to the running app over the private socket. It has no
//! tools of its own and no access to anything else.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::server::ask;

/// What the helper talks to. The app's socket, or a fake in tests.
pub trait Backend: Send + Sync {
    fn request(&self, request: Value) -> Result<Value, String>;
}

pub struct SocketBackend {
    pub socket: PathBuf,
    pub token: String,
    pub session: String,
}

impl Backend for SocketBackend {
    fn request(&self, mut request: Value) -> Result<Value, String> {
        request["token"] = json!(self.token);
        request["session"] = json!(self.session);
        ask(&self.socket, &request).map_err(|e| format!("Circles is not reachable: {e}"))
    }
}

/// The answer to one message, or `None` for a notification, which gets none.
pub fn handle(line: &str, backend: &dyn Backend) -> Option<Value> {
    let message: Value = serde_json::from_str(line).ok()?;
    let id = message.get("id").cloned();
    let method = message["method"].as_str().unwrap_or_default();
    // A message without an id is a notification.
    let id = id?;
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": message["params"]["protocolVersion"]
                .as_str()
                .unwrap_or("2024-11-05"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "circles", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => match backend.request(json!({ "method": "list" })) {
            Ok(reply) if reply["ok"] == true => Ok(json!({
                "tools": reply["tools"].as_array().cloned().unwrap_or_default().iter().map(|t| json!({
                    "name": t["name"],
                    "description": t["description"],
                    "inputSchema": t["input_schema"],
                })).collect::<Vec<_>>(),
            })),
            Ok(reply) => Err((
                -32000,
                reply["error"].as_str().unwrap_or("Refused.").to_string(),
            )),
            Err(e) => Err((-32000, e)),
        },
        "tools/call" => {
            let params = &message["params"];
            match backend.request(json!({
                "method": "call",
                "tool": params["name"],
                "arguments": params["arguments"],
            })) {
                Ok(reply) if reply["ok"] == true => Ok(json!({
                    "content": [{ "type": "text", "text": reply["text"] }],
                    "isError": reply["is_error"].as_bool().unwrap_or(false),
                })),
                Ok(reply) => Ok(json!({
                    "content": [{ "type": "text", "text": reply["error"].as_str().unwrap_or("Refused.") }],
                    "isError": true,
                })),
                Err(e) => {
                    Ok(json!({ "content": [{ "type": "text", "text": e }], "isError": true }))
                }
            }
        }
        other => Err((-32601, format!("{other} is not supported."))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

/// Runs until stdin closes. Returns the exit code.
pub fn run_stdio() -> i32 {
    let (Some(socket), Some(token), Some(session)) = (
        std::env::var_os("CIRCLES_SOCKET"),
        std::env::var("CIRCLES_TOKEN").ok(),
        std::env::var("CIRCLES_SESSION_ID").ok(),
    ) else {
        eprintln!(
            "circles --circles-mcp needs CIRCLES_SOCKET, CIRCLES_TOKEN and CIRCLES_SESSION_ID."
        );
        return 2;
    };
    let backend: Arc<dyn Backend> = Arc::new(SocketBackend {
        socket: PathBuf::from(socket),
        token,
        session,
    });
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let mut threads = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        // A call can wait for a person for hours, so each one has its own thread and the others
        // (such as ping) are never held up.
        let backend = backend.clone();
        let out = out.clone();
        threads.push(std::thread::spawn(move || {
            if let Some(reply) = handle(&line, backend.as_ref()) {
                let mut out = out.lock().expect("stdout poisoned");
                let _ = writeln!(out, "{reply}");
                let _ = out.flush();
            }
        }));
    }
    // Stopping stdin means the run ended. Calls still waiting are dropped with the process.
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl Backend for Fake {
        fn request(&self, request: Value) -> Result<Value, String> {
            Ok(match request["method"].as_str() {
                Some("list") => json!({"ok": true, "tools": [
                    {"name": "list_projects", "description": "d", "input_schema": {"type": "object"}}
                ]}),
                Some("call") => {
                    json!({"ok": true, "text": format!("ran {}", request["tool"]), "is_error": false})
                }
                _ => json!({"ok": false, "error": "no"}),
            })
        }
    }

    struct Down;
    impl Backend for Down {
        fn request(&self, _r: Value) -> Result<Value, String> {
            Err("Circles is not reachable: gone".into())
        }
    }

    fn rpc(method: &str, params: Value) -> String {
        json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params}).to_string()
    }

    #[test]
    fn it_introduces_itself_with_the_version_the_client_asked_for() {
        let r = handle(
            &rpc("initialize", json!({"protocolVersion": "2025-03-26"})),
            &Fake,
        )
        .unwrap();
        assert_eq!(r["id"], 7);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "circles");
        assert!(r["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn it_lists_the_tools_the_app_offers() {
        let r = handle(&rpc("tools/list", json!({})), &Fake).unwrap();
        assert_eq!(r["result"]["tools"][0]["name"], "list_projects");
        assert_eq!(r["result"]["tools"][0]["inputSchema"]["type"], "object");
    }

    #[test]
    fn a_call_goes_to_the_app_and_its_text_comes_back() {
        let r = handle(
            &rpc(
                "tools/call",
                json!({"name": "list_projects", "arguments": {}}),
            ),
            &Fake,
        )
        .unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "ran \"list_projects\"");
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn an_app_that_is_gone_is_an_error_the_model_can_read() {
        let r = handle(
            &rpc("tools/call", json!({"name": "x", "arguments": {}})),
            &Down,
        )
        .unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not reachable"));
        let list = handle(&rpc("tools/list", json!({})), &Down).unwrap();
        assert!(list["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not reachable"));
    }

    #[test]
    fn notifications_get_no_answer_and_unknown_methods_an_error() {
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string();
        assert!(handle(&note, &Fake).is_none());
        assert!(handle("not json", &Fake).is_none());
        let r = handle(&rpc("resources/list", json!({})), &Fake).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        assert_eq!(
            handle(&rpc("ping", json!({})), &Fake).unwrap()["result"],
            json!({})
        );
    }
}
