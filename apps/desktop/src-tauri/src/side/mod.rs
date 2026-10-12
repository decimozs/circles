//! A side question: a quick question about a chat, answered in a small pane without touching the
//! chat. It runs a separate, short Claude Code process on a fork of the chat's saved context, with
//! nothing it could change anything with, and nothing is written back.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use crate::env::ResolvedEnv;

pub mod service;

/// How long a side question may take.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// The most of an answer that is kept. A side question is for a short answer.
pub const ANSWER_LIMIT: usize = 8000;

/// The most of a question that is accepted.
pub const QUESTION_LIMIT: usize = 2000;

const SYSTEM_PROMPT: &str = "This is a side question about the conversation so far. Answer briefly and directly. You cannot change anything and you are not continuing the task.";

/// What is needed to ask one.
#[derive(Debug, Clone)]
pub struct SideRequest {
    pub program: PathBuf,
    pub cwd: PathBuf,
    /// The chat's own Claude session, forked. `None` for a chat that never ran: no context then.
    pub resume: Option<String>,
    pub model: Option<String>,
    pub question: String,
    /// Allow reading files in the chat's folder, and nothing else.
    pub read_files: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SideEvent {
    /// More of the answer.
    Delta(String),
    /// The answer is complete. The whole text.
    Done(String),
    Failed(String),
}

/// The command line. Never a tool that changes anything, never the person's connected servers,
/// never their hooks, and never a new session file.
pub fn build_args(req: &SideRequest) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-p".into(),
        req.question.clone(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--include-partial-messages".into(),
        "--no-session-persistence".into(),
        "--strict-mcp-config".into(),
        "--mcp-config".into(),
        r#"{"mcpServers":{}}"#.into(),
        "--settings".into(),
        r#"{"disableAllHooks":true}"#.into(),
        "--append-system-prompt".into(),
        SYSTEM_PROMPT.into(),
        "--tools".into(),
        if req.read_files { "Read,Grep,Glob" } else { "" }.into(),
    ];
    if let Some(session) = &req.resume {
        args.extend(["--resume".into(), session.clone(), "--fork-session".into()]);
    }
    if let Some(model) = req.model.as_deref().filter(|m| !m.is_empty()) {
        args.extend(["--model".into(), model.to_string()]);
    }
    args
}

/// One line of Claude Code's output as an event, when it is one.
pub fn parse_line(line: &str) -> Option<SideEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    match value["type"].as_str()? {
        "stream_event" => {
            let event = &value["event"];
            if event["type"] == "content_block_delta" && event["delta"]["type"] == "text_delta" {
                return event["delta"]["text"]
                    .as_str()
                    .map(|t| SideEvent::Delta(t.into()));
            }
            None
        }
        "result" => {
            if value["is_error"].as_bool() == Some(true) || value["subtype"] != "success" {
                let why = value["result"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Claude Code could not answer.");
                Some(SideEvent::Failed(why.into()))
            } else {
                Some(SideEvent::Done(
                    value["result"].as_str().unwrap_or("").into(),
                ))
            }
        }
        _ => None,
    }
}

/// Runs one side question. Events go to `events` until the end, which is always a `Done` or a
/// `Failed`. Dropping or firing `cancel` stops the process.
pub async fn run(
    req: SideRequest,
    env: Arc<ResolvedEnv>,
    events: mpsc::UnboundedSender<SideEvent>,
    cancel: oneshot::Receiver<()>,
) {
    let outcome = tokio::select! {
        r = run_inner(&req, &env, &events) => r,
        _ = cancel => Err("Stopped.".to_string()),
        _ = tokio::time::sleep(TIMEOUT) => Err("The answer took too long.".to_string()),
    };
    if let Err(message) = outcome {
        let _ = events.send(SideEvent::Failed(message));
    }
}

