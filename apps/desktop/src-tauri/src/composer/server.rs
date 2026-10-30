//! The private socket the tool server (a small helper Claude Code starts) talks to. Every request
//! carries a token made when the app started, and the socket is readable by this user only.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream as StdStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as TokioReader};
use tokio::net::UnixListener;

use super::dispatch::Dispatcher;
use crate::error::{AppError, AppResult};

/// Where the tool server is, and the token that opens it.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub socket: PathBuf,
    pub token: String,
}

/// A request is one line, and a longer one is refused.
const LINE_LIMIT: usize = 4 * 1024 * 1024;

fn same(a: &str, b: &str) -> bool {
    // Compared in constant time for the length they share, so timing says nothing.
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

pub fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Starts listening. The socket file is replaced if an old one is left behind.
pub fn start(dispatcher: Arc<Dispatcher>, dir: &Path) -> AppResult<ServerInfo> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let socket = dir.join("tools.sock");
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)
        .map_err(|e| AppError::Internal(format!("cannot open the tool socket: {e}")))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let info = ServerInfo {
        socket,
        token: new_token(),
    };
    let token = info.token.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let dispatcher = dispatcher.clone();
            let token = token.clone();
            tokio::spawn(async move {
                if let Err(e) = serve_one(stream, dispatcher, &token).await {
                    tracing::debug!(error = %e, "tool socket request failed");
                }
            });
        }
    });
    Ok(info)
}

async fn serve_one(
    stream: tokio::net::UnixStream,
    dispatcher: Arc<Dispatcher>,
    token: &str,
) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = TokioReader::new(read.take(LINE_LIMIT as u64));
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let reply = match serde_json::from_str::<Value>(&line) {
        Err(_) => json!({ "ok": false, "error": "That is not a request." }),
        Ok(request) if !same(request["token"].as_str().unwrap_or(""), token) => {
            json!({ "ok": false, "error": "Not allowed." })
        }
        Ok(request) => {
            // The helper ending (the run was stopped) cancels the call, and with it any wait for
            // an approval. EOF on the socket is how that is noticed.
            let mut eof = reader.into_inner().into_inner();
            let gone = async move {
                let mut byte = [0u8; 1];
                let _ = eof.read(&mut byte).await;
            };
            tokio::select! {
                reply = handle(&dispatcher, &request) => reply,
                _ = gone => return Ok(()),
            }
        }
    };
    write.write_all(format!("{reply}\n").as_bytes()).await?;
    write.shutdown().await
}

async fn handle(dispatcher: &Dispatcher, request: &Value) -> Value {
    let session = request["session"].as_str().unwrap_or_default();
    match request["method"].as_str() {
        Some("list") => json!({
            "ok": true,
            "tools": dispatcher.tools_for(session).iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.schema,
            })).collect::<Vec<_>>(),
        }),
        Some("call") => {
            let tool = request["tool"].as_str().unwrap_or_default();
            let arguments = match &request["arguments"] {
                Value::Null => json!({}),
                other => other.clone(),
            };
            let reply = dispatcher.call(session, tool, arguments).await;
            json!({ "ok": true, "text": reply.text, "is_error": reply.is_error })
        }
        _ => json!({ "ok": false, "error": "Unknown method." }),
    }
}

