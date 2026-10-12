//! Claude Code as a child process, driven over `stream-json`.
//!
//! One process serves one conversation across many turns. Messages go in on stdin as JSON lines,
//! and events come out on stdout the same way. The process runs in its own process group, so
//! stopping it also stops everything its tools started.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};

use super::permissions::{response_line, PermissionDecision, PermissionRequest};
use super::store::PermissionMode;
use super::{Harness, HarnessProcess, HarnessSink, Launch, ProcessExit, StartSpec};
use crate::db::Db;
use crate::env::{EnvResolver, ResolvedEnv};
use crate::error::{AppError, AppResult};
use crate::ipc::new_id;
use crate::process::{self, ProcessRecord};

const STDERR_LINES: usize = 20;
/// After the process ends, how long to wait for the last output to be read.
const OUTPUT_DRAIN: Duration = Duration::from_millis(500);
/// How long a process gets to end by itself once its input is closed.
const CLOSE_GRACE: Duration = Duration::from_millis(1500);

/// The login environment, either known now or resolved when first needed. The app starts
/// before the environment is read, so it uses the lazy form.
enum EnvSource {
    Ready(Arc<ResolvedEnv>),
    Lazy(EnvResolver),
}

pub struct ClaudeCode {
    env: EnvSource,
    db: Option<Arc<Db>>,
}

impl ClaudeCode {
    pub fn new(env: Arc<ResolvedEnv>, db: Option<Arc<Db>>) -> Self {
        Self {
            env: EnvSource::Ready(env),
            db,
        }
    }

    pub fn lazy(resolver: EnvResolver, db: Option<Arc<Db>>) -> Self {
        Self {
            env: EnvSource::Lazy(resolver),
            db,
        }
    }

    async fn env(&self) -> Arc<ResolvedEnv> {
        match &self.env {
            EnvSource::Ready(env) => env.clone(),
            EnvSource::Lazy(resolver) => resolver.get().await,
        }
    }
}

/// The command line for a session.
pub fn build_args(spec: &StartSpec) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        spec.permission_mode.cli_value(),
        "--permission-prompt-tool",
        "stdio",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    match &spec.launch {
        Launch::New { claude_session_id } => {
            args.extend(["--session-id".into(), claude_session_id.clone()]);
        }
        Launch::Resume { claude_session_id } => {
            args.extend(["--resume".into(), claude_session_id.clone()]);
        }
        Launch::Fork {
            from,
            claude_session_id,
        } => {
            args.extend([
                "--resume".into(),
                from.clone(),
                "--fork-session".into(),
                "--session-id".into(),
                claude_session_id.clone(),
            ]);
        }
    }
    if let Some(model) = spec.model.as_deref().filter(|m| !m.is_empty()) {
        args.extend(["--model".into(), model.to_string()]);
    }
    if let Some(effort) = spec.effort.as_deref().filter(|e| !e.is_empty()) {
        args.extend(["--effort".into(), effort.to_string()]);
    }
    args.extend(
        spec.extra_args
            .iter()
            .filter(|a| !a.trim().is_empty())
            .cloned(),
    );
    args.extend(spec.fixed_args.iter().cloned());
    args
}

/// A message for Claude. Newline terminated.
pub fn user_message_line(text: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({ "type": "user", "message": { "role": "user", "content": text } })
    )
}

fn control_request_line(request: serde_json::Value) -> String {
    format!(
        "{}\n",
        serde_json::json!({ "type": "control_request", "request_id": new_id(), "request": request })
    )
}

pub fn interrupt_line() -> String {
    control_request_line(serde_json::json!({ "subtype": "interrupt" }))
}

/// Tells Claude Code that a control request is not supported, so it stops waiting for an answer.
pub fn reject_line(request_id: &str, reason: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "type": "control_response",
            "response": { "subtype": "error", "request_id": request_id, "error": reason },
        })
    )
}

pub fn permission_mode_line(mode: PermissionMode) -> String {
    control_request_line(
        serde_json::json!({ "subtype": "set_permission_mode", "mode": mode.cli_value() }),
    )
}

pub struct ClaudeProcess {
    pid: i32,
    stdin: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
    exited: watch::Receiver<Option<ProcessExit>>,
}