async fn run_inner(
    req: &SideRequest,
    env: &ResolvedEnv,
    events: &mpsc::UnboundedSender<SideEvent>,
) -> Result<(), String> {
    if !req.cwd.is_dir() {
        return Err("The folder of this chat is not there any more.".into());
    }
    let mut command = Command::new(&req.program);
    env.apply(&mut command);
    command
        .args(build_args(req))
        .current_dir(&req.cwd)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Dropped with the task, so Stop and the time limit end the process.
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start Claude Code: {e}"))?;
    let stdout = child.stdout.take().ok_or("no output")?;
    let mut lines = BufReader::new(stdout).lines();
    let mut kept = 0usize;
    let mut finished = false;
    while let Ok(Some(line)) = lines.next_line().await {
        match parse_line(&line) {
            Some(SideEvent::Delta(text)) => {
                if kept < ANSWER_LIMIT {
                    kept += text.chars().count();
                    let _ = events.send(SideEvent::Delta(text));
                }
            }
            Some(SideEvent::Done(text)) => {
                let text: String = text.chars().take(ANSWER_LIMIT).collect();
                let _ = events.send(SideEvent::Done(text));
                finished = true;
            }
            Some(SideEvent::Failed(message)) => return Err(message),
            None => {}
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if finished {
        return Ok(());
    }
    if status.success() {
        Err("Claude Code gave no answer.".into())
    } else {
        Err("Claude Code stopped before it answered.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn request(program: &std::path::Path, cwd: &std::path::Path) -> SideRequest {
        SideRequest {
            program: program.to_path_buf(),
            cwd: cwd.to_path_buf(),
            resume: Some("abc".into()),
            model: Some("haiku".into()),
            question: "what is this?".into(),
            read_files: false,
        }
    }

    /// A stand-in for `claude` that prints the given lines.
    fn fake(dir: &std::path::Path, body: &str) -> PathBuf {
        let path = dir.join("claude");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    async fn collect(req: SideRequest, stop_after: Option<Duration>) -> Vec<SideEvent> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let env = Arc::new(ResolvedEnv::for_test(&[("PATH", "/usr/bin:/bin")]));
        if let Some(after) = stop_after {
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                let _ = cancel_tx.send(());
            });
        } else {
            std::mem::forget(cancel_tx);
        }
        run(req, env, tx, cancel_rx).await;
        let mut out = Vec::new();
        while let Ok(event) = rx.try_recv() {
            out.push(event);
        }
        out
    }

    #[test]
    fn the_command_line_has_nothing_that_could_change_anything() {
        let args = build_args(&request(
            std::path::Path::new("claude"),
            std::path::Path::new("/"),
        ));
        let has = |a: &str| args.iter().any(|x| x == a);
        assert!(has("--resume") && has("--fork-session"), "{args:?}");
        assert!(has("--no-session-persistence"));
        assert!(has("--strict-mcp-config"));
        assert!(has(r#"{"mcpServers":{}}"#));
        assert!(has(r#"{"disableAllHooks":true}"#));
        // `--tools ""`: the empty list.
        let at = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[at + 1], "");
        assert!(!has("--permission-mode") && !has("--dangerously-skip-permissions"));
        assert!(has("--model") && has("haiku"));
    }

    #[test]
    fn reading_files_allows_only_the_reading_tools() {
        let mut req = request(std::path::Path::new("claude"), std::path::Path::new("/"));
        req.read_files = true;
        let args = build_args(&req);
        let at = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[at + 1], "Read,Grep,Glob");
    }

    #[test]
    fn a_chat_that_never_ran_has_nothing_to_fork() {
        let mut req = request(std::path::Path::new("claude"), std::path::Path::new("/"));
        req.resume = None;
        let args = build_args(&req);
        assert!(!args
            .iter()
            .any(|a| a == "--resume" || a == "--fork-session"));
    }

    #[test]
    fn lines_become_events() {
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hel"}}}"#;
        assert_eq!(parse_line(delta), Some(SideEvent::Delta("Hel".into())));
        let done = r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello"}"#;
        assert_eq!(parse_line(done), Some(SideEvent::Done("Hello".into())));
        let failed =
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"No"}"#;
        assert_eq!(parse_line(failed), Some(SideEvent::Failed("No".into())));
        assert_eq!(parse_line(r#"{"type":"system","subtype":"init"}"#), None);
        assert_eq!(parse_line("not json"), None);
    }

    #[tokio::test]
    async fn an_answer_streams_and_ends_with_done() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(
            dir.path(),
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"The "}}}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"word."}}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"The word."}'"#,
        );
        let events = collect(request(&program, dir.path()), None).await;
        assert_eq!(
            events,
            vec![
                SideEvent::Delta("The ".into()),
                SideEvent::Delta("word.".into()),
                SideEvent::Done("The word.".into()),
            ]
        );
    }

    #[tokio::test]
    async fn a_process_that_dies_without_answering_fails_plainly() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(dir.path(), "exit 3");
        let events = collect(request(&program, dir.path()), None).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SideEvent::Failed(m) if m.contains("stopped before")));
    }

    #[tokio::test]
    async fn stop_ends_it_and_leaves_no_process() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(dir.path(), "sleep 30");
        let started = std::time::Instant::now();
        let events = collect(
            request(&program, dir.path()),
            Some(Duration::from_millis(100)),
        )
        .await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(events, vec![SideEvent::Failed("Stopped.".into())]);
    }

    #[tokio::test]
    async fn a_missing_folder_or_program_fails_plainly() {
        let dir = tempfile::tempdir().unwrap();
        let mut req = request(std::path::Path::new("/nonexistent/claude"), dir.path());
        let events = collect(req.clone(), None).await;
        assert!(matches!(&events[0], SideEvent::Failed(m) if m.contains("Cannot start")));
        req.cwd = dir.path().join("gone");
        let events = collect(req, None).await;
        assert!(matches!(&events[0], SideEvent::Failed(m) if m.contains("not there")));
    }

    /// With the real `claude`. Needs a saved session of it to fork:
    /// `CIRCLES_REAL_CLAUDE_SESSION=<id> CIRCLES_REAL_CLAUDE_DIR=<its folder> cargo test real_claude -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn real_claude_answers_a_side_question_from_the_forked_context() {
        let (Some(session), Some(dir)) = (
            std::env::var("CIRCLES_REAL_CLAUDE_SESSION").ok(),
            std::env::var("CIRCLES_REAL_CLAUDE_DIR").ok(),
        ) else {
            return;
        };
        let program = std::process::Command::new("sh")
            .args(["-c", "command -v claude"])
            .output()
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string()))
            .unwrap();
        let vars: Vec<(String, String)> = std::env::vars().collect();
        let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let env = Arc::new(ResolvedEnv::for_test(&pairs));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (_keep, cancel) = oneshot::channel();
        let req = SideRequest {
            program,
            cwd: PathBuf::from(dir),
            resume: Some(session),
            model: Some("haiku".into()),
            question: "What was the secret word? One word.".into(),
            read_files: false,
        };
        let started = std::time::Instant::now();
        run(req, env, tx, cancel).await;
        let mut first = None;
        let mut last = None;
        while let Ok(event) = rx.try_recv() {
            if first.is_none() {
                first = Some(started.elapsed());
            }
            last = Some(event);
        }
        println!("LAST: {last:?} in {:.1}s", started.elapsed().as_secs_f32());
        assert!(matches!(last, Some(SideEvent::Done(t)) if t.to_lowercase().contains("kumquat")));
    }
}