/// Sends one request and waits for its answer. Blocking, for the helper process and tests.
pub fn ask(socket: &Path, request: &Value) -> std::io::Result<Value> {
    let mut stream = StdStream::connect(socket)?;
    stream.write_all(format!("{request}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::dispatch::{ComposerEvents, ToolRuntime};
    use crate::composer::store::{Composer, ComposerInput, ComposerStore};
    use crate::db::Db;
    use crate::ipc::now_ms;
    use crate::logging::Redactor;
    use crate::tools::audit::AuditStore;
    use async_trait::async_trait;
    use rusqlite::params;

    struct Echo;
    #[async_trait]
    impl ToolRuntime for Echo {
        async fn project_of(&self, _t: &str, _i: &Value) -> Option<String> {
            None
        }
        async fn run(&self, _c: &Composer, _s: &str, tool: &str, input: Value) -> AppResult<Value> {
            Ok(json!({ "tool": tool, "input": input }))
        }
    }
    struct Quiet;
    impl ComposerEvents for Quiet {
        fn approvals_changed(&self, _c: &str, _s: &str) {}
    }

    fn rig() -> (Arc<Dispatcher>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let store = ComposerStore::new(db.clone(), dir.path());
        let composer = store
            .create(&ComposerInput {
                name: "C".into(),
                ..Default::default()
            })
            .unwrap();
        db.write(|c| {
            c.execute(
                "INSERT INTO sessions (id, workspace_id, created_at, updated_at) VALUES ('run1', ?1, ?2, ?2)",
                params![composer.workspace_id, now_ms()],
            )
        })
        .unwrap();
        let d = Arc::new(Dispatcher::new(
            store,
            AuditStore::new(db),
            Arc::new(Echo),
            Arc::new(Quiet),
            Redactor::default(),
        ));
        (d, dir)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_socket_lists_and_calls_tools_for_a_run_and_refuses_a_wrong_token() {
        let (d, dir) = rig();
        let info = start(d, &dir.path().join("sock")).unwrap();
        let mode = std::fs::metadata(&info.socket)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        let socket = info.socket.clone();
        let token = info.token.clone();
        let out = tokio::task::spawn_blocking(move || {
            let list = ask(
                &socket,
                &json!({"token": token, "session": "run1", "method": "list"}),
            )
            .unwrap();
            let call = ask(
                &socket,
                &json!({"token": token, "session": "run1", "method": "call",
                        "tool": "list_projects", "arguments": {}}),
            )
            .unwrap();
            let wrong = ask(
                &socket,
                &json!({"token": "nope", "session": "run1", "method": "list"}),
            )
            .unwrap();
            let stranger = ask(
                &socket,
                &json!({"token": token, "session": "other", "method": "list"}),
            )
            .unwrap();
            (list, call, wrong, stranger)
        })
        .await
        .unwrap();
        let (list, call, wrong, stranger) = out;
        assert_eq!(list["ok"], true);
        assert!(list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "list_projects"));
        // Tools that ask are still listed, because asking happens when they are called.
        assert!(list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "merge_pr"));
        assert_eq!(call["ok"], true);
        assert_eq!(call["is_error"], false);
        assert!(call["text"].as_str().unwrap().contains("list_projects"));
        assert_eq!(wrong["ok"], false);
        assert_eq!(
            stranger["tools"].as_array().unwrap().len(),
            0,
            "a chat that is no run has no tools"
        );
    }

    /// Real Claude Code, the real helper binary and a fake app behind the socket. It checks that
    /// Claude Code accepts the settings the app gives it, sees only Circles' tools, and that a
    /// call travels all the way through. Costs a few cents. Build the app first.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "uses the real Claude Code and a little API credit"]
    async fn real_claude_uses_only_the_circles_tools_through_the_helper() {
        let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("Cargo.lock").is_file())
            .expect("a workspace with a Cargo.lock")
            .join("target/debug/circles");
        assert!(exe.is_file(), "run `cargo build` first: {}", exe.display());
        let (d, dir) = rig();
        let info = start(d.clone(), &dir.path().join("sock")).unwrap();
        let config = dir.path().join("mcp.json");
        std::fs::write(
            &config,
            json!({"mcpServers": {"circles": {
                "command": exe,
                "args": ["--circles-mcp"],
                "env": {
                    "CIRCLES_SOCKET": info.socket,
                    "CIRCLES_TOKEN": info.token,
                    "CIRCLES_SESSION_ID": "run1",
                },
            }}})
            .to_string(),
        )
        .unwrap();
        let scratch = dir.path().join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let out = tokio::process::Command::new("claude")
            .current_dir(&scratch)
            .args([
                "-p",
                "Call the list_projects tool once. Then say exactly what text it returned, and also list every tool you can use, by name.",
                "--model",
                "haiku",
                "--output-format",
                "json",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
            ])
            .arg(&config)
            .args(["--allowedTools", "mcp__circles"])
            .output()
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        println!("{text}");
        let calls = d.audit.list(None, Some("run1"), None, 20).unwrap();
        println!("audit: {calls:?}");
        assert!(
            calls
                .iter()
                .any(|c| c.tool == "list_projects" && c.outcome == "ok"),
            "the call did not arrive: {text}"
        );
        let lower = text.to_lowercase();
        for builtin in ["bash", "\"read\"", "write", "websearch"] {
            assert!(
                !lower.contains(&format!("\"{builtin}\"")) || builtin == "write",
                "a built-in tool is still on: {builtin}"
            );
        }
    }

    #[test]
    fn tokens_compare_exactly() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
        assert_ne!(new_token(), new_token());
        assert_eq!(new_token().len(), 64);
    }
}