fn claude_missing() -> AppError {
    AppError::ClaudeMissing(
        "Claude Code (`claude`) was not found. Install it from https://claude.com/claude-code, then run `claude` once to sign in."
            .into(),
    )
}

#[async_trait]
impl Harness for ClaudeCode {
    async fn start(
        &self,
        spec: StartSpec,
        sink: Arc<dyn HarnessSink>,
    ) -> AppResult<Arc<dyn HarnessProcess>> {
        let env = self.env().await;
        let bin = match &spec.claude_path {
            Some(path) if path.is_file() => path.clone(),
            Some(path) => {
                return Err(AppError::ClaudeMissing(format!(
                    "The Claude Code you chose in the settings was not found at {}.",
                    path.display()
                )))
            }
            None => env.which("claude").ok_or_else(claude_missing)?,
        };
        if !spec.cwd.is_dir() {
            return Err(AppError::Invalid(format!(
                "The folder {} does not exist, so the agent cannot start there.",
                spec.cwd.display()
            )));
        }

        let mut cmd = Command::new(bin);
        cmd.args(build_args(&spec))
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        env.apply(&mut cmd);
        cmd.env("NO_COLOR", "1");
        for (key, value) in &spec.env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Internal(format!("cannot start Claude Code: {e}")))?;
        let pid = child
            .id()
            .and_then(|p| i32::try_from(p).ok())
            .ok_or_else(|| {
                AppError::Internal("Claude Code started but has no process id".into())
            })?;
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        if let Some(db) = &self.db {
            let record = ProcessRecord::new(pid, "agent", Some(&spec.session_id), "claude");
            if let Err(e) = process::register(db, &record) {
                tracing::warn!(error = %e, "cannot record the agent process");
            }
        }

        // Input: one task owns stdin, so writes stay in order and never block the caller.
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(bytes) = stdin_rx.recv().await {
                if stdin.write_all(&bytes).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
            // Dropping stdin here closes it, which tells Claude Code to finish.
        });

        // Output: lines go to the sink as they arrive.
        let session_id = spec.session_id.clone();
        let line_sink = sink.clone();
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    line_sink.line(&session_id, &line);
                }
            }
        });

        // Errors: keep the last few lines for the message when the process dies.
        let tail: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let tail_for_task = tail.clone();
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut tail = tail_for_task.lock().expect("stderr tail poisoned");
                if tail.len() == STDERR_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });

        let (exit_tx, exit_rx) = watch::channel(None);
        let session_id = spec.session_id.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            // A process that hands its stdout to a background child keeps it open, so the
            // output is waited for only briefly.
            let _ = tokio::time::timeout(OUTPUT_DRAIN, reader).await;
            let _ = tokio::time::timeout(OUTPUT_DRAIN, stderr_task).await;
            let exit = {
                use std::os::unix::process::ExitStatusExt;
                let (code, signaled) = match &status {
                    Ok(s) => (s.code(), s.signal().is_some()),
                    Err(_) => (None, false),
                };
                ProcessExit {
                    code,
                    signaled,
                    stderr_tail: tail
                        .lock()
                        .expect("stderr tail poisoned")
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n"),
                }
            };
            if let Some(db) = &db {
                let _ = process::unregister(db, pid);
            }
            sink.exited(&session_id, exit.clone());
            let _ = exit_tx.send(Some(exit));
        });

        Ok(Arc::new(ClaudeProcess {
            pid,
            stdin: Mutex::new(Some(stdin_tx)),
            exited: exit_rx,
        }))
    }
}

impl ClaudeProcess {
    fn write(&self, line: String) -> AppResult<()> {
        // Once the process ended, nothing reads the queue, so a send would be lost without a word.
        if !self.is_running() {
            return Err(AppError::Invalid("The agent process has ended.".into()));
        }
        let guard = self.stdin.lock().expect("stdin poisoned");
        let tx = guard
            .as_ref()
            .ok_or_else(|| AppError::Invalid("The agent process has ended.".into()))?;
        tx.send(line.into_bytes())
            .map_err(|_| AppError::Invalid("The agent process has ended.".into()))
    }

    fn signal_group(&self, signal: i32) {
        // SAFETY: killpg only sends a signal. The pid leads the group we created for it.
        unsafe {
            libc::killpg(self.pid, signal);
        }
    }

