//! Pseudo terminals. One `Pty` is one shell or script attached to a terminal tab.
//!
//! Output is read on a thread, batched (about every 16 ms) and sent to whatever is attached.
//! The last part of the output is always kept, so a tab that is opened later, or after the
//! frontend was closed, can replay it. The child runs in its own session, so stopping it signals
//! its whole process group: a dev server and everything it started.

pub mod scrollback;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, watch};
use ts_rs::TS;

use crate::db::Db;
use crate::env::ResolvedEnv;
use crate::error::{AppError, AppResult};
use crate::process::{self, ProcessRecord};
use scrollback::Scrollback;

/// How long output is gathered before it is sent on.
const BATCH_INTERVAL: Duration = Duration::from_millis(16);
/// A batch is sent early once it reaches this size.
const MAX_BATCH: usize = 64 * 1024;
/// After the child exits, wait this long for the last output before reporting the exit. A
/// background process can keep the terminal open, so the exit is not tied to the output ending.
const EXIT_DRAIN: Duration = Duration::from_millis(200);
/// Output kept per terminal for replay and restore.
pub const SCROLLBACK_BYTES: usize = 256 * 1024;
/// Grace period between SIGTERM and SIGKILL when stopping a process.
pub const STOP_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ExitInfo {
    /// Exit status. `None` when the process was ended by a signal.
    pub code: Option<i32>,
    pub signaled: bool,
}

/// Where terminal output goes. The app sends it over a Tauri channel, tests collect it.
pub trait PtySink: Send + Sync {
    fn data(&self, bytes: Vec<u8>);
    fn exit(&self, info: &ExitInfo);
    /// The view should clear itself. Sent when a new process takes over a tab.
    fn reset(&self) {}
}

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub cwd: PathBuf,
    /// `None` starts an interactive login shell. `Some` runs the command with the login shell
    /// and ends when it ends.
    pub command: Option<String>,
    /// Added on top of the login environment.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// For the process registry: `pty`, `setup`, `run`.
    pub kind: String,
    pub owner_id: Option<String>,
    /// Output that comes before the process's own, such as a restored previous session.
    pub initial_output: Vec<u8>,
}

struct Shared {
    scrollback: Scrollback,
    sink: Option<Arc<dyn PtySink>>,
    exit: Option<ExitInfo>,
}

pub struct Pty {
    pid: i32,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    shared: Arc<Mutex<Shared>>,
    exited: watch::Receiver<Option<ExitInfo>>,
}

fn shell_of(env: &ResolvedEnv) -> String {
    env.get("SHELL").unwrap_or("/bin/zsh").to_string()
}

fn pty_err(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("terminal: {e}"))
}

impl Pty {
    /// Starts the process. `sink` receives the output from the first byte on.
    pub fn spawn(
        env: &ResolvedEnv,
        db: Option<Arc<Db>>,
        spec: SpawnSpec,
        sink: Option<Arc<dyn PtySink>>,
    ) -> AppResult<Arc<Pty>> {
        if !spec.cwd.is_dir() {
            return Err(AppError::Invalid(format!(
                "The folder {} does not exist, so a terminal cannot start there.",
                spec.cwd.display()
            )));
        }
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: spec.rows.max(1),
                cols: spec.cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_err)?;

        let shell = shell_of(env);
        let mut cmd = CommandBuilder::new(&shell);
        match &spec.command {
            None => cmd.arg("-l"),
            Some(command) => {
                cmd.arg("-lc");
                cmd.arg(command);
            }
        }
        cmd.cwd(&spec.cwd);
        cmd.env_clear();
        for (key, value) in env.vars() {
            cmd.env(key, value);
        }
        for (key, value) in &spec.env {
            cmd.env(key, value);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let mut child = pair.slave.spawn_command(cmd).map_err(pty_err)?;
        // The slave must be closed on our side or the reader never sees the end of output.
        drop(pair.slave);

        let pid = child
            .process_id()
            .and_then(|p| i32::try_from(p).ok())
            .ok_or_else(|| pty_err("the process has no id"))?;
        let mut reader = pair.master.try_clone_reader().map_err(pty_err)?;
        let writer = pair.master.take_writer().map_err(pty_err)?;

        if let Some(db) = &db {
            let command = std::path::Path::new(&shell)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(shell.clone());
            let record = ProcessRecord::new(pid, &spec.kind, spec.owner_id.as_deref(), &command);
            if let Err(e) = process::register(db, &record) {
                tracing::warn!(error = %e, "cannot record the terminal process");
            }
        }

        let (exit_tx, exit_rx) = watch::channel(None);
        let mut scrollback = Scrollback::new(SCROLLBACK_BYTES);
        scrollback.push(&spec.initial_output);
        let shared = Arc::new(Mutex::new(Shared {
            scrollback,
            sink,
            exit: None,
        }));

        // Output: a thread reads, a task batches and delivers.
        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if chunk_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Exit: a thread waits for the child.
        let (done_tx, done_rx) = oneshot::channel::<ExitInfo>();
        std::thread::spawn(move || {
            let info = match child.wait() {
                Ok(status) => ExitInfo {
                    code: i32::try_from(status.exit_code()).ok(),
                    signaled: status.signal().is_some(),
                },
                Err(_) => ExitInfo {
                    code: None,
                    signaled: false,
                },
            };
            let _ = done_tx.send(info);
        });

        let task_shared = shared.clone();
        let task_db = db.clone();
        tokio::spawn(async move {
            let mut batch: Vec<u8> = Vec::new();
            let mut ended = false;
            let mut exit: Option<ExitInfo> = None;
            let mut done_rx = Some(done_rx);
            let mut drain: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;
            let mut tick = tokio::time::interval(BATCH_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            let flush = |batch: &mut Vec<u8>| {
                if batch.is_empty() {
                    return;
                }
                let bytes = std::mem::take(batch);
                let mut shared = task_shared.lock().expect("terminal state poisoned");
                shared.scrollback.push(&bytes);
                if let Some(sink) = &shared.sink {
                    sink.data(bytes);
                }
            };

            loop {
                tokio::select! {
                    chunk = chunk_rx.recv(), if !ended => match chunk {
                        Some(bytes) => {
                            batch.extend_from_slice(&bytes);
                            if batch.len() >= MAX_BATCH {
                                flush(&mut batch);
                            }
                        }
                        None => ended = true,
                    },
                    info = async { done_rx.as_mut().unwrap().await }, if done_rx.is_some() => {
                        done_rx = None;
                        exit = Some(info.unwrap_or(ExitInfo { code: None, signaled: false }));
                        drain = Some(Box::pin(tokio::time::sleep(EXIT_DRAIN)));
                    }
                    _ = tick.tick() => flush(&mut batch),
                    _ = async { drain.as_mut().unwrap().await }, if drain.is_some() => break,
                }
                if ended && exit.is_some() {
                    break;
                }
            }

            // Whatever is still in flight belongs to the output.
            while let Ok(bytes) = chunk_rx.try_recv() {
                batch.extend_from_slice(&bytes);
            }
            flush(&mut batch);

            let info = match exit {
                Some(info) => info,
                None => match done_rx {
                    Some(rx) => rx.await.unwrap_or(ExitInfo {
                        code: None,
                        signaled: false,
                    }),
                    None => ExitInfo {
                        code: None,
                        signaled: false,
                    },
                },
            };
            if let Some(db) = &task_db {
                let _ = process::unregister(db, pid);
            }
            {
                let mut shared = task_shared.lock().expect("terminal state poisoned");
                shared.exit = Some(info.clone());
                if let Some(sink) = &shared.sink {
                    sink.exit(&info);
                }
            }
            let _ = exit_tx.send(Some(info));
        });

        Ok(Arc::new(Pty {
            pid,
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            shared,
            exited: exit_rx,
        }))
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn is_running(&self) -> bool {
        self.exited.borrow().is_none()
    }

    /// Waits until the process has ended and returns how.
    pub async fn wait_exit(&self) -> ExitInfo {
        let mut rx = self.exited.clone();
        loop {
            if let Some(info) = rx.borrow().clone() {
                return info;
            }
            if rx.changed().await.is_err() {
                return rx.borrow().clone().unwrap_or(ExitInfo {
                    code: None,
                    signaled: false,
                });
            }
        }
    }

    pub fn exit_info(&self) -> Option<ExitInfo> {
        self.exited.borrow().clone()
    }

    /// Attaches `sink`, first replaying the kept output so the view starts where the terminal is.
    /// The sink may already have seen an exit, which is replayed too.
    pub fn attach(&self, sink: Arc<dyn PtySink>) {
        let mut shared = self.shared.lock().expect("terminal state poisoned");
        let replay = shared.scrollback.snapshot();
        if !replay.is_empty() {
            sink.data(replay);
        }
        if let Some(info) = &shared.exit {
            sink.exit(info);
        }
        shared.sink = Some(sink);
    }

    /// Stops sending output. The process keeps running and the output is still kept.
    pub fn detach(&self) {
        self.shared.lock().expect("terminal state poisoned").sink = None;
    }

    pub fn scrollback(&self) -> Vec<u8> {
        self.shared
            .lock()
            .expect("terminal state poisoned")
            .scrollback
            .snapshot()
    }

    pub fn write(&self, bytes: &[u8]) -> AppResult<()> {
        if !self.is_running() {
            return Err(AppError::Invalid("the terminal process has ended".into()));
        }
        let mut writer = self.writer.lock().expect("terminal writer poisoned");
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        self.master
            .lock()
            .expect("terminal master poisoned")
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_err)
    }

    fn signal_group(&self, signal: i32) {
        // SAFETY: killpg only sends a signal. The pid is the leader of the session we created.
        unsafe {
            libc::killpg(self.pid, signal);
        }
    }

    /// True while any process of the group is alive.
    pub fn group_alive(&self) -> bool {
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

    /// Ends the process and everything it started: SIGTERM to the group, and SIGKILL if it is
    /// still there after `grace`. Returns how the main process ended.
    pub async fn stop(&self, grace: Duration) -> Option<ExitInfo> {
        if self.is_running() || self.group_alive() {
            self.signal_group(libc::SIGTERM);
            let ended = self.wait_for_exit(grace).await;
            if !ended || self.group_alive() {
                // Give stragglers a moment, then force them.
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
        self.exit_info()
    }

    /// Immediate, blocking version for shutting the app down.
    pub fn kill_now(&self) {
        self.signal_group(libc::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;

    #[derive(Default)]
    struct Collect {
        chunks: Mutex<Vec<Vec<u8>>>,
        exit: Mutex<Option<ExitInfo>>,
    }

    impl Collect {
        fn text(&self) -> String {
            let bytes: Vec<u8> = self
                .chunks
                .lock()
                .unwrap()
                .iter()
                .flatten()
                .copied()
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }
        fn calls(&self) -> usize {
            self.chunks.lock().unwrap().len()
        }
        fn exit(&self) -> Option<ExitInfo> {
            self.exit.lock().unwrap().clone()
        }
    }

    impl PtySink for Collect {
        fn data(&self, bytes: Vec<u8>) {
            self.chunks.lock().unwrap().push(bytes);
        }
        fn exit(&self, info: &ExitInfo) {
            *self.exit.lock().unwrap() = Some(info.clone());
        }
    }

    fn env() -> ResolvedEnv {
        let path = std::env::var("PATH").unwrap_or_default();
        ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("SHELL", "/bin/sh"),
            ("HOME", "/tmp"),
        ])
    }

    fn spec(command: Option<&str>) -> SpawnSpec {
        SpawnSpec {
            cwd: std::env::temp_dir(),
            command: command.map(str::to_string),
            env: vec![],
            cols: 80,
            rows: 24,
            kind: "run".into(),
            owner_id: Some("w1".into()),
            initial_output: Vec::new(),
        }
    }

    async fn run(command: &str) -> (Arc<Pty>, Arc<Collect>) {
        let sink = Arc::new(Collect::default());
        let pty = Pty::spawn(&env(), None, spec(Some(command)), Some(sink.clone())).unwrap();
        (pty, sink)
    }

    async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn runs_a_command_and_reports_success() {
        let (pty, sink) = run("echo hello from the terminal").await;
        wait_until("exit", || sink.exit().is_some()).await;
        assert!(sink.text().contains("hello from the terminal"));
        assert_eq!(
            sink.exit(),
            Some(ExitInfo {
                code: Some(0),
                signaled: false
            })
        );
        assert!(!pty.is_running());
        assert_eq!(pty.exit_info().unwrap().code, Some(0));
    }

    #[tokio::test]
    async fn reports_a_failing_exit_code() {
        let (_pty, sink) = run("echo about to fail; exit 3").await;
        wait_until("exit", || sink.exit().is_some()).await;
        assert_eq!(sink.exit().unwrap().code, Some(3));
        assert!(sink.text().contains("about to fail"));
    }

    #[tokio::test]
    async fn runs_in_the_working_directory_with_the_extra_environment() {
        let dir = std::fs::canonicalize(tempfile::tempdir().unwrap().keep()).unwrap();
        let sink = Arc::new(Collect::default());
        let mut s = spec(Some(
            "pwd; echo port=$CIRCLES_PORT name=$CIRCLES_WORKSPACE_NAME term=$TERM",
        ));
        s.cwd = dir.clone();
        s.env = vec![
            ("CIRCLES_PORT".into(), "55010".into()),
            ("CIRCLES_WORKSPACE_NAME".into(), "algiers".into()),
        ];
        Pty::spawn(&env(), None, s, Some(sink.clone())).unwrap();
        wait_until("exit", || sink.exit().is_some()).await;
        let text = sink.text();
        assert!(text.contains(dir.to_str().unwrap()), "{text}");
        assert!(
            text.contains("port=55010 name=algiers term=xterm-256color"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn the_login_environment_replaces_the_app_environment() {
        std::env::set_var("CIRCLES_LEAK_CHECK", "leaked");
        let (_pty, sink) = run("echo leak=${CIRCLES_LEAK_CHECK:-clean}").await;
        wait_until("exit", || sink.exit().is_some()).await;
        assert!(sink.text().contains("leak=clean"), "{}", sink.text());
    }

    #[tokio::test]
    async fn an_interactive_shell_takes_input() {
        let sink = Arc::new(Collect::default());
        let pty = Pty::spawn(&env(), None, spec(None), Some(sink.clone())).unwrap();
        pty.write(b"echo typed-$((20 + 22))\n").unwrap();
        wait_until("the shell to answer", || sink.text().contains("typed-42")).await;
        pty.write(b"exit 5\n").unwrap();
        wait_until("exit", || sink.exit().is_some()).await;
        assert_eq!(sink.exit().unwrap().code, Some(5));
    }

    #[tokio::test]
    async fn writing_to_an_ended_terminal_fails() {
        let (pty, sink) = run("true").await;
        wait_until("exit", || sink.exit().is_some()).await;
        assert!(pty.write(b"hello\n").is_err());
    }

    #[tokio::test]
    async fn resizing_reaches_the_process() {
        let (pty, sink) = run("sleep 0.6; stty size").await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        pty.resize(120, 40).unwrap();
        wait_until("exit", || sink.exit().is_some()).await;
        assert!(sink.text().contains("40 120"), "{}", sink.text());
    }

    #[tokio::test]
    async fn the_child_leads_its_own_process_group() {
        let (pty, _sink) = run("sleep 30").await;
        let out = std::process::Command::new("ps")
            .args(["-o", "pgid=", "-p", &pty.pid().to_string()])
            .output()
            .unwrap();
        let pgid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        assert_eq!(pgid, pty.pid());
        pty.stop(Duration::from_secs(5)).await;
    }

    #[tokio::test]
    async fn stop_ends_the_whole_process_group() {
        // Two children that outlive the shell's own work.
        let (pty, sink) = run("sleep 60 & sleep 60 & wait").await;
        wait_until("the children to start", || {
            let out = std::process::Command::new("ps")
                .args(["-o", "pid=", "-g", &pty.pid().to_string()])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).lines().count() >= 3
        })
        .await;
        assert!(pty.is_running() && pty.group_alive());

        let started = Instant::now();
        let info = pty.stop(Duration::from_secs(5)).await.unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "SIGTERM is enough, no need to wait"
        );
        assert!(info.signaled || info.code != Some(0));
        assert!(!pty.group_alive(), "no process of the group survives");
        wait_until("exit event", || sink.exit().is_some()).await;
    }

    #[tokio::test]
    async fn stop_escalates_to_sigkill_for_a_process_that_ignores_sigterm() {
        let (pty, _sink) = run("trap '' TERM; while true; do sleep 1; done").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let started = Instant::now();
        pty.stop(Duration::from_millis(500)).await;
        assert!(!pty.group_alive(), "SIGKILL ended the group");
        assert!(!pty.is_running());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn stopping_an_ended_terminal_is_harmless() {
        let (pty, sink) = run("true").await;
        wait_until("exit", || sink.exit().is_some()).await;
        let info = pty.stop(Duration::from_secs(1)).await;
        assert_eq!(info.unwrap().code, Some(0));
    }

    #[tokio::test]
    async fn output_is_batched() {
        let (_pty, sink) =
            run("i=0; while [ $i -lt 3000 ]; do echo line-$i; i=$((i+1)); done").await;
        wait_until("exit", || sink.exit().is_some()).await;
        let text = sink.text();
        assert!(
            text.contains("line-0\r\n") && text.contains("line-2999\r\n"),
            "all output arrives"
        );
        assert!(sink.calls() < 300, "3000 lines in {} sends", sink.calls());
    }

    #[tokio::test]
    async fn a_late_view_gets_the_kept_output_replayed_before_new_output() {
        let (pty, first) = run("echo first-part; sleep 0.5; echo second-part; sleep 0.3").await;
        wait_until("first output", || first.text().contains("first-part")).await;
        pty.detach();

        let second = Arc::new(Collect::default());
        pty.attach(second.clone());
        wait_until("exit", || second.exit().is_some()).await;
        let text = second.text();
        let first_at = text.find("first-part").expect("replayed");
        let second_at = text.find("second-part").expect("live");
        assert!(first_at < second_at, "replay comes first: {text}");
        assert_eq!(
            text.matches("first-part").count(),
            1,
            "nothing is duplicated"
        );
    }

    #[tokio::test]
    async fn output_while_detached_is_kept() {
        let (pty, sink) = run("sleep 0.3; echo while-away").await;
        pty.detach();
        wait_until("exit", || pty.exit_info().is_some()).await;
        assert!(
            !sink.text().contains("while-away"),
            "nothing was sent while detached"
        );
        assert!(String::from_utf8_lossy(&pty.scrollback()).contains("while-away"));

        let late = Arc::new(Collect::default());
        pty.attach(late.clone());
        assert!(late.text().contains("while-away"));
        assert_eq!(
            late.exit().unwrap().code,
            Some(0),
            "an exit that already happened is replayed"
        );
    }

    #[tokio::test]
    async fn exit_is_reported_even_if_a_background_process_keeps_the_terminal_open() {
        let (pty, sink) = run("sleep 30 & echo launched").await;
        let started = Instant::now();
        wait_until("exit", || sink.exit().is_some()).await;
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        assert!(sink.text().contains("launched"));
        // The straggler is cleaned up like any other member of the group.
        pty.stop(Duration::from_secs(1)).await;
        assert!(!pty.group_alive());
    }

    #[tokio::test]
    async fn the_process_is_recorded_while_it_runs_and_forgotten_after() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
        let sink = Arc::new(Collect::default());
        let pty = Pty::spawn(
            &env(),
            Some(db.clone()),
            spec(Some("sleep 0.6")),
            Some(sink.clone()),
        )
        .unwrap();

        let records = process::list(&db).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].pid, pty.pid());
        assert_eq!(records[0].kind, "run");
        assert_eq!(records[0].owner_id.as_deref(), Some("w1"));
        assert_eq!(records[0].command, "sh");

        wait_until("exit", || sink.exit().is_some()).await;
        assert!(process::list(&db).unwrap().is_empty());
    }

    #[tokio::test]
    async fn initial_output_comes_before_the_process_output() {
        let sink = Arc::new(Collect::default());
        let mut s = spec(Some("echo from-the-process"));
        s.initial_output = b"restored history\r\n".to_vec();
        let pty = Pty::spawn(&env(), None, s, None).unwrap();
        pty.wait_exit().await;
        pty.attach(sink.clone());
        let text = sink.text();
        assert!(
            text.find("restored history").unwrap() < text.find("from-the-process").unwrap(),
            "{text}"
        );
    }

    #[tokio::test]
    async fn wait_exit_returns_the_exit_info() {
        let (pty, _sink) = run("exit 7").await;
        assert_eq!(pty.wait_exit().await.code, Some(7));
        // Waiting again after the exit returns at once.
        assert_eq!(pty.wait_exit().await.code, Some(7));
    }

    #[tokio::test]
    async fn a_missing_working_directory_is_an_error() {
        let mut s = spec(Some("true"));
        s.cwd = PathBuf::from("/definitely/not/a/folder");
        assert!(Pty::spawn(&env(), None, s, None).is_err());
    }
}