    fn group_alive(&self) -> bool {
        // SAFETY: signal 0 only checks that the group exists.
        let rc = unsafe { libc::killpg(self.pid, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    async fn wait_for_exit(&self, within: Duration) -> bool {
        let mut rx = self.exited.clone();
        tokio::time::timeout(within, async {
            while rx.borrow().is_none() {
                if rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .is_ok()
    }
}

#[async_trait]
impl HarnessProcess for ClaudeProcess {
    fn pid(&self) -> i32 {
        self.pid
    }

    fn is_running(&self) -> bool {
        self.exited.borrow().is_none()
    }

    fn send_user(&self, text: &str) -> AppResult<String> {
        let line = user_message_line(text);
        self.write(line.clone())?;
        Ok(line)
    }

    fn interrupt(&self) -> AppResult<String> {
        let line = interrupt_line();
        self.write(line.clone())?;
        Ok(line)
    }

    fn answer_permission(
        &self,
        request: &PermissionRequest,
        decision: &PermissionDecision,
    ) -> AppResult<String> {
        let line = response_line(request, decision);
        self.write(line.clone())?;
        Ok(line)
    }

    fn reject_control(&self, request_id: &str, reason: &str) -> AppResult<String> {
        let line = reject_line(request_id, reason);
        self.write(line.clone())?;
        Ok(line)
    }

    fn set_permission_mode(&self, mode: PermissionMode) -> AppResult<String> {
        let line = permission_mode_line(mode);
        self.write(line.clone())?;
        Ok(line)
    }

    async fn stop(&self, grace: Duration) -> Option<ProcessExit> {
        // Close the input first: a process that is idle ends by itself. Then the group.
        self.stdin.lock().expect("stdin poisoned").take();
        if !self.wait_for_exit(CLOSE_GRACE).await || self.group_alive() {
            self.signal_group(libc::SIGTERM);
            let ended = self.wait_for_exit(grace).await;
            if !ended || self.group_alive() {
                let deadline =
                    tokio::time::Instant::now() + if ended { grace } else { Duration::ZERO };
                while self.group_alive() && tokio::time::Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                if self.group_alive() {
                    self.signal_group(libc::SIGKILL);
                }
                self.wait_for_exit(Duration::from_secs(2)).await;
            }
        }
        self.exited.borrow().clone()
    }

    fn kill_now(&self) {
        self.signal_group(libc::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn spec(launch: Launch) -> StartSpec {
        StartSpec {
            session_id: "s1".into(),
            cwd: std::env::temp_dir(),
            env: vec![],
            launch,
            model: None,
            effort: None,
            permission_mode: PermissionMode::Manual,
            claude_path: None,
            extra_args: vec![],
            fixed_args: vec![],
        }
    }

    #[test]
    fn extra_arguments_from_the_settings_go_on_the_end_without_empty_ones() {
        let plain = spec(Launch::New {
            claude_session_id: "abc".into(),
        });
        assert!(build_args(&plain).iter().all(|a| a != "--add-dir"));
        let mut with = spec(Launch::New {
            claude_session_id: "abc".into(),
        });
        with.extra_args = vec![
            "--add-dir".into(),
            "/data".into(),
            "  ".into(),
            "--verbose".into(),
        ];
        let args = build_args(&with);
        assert_eq!(args[args.len() - 3..], ["--add-dir", "/data", "--verbose"]);
        assert!(!args.iter().any(|a| a.trim().is_empty()));
    }

    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn a_new_session_streams_json_and_asks_the_host_for_permissions() {
        let args = build_args(&spec(Launch::New {
            claude_session_id: "abc".into(),
        }));
        assert_eq!(args[0], "-p");
        assert!(has_pair(&args, "--input-format", "stream-json"));
        assert!(has_pair(&args, "--output-format", "stream-json"));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--include-partial-messages".to_string()));
        assert!(has_pair(&args, "--permission-mode", "manual"));
        assert!(has_pair(&args, "--permission-prompt-tool", "stdio"));
        assert!(has_pair(&args, "--session-id", "abc"));
        assert!(!args.contains(&"--resume".to_string()));
        assert!(!args.contains(&"--model".to_string()) && !args.contains(&"--effort".to_string()));
    }

    #[test]
    fn resuming_continues_the_same_conversation() {
        let args = build_args(&spec(Launch::Resume {
            claude_session_id: "abc".into(),
        }));
        assert!(has_pair(&args, "--resume", "abc"));
        assert!(!args.contains(&"--session-id".to_string()));
        assert!(!args.contains(&"--fork-session".to_string()));
    }

    #[test]
    fn forking_copies_a_conversation_into_a_new_one() {
        let args = build_args(&spec(Launch::Fork {
            from: "old".into(),
            claude_session_id: "new".into(),
        }));
        assert!(has_pair(&args, "--resume", "old"));
        assert!(args.contains(&"--fork-session".to_string()));
        assert!(has_pair(&args, "--session-id", "new"));
    }

    #[test]
    fn model_effort_and_permission_mode_are_passed_through() {
        let mut s = spec(Launch::New {
            claude_session_id: "abc".into(),
        });
        s.model = Some("opus".into());
        s.effort = Some("high".into());
        s.permission_mode = PermissionMode::AcceptEdits;
        let args = build_args(&s);
        assert!(has_pair(&args, "--model", "opus"));
        assert!(has_pair(&args, "--effort", "high"));
        assert!(has_pair(&args, "--permission-mode", "acceptEdits"));

        s.permission_mode = PermissionMode::Bypass;
        assert!(has_pair(
            &build_args(&s),
            "--permission-mode",
            "bypassPermissions"
        ));
        s.permission_mode = PermissionMode::Plan;
        assert!(has_pair(&build_args(&s), "--permission-mode", "plan"));

        s.model = Some(String::new());
        s.effort = None;
        let args = build_args(&s);
        assert!(
            !args.contains(&"--model".to_string()),
            "an empty model is not passed"
        );
    }

    #[test]
    fn messages_are_one_json_line() {
        let line = user_message_line("Add a \"dark mode\"\nswitch");
        assert!(
            line.ends_with('\n') && line.matches('\n').count() == 1,
            "newlines in text are escaped"
        );
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["role"], "user");
        assert_eq!(v["message"]["content"], "Add a \"dark mode\"\nswitch");
    }

    #[test]
    fn control_requests_have_an_id_and_a_subtype() {
        let v: Value = serde_json::from_str(interrupt_line().trim()).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(v["request"]["subtype"], "interrupt");
        assert!(!v["request_id"].as_str().unwrap().is_empty());
        let other: Value = serde_json::from_str(interrupt_line().trim()).unwrap();
        assert_ne!(
            v["request_id"], other["request_id"],
            "every request gets its own id"
        );

        let mode: Value =
            serde_json::from_str(permission_mode_line(PermissionMode::AcceptEdits).trim()).unwrap();
        assert_eq!(mode["request"]["subtype"], "set_permission_mode");
        assert_eq!(mode["request"]["mode"], "acceptEdits");
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
    use crate::harness::parser::{parse_line, ControlKind, Event};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    const FAKE: &str = include_str!("fixtures/fake_claude.py");

    #[derive(Default)]
    struct Collect {
        lines: Mutex<Vec<String>>,
        exits: Mutex<Vec<(ProcessExit, usize)>>,
    }

    impl Collect {
        fn lines(&self) -> Vec<String> {
            self.lines.lock().unwrap().clone()
        }
        fn kinds(&self) -> Vec<String> {
            self.lines()
                .iter()
                .map(|l| parse_line(l).unwrap().kind)
                .collect()
        }
        fn exit(&self) -> Option<(ProcessExit, usize)> {
            self.exits.lock().unwrap().first().cloned()
        }
        fn count(&self, kind: &str) -> usize {
            self.kinds().iter().filter(|k| k.as_str() == kind).count()
        }
    }

    impl HarnessSink for Collect {
        fn line(&self, _session_id: &str, raw: &str) {
            self.lines.lock().unwrap().push(raw.to_string());
        }
        fn exited(&self, _session_id: &str, exit: ProcessExit) {
            let seen = self.lines.lock().unwrap().len();
            self.exits.lock().unwrap().push((exit, seen));
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        log: PathBuf,
        harness: ClaudeCode,
        db: Arc<Db>,
    }

    fn fixture() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(tmp.path()).unwrap();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(&claude, FAKE).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = dir.join("fake.log");
        // The fake comes first, so the real `claude` is never the one that runs.
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        assert_eq!(
            crate::env::which_in(&path, "claude"),
            Some(bin.join("claude"))
        );
        let env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("HOME", "/tmp"),
            ("FAKE_CLAUDE_LOG", log.to_str().unwrap()),
        ]);
        let db = Arc::new(Db::open(&dir.join("t.db")).unwrap());
        Fx {
            _tmp: tmp,
            dir,
            log,
            harness: ClaudeCode::new(Arc::new(env), Some(db.clone())),
            db,
        }
    }

    fn spec(fx: &Fx, launch: Launch) -> StartSpec {
        StartSpec {
            session_id: "s1".into(),
            cwd: fx.dir.clone(),
            env: vec![("CIRCLES_PORT".into(), "55010".into())],
            launch,
            model: None,
            effort: None,
            permission_mode: PermissionMode::Manual,
            claude_path: None,
            extra_args: vec![],
            fixed_args: vec![],
        }
    }

    async fn start(fx: &Fx) -> (Arc<dyn HarnessProcess>, Arc<Collect>) {
        start_with(
            fx,
            Launch::New {
                claude_session_id: "11111111-1111-1111-1111-111111111111".into(),
            },
        )
        .await
    }

    async fn start_with(fx: &Fx, launch: Launch) -> (Arc<dyn HarnessProcess>, Arc<Collect>) {
        let sink = Arc::new(Collect::default());
        let process = fx
            .harness
            .start(spec(fx, launch), sink.clone())
            .await
            .unwrap();
        (process, sink)
    }

    async fn until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn log_entries(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn request_id_of(raw: &str) -> String {
        let Event::ControlRequest(r) = parse_line(raw).unwrap().event else {
            panic!("control request expected")
        };
        r.request_id
    }

    #[tokio::test]
    async fn a_turn_flows_from_message_to_result() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("echo hello there").unwrap();
        until("the result", || sink.count("result:success") == 1).await;
        assert_eq!(
            sink.kinds(),
            [
                "system:init",
                "rate_limit_event",
                "assistant",
                "result:success"
            ]
        );
        let assistant = sink
            .lines()
            .iter()
            .find(|l| l.contains("echo: hello there"))
            .cloned();
        assert!(assistant.is_some());
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn the_process_gets_the_documented_flags_directory_and_environment() {
        let fx = fixture();
        let mut s = spec(
            &fx,
            Launch::New {
                claude_session_id: "22222222-2222-2222-2222-222222222222".into(),
            },
        );
        s.model = Some("opus".into());
        s.effort = Some("high".into());
        s.permission_mode = PermissionMode::AcceptEdits;
        let sink = Arc::new(Collect::default());
        let process = fx.harness.start(s, sink.clone()).await.unwrap();
        process.send_user("echo x").unwrap();
        until("a result", || sink.count("result:success") == 1).await;

        let first = &log_entries(&fx.log)[0];
        let argv: Vec<&str> = first["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert!(argv
            .windows(2)
            .any(|w| w == ["--session-id", "22222222-2222-2222-2222-222222222222"]));
        assert!(argv.windows(2).any(|w| w == ["--model", "opus"]));
        assert!(argv.windows(2).any(|w| w == ["--effort", "high"]));
        assert!(argv
            .windows(2)
            .any(|w| w == ["--permission-mode", "acceptEdits"]));
        assert!(argv
            .windows(2)
            .any(|w| w == ["--permission-prompt-tool", "stdio"]));
        assert_eq!(first["cwd"], fx.dir.to_str().unwrap());
        assert_eq!(first["env"]["CIRCLES_PORT"], "55010");
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn resume_and_fork_pass_the_right_flags() {
        let fx = fixture();
        let (a, _) = start_with(
            &fx,
            Launch::Resume {
                claude_session_id: "old".into(),
            },
        )
        .await;
        let (b, _) = start_with(
            &fx,
            Launch::Fork {
                from: "old".into(),
                claude_session_id: "33333333-3333-3333-3333-333333333333".into(),
            },
        )
        .await;
        until("both to start", || log_entries(&fx.log).len() >= 2).await;
        let entries = log_entries(&fx.log);
        let argvs: Vec<Vec<String>> = entries
            .iter()
            .map(|e| {
                e["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect()
            })
            .collect();
        let resume = argvs
            .iter()
            .find(|a| !a.contains(&"--fork-session".to_string()))
            .unwrap();
        assert!(resume.windows(2).any(|w| w == ["--resume", "old"]));
        let fork = argvs
            .iter()
            .find(|a| a.contains(&"--fork-session".to_string()))
            .unwrap();
        assert!(fork.windows(2).any(|w| w == ["--resume", "old"]));
        assert!(fork
            .windows(2)
            .any(|w| w == ["--session-id", "33333333-3333-3333-3333-333333333333"]));
        a.stop(Duration::from_secs(2)).await;
        b.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn one_process_serves_many_turns() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        let pid = process.pid();
        for word in ["one", "two", "three"] {
            process.send_user(&format!("echo {word}")).unwrap();
            let expected = sink.count("result:success") + 1;
            until("the turn", || sink.count("result:success") == expected).await;
        }
        assert_eq!(process.pid(), pid);
        assert_eq!(sink.count("assistant"), 3);
        assert_eq!(
            log_entries(&fx.log)
                .iter()
                .filter(|e| e.get("argv").is_some())
                .count(),
            1,
            "started once"
        );
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn text_streams_in_before_the_final_message() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("stream").unwrap();
        until("the result", || sink.count("result:success") == 1).await;
        let kinds = sink.kinds();
        let first_delta = kinds
            .iter()
            .position(|k| k == "stream_event:content_block_delta")
            .unwrap();
        let assistant = kinds.iter().position(|k| k == "assistant").unwrap();
        assert!(first_delta < assistant);
        assert_eq!(
            kinds
                .iter()
                .filter(|k| *k == "stream_event:content_block_delta")
                .count(),
            3
        );
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn interrupting_stops_the_turn_and_leaves_the_process_usable() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("slow").unwrap();
        until("the turn to be running", || {
            sink.count("stream_event:content_block_delta") >= 2
        })
        .await;

        process.interrupt().unwrap();
        until("the turn to end", || sink.count("result:success") == 1).await;
        assert_eq!(
            sink.count("control_response"),
            1,
            "the interrupt is acknowledged"
        );
        assert!(sink
            .lines()
            .iter()
            .any(|l| l.contains("[Request interrupted by user]")));
        assert!(process.is_running(), "the process stays up");

        process.send_user("echo still here").unwrap();
        until("the next turn", || sink.count("result:success") == 2).await;
        assert!(sink.lines().iter().any(|l| l.contains("echo: still here")));
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn a_message_sent_during_a_turn_steers_it() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("steer").unwrap();
        until("the turn to be running", || {
            sink.count("stream_event:content_block_delta") >= 1
        })
        .await;
        process.send_user("use tabs").unwrap();
        until("the turn to end", || sink.count("result:success") == 1).await;
        assert!(
            sink.lines().iter().any(|l| l.contains("steered:use tabs")),
            "{:?}",
            sink.lines().last()
        );
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn a_permission_request_can_be_allowed() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("tool").unwrap();
        until("the permission request", || {
            sink.count("control_request:can_use_tool") == 1
        })
        .await;
        let raw = sink
            .lines()
            .into_iter()
            .find(|l| l.contains("can_use_tool"))
            .unwrap();
        let Event::ControlRequest(request) = parse_line(&raw).unwrap().event else {
            panic!()
        };
        let ControlKind::CanUseTool {
            tool_name, input, ..
        } = &request.kind
        else {
            panic!()
        };
        assert_eq!(tool_name, "Write");
        assert!(input["file_path"].as_str().unwrap().ends_with("out.txt"));
        assert_eq!(
            sink.count("result:success"),
            0,
            "the turn waits for the answer"
        );

        let pending = PermissionRequest::from_control("s1", &request).unwrap();
        process
            .answer_permission(&pending, &PermissionDecision::Allow)
            .unwrap();
        until("the turn to finish", || sink.count("result:success") == 1).await;
        assert!(sink.lines().iter().any(|l| l.contains("Write done")));
        let received = log_entries(&fx.log)
            .into_iter()
            .find_map(|e| {
                e.get("received")
                    .cloned()
                    .filter(|m| m["type"] == "control_response")
            })
            .unwrap();
        assert_eq!(received["response"]["response"]["behavior"], "allow");
        assert_eq!(received["response"]["request_id"], request_id_of(&raw));
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn a_permission_request_can_be_denied_with_a_reason() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("bash rm -rf build").unwrap();
        until("the permission request", || {
            sink.count("control_request:can_use_tool") == 1
        })
        .await;
        let raw = sink
            .lines()
            .into_iter()
            .find(|l| l.contains("can_use_tool"))
            .unwrap();
        let Event::ControlRequest(request) = parse_line(&raw).unwrap().event else {
            panic!()
        };
        let pending = PermissionRequest::from_control("s1", &request).unwrap();
        assert_eq!(pending.input["command"], "rm -rf build");

        process
            .answer_permission(
                &pending,
                &PermissionDecision::Deny {
                    message: Some("Do not delete build".into()),
                },
            )
            .unwrap();
        until("the turn to finish", || sink.count("result:success") == 1).await;
        let tool_result = sink
            .lines()
            .into_iter()
            .find(|l| l.contains("Do not delete build"))
            .expect("Claude sees the reason");
        assert!(tool_result.contains("\"is_error\": true"));
        assert!(sink.lines().iter().any(|l| l.contains("Bash denied")));
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn the_permission_mode_can_be_changed_while_running() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.set_permission_mode(PermissionMode::Plan).unwrap();
        until("the acknowledgement", || {
            sink.count("control_response") == 1
        })
        .await;
        let set = log_entries(&fx.log)
            .into_iter()
            .find_map(|e| e.get("mode_set").cloned())
            .unwrap();
        assert_eq!(set, "plan");
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn a_crash_is_reported_with_its_exit_code_and_error_output_after_all_output() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("crash").unwrap();
        until("the exit", || sink.exit().is_some()).await;
        let (exit, lines_seen) = sink.exit().unwrap();
        assert_eq!(exit.code, Some(3));
        assert!(!exit.signaled);
        assert!(
            exit.stderr_tail.contains("fake claude crashed on purpose"),
            "{}",
            exit.stderr_tail
        );
        assert!(
            lines_seen >= 2,
            "init and the partial text were delivered before the exit: {lines_seen}"
        );
        assert_eq!(
            sink.lines().len(),
            lines_seen,
            "nothing arrives after the exit"
        );
        assert!(!process.is_running());
        assert!(
            process.send_user("echo more").is_err(),
            "a dead process cannot be written to"
        );
    }

    #[tokio::test]
    async fn stopping_an_idle_process_closes_its_input_and_it_ends_cleanly() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("echo hi").unwrap();
        until("the turn", || sink.count("result:success") == 1).await;
        let started = Instant::now();
        let exit = process.stop(Duration::from_secs(5)).await.unwrap();
        assert_eq!(exit.code, Some(0), "closing stdin is a clean goodbye");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(sink.exit().is_some());
    }

    #[tokio::test]
    async fn stopping_a_busy_process_ends_it_even_though_it_is_mid_turn() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("slow").unwrap();
        until("the turn to be running", || {
            sink.count("stream_event:content_block_delta") >= 1
        })
        .await;
        let started = Instant::now();
        let exit = process.stop(Duration::from_secs(5)).await;
        assert!(exit.is_some());
        assert!(!process.is_running());
        assert!(started.elapsed() < Duration::from_secs(8));
        assert!(
            unsafe { libc::killpg(process.pid(), 0) } != 0,
            "the process group is gone"
        );
    }

    #[tokio::test]
    async fn a_very_long_line_arrives_whole() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("big").unwrap();
        until("the result", || sink.count("result:success") == 1).await;
        let big = sink
            .lines()
            .into_iter()
            .find(|l| l.len() > 250_000)
            .expect("a long line");
        let Event::Assistant(a) = parse_line(&big).unwrap().event else {
            panic!()
        };
        assert!(
            matches!(&a.blocks[0], crate::harness::parser::Block::Text(t) if t.len() == 300_000)
        );
        process.stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn the_process_is_recorded_while_it_runs_and_forgotten_after() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        let records = process::list(&fx.db).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            (
                records[0].pid,
                records[0].kind.as_str(),
                records[0].command.as_str()
            ),
            (process.pid(), "agent", "claude")
        );
        assert_eq!(records[0].owner_id.as_deref(), Some("s1"));
        process.stop(Duration::from_secs(2)).await;
        until("the exit", || sink.exit().is_some()).await;
        assert!(process::list(&fx.db).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_missing_claude_is_a_clear_typed_error() {
        let fx = fixture();
        let env = ResolvedEnv::for_test(&[("PATH", "/nonexistent")]);
        let harness = ClaudeCode::new(Arc::new(env), None);
        let Err(err) = harness
            .start(
                spec(
                    &fx,
                    Launch::New {
                        claude_session_id: "x".into(),
                    },
                ),
                Arc::new(Collect::default()),
            )
            .await
        else {
            panic!("starting without claude must fail")
        };
        assert_eq!(err.code(), "claude_missing");
        assert!(err.to_string().contains("claude.com/claude-code"));
    }

    #[tokio::test]
    async fn a_claude_chosen_in_the_settings_runs_instead_of_the_one_on_the_path() {
        let fx = fixture();
        // The PATH has no claude at all, only the chosen file does.
        let chosen = fx.dir.join("bin/claude");
        let bare_path = "/usr/bin:/bin";
        assert_eq!(
            crate::env::which_in(bare_path, "claude"),
            None,
            "no claude on this PATH"
        );
        let env = ResolvedEnv::for_test(&[
            ("PATH", bare_path),
            ("FAKE_CLAUDE_LOG", fx.log.to_str().unwrap()),
        ]);
        let harness = ClaudeCode::new(Arc::new(env), None);
        let mut spec = spec(
            &fx,
            Launch::New {
                claude_session_id: "x".into(),
            },
        );
        spec.claude_path = Some(chosen);
        spec.extra_args = vec!["--add-dir".into(), "/data".into()];
        let sink = Arc::new(Collect::default());
        let process = harness.start(spec, sink).await.unwrap();
        process.send_user("echo hi").unwrap();
        until("the process started", || {
            log_entries(&fx.log).iter().any(|e| e.get("argv").is_some())
        })
        .await;
        let argv = log_entries(&fx.log)
            .into_iter()
            .find_map(|e| e.get("argv").cloned())
            .unwrap();
        assert!(argv.to_string().contains("--add-dir"), "{argv}");
        process.kill_now();
    }

    #[tokio::test]
    async fn a_chosen_claude_that_is_gone_says_so_and_does_not_fall_back() {
        let fx = fixture();
        let mut spec = spec(
            &fx,
            Launch::New {
                claude_session_id: "x".into(),
            },
        );
        spec.claude_path = Some(PathBuf::from("/nonexistent/claude"));
        let Err(err) = fx.harness.start(spec, Arc::new(Collect::default())).await else {
            panic!("a missing chosen claude must fail")
        };
        assert_eq!(err.code(), "claude_missing");
        assert!(err.to_string().contains("/nonexistent/claude"), "{err}");
    }

    #[tokio::test]
    async fn a_missing_working_directory_is_an_error() {
        let fx = fixture();
        let mut s = spec(
            &fx,
            Launch::New {
                claude_session_id: "x".into(),
            },
        );
        s.cwd = PathBuf::from("/definitely/not/here");
        let Err(err) = fx.harness.start(s, Arc::new(Collect::default())).await else {
            panic!("starting in a missing folder must fail")
        };
        assert_eq!(err.code(), "invalid");
    }

    #[tokio::test]
    async fn thinking_and_tool_calls_come_through_in_one_turn() {
        let fx = fixture();
        let (process, sink) = start(&fx).await;
        process.send_user("think").unwrap();
        until("the result", || sink.count("result:success") == 1).await;
        let messages: Vec<_> = sink
            .lines()
            .iter()
            .flat_map(|l| crate::harness::parser::normalize(&parse_line(l).unwrap().event))
            .collect();
        use crate::harness::parser::MessageKind::*;
        assert_eq!(
            messages.iter().map(|m| m.kind).collect::<Vec<_>>(),
            [Thinking, Text, ToolCall, ToolResult, Text]
        );
        process.stop(Duration::from_secs(2)).await;
    }
}
