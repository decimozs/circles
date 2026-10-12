//! Agent harnesses. A harness starts an AI coding agent as a child process and talks to it.
//! Only Claude Code exists today, behind a trait so another one can be added later.

pub mod claude;
pub mod from_settings;
pub mod parser;
pub mod permissions;
pub mod sessions;
pub mod store;
pub mod tauri;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::AppResult;
use permissions::PermissionDecision;
use store::PermissionMode;

/// How a process begins its conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A conversation that does not exist yet. It gets this id.
    New { claude_session_id: String },
    /// Continue an earlier conversation.
    Resume { claude_session_id: String },
    /// A copy of an earlier conversation that goes its own way.
    Fork {
        from: String,
        claude_session_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct StartSpec {
    /// Our session id, passed back to the sink.
    pub session_id: String,
    pub cwd: PathBuf,
    /// Added on top of the login environment (`CIRCLES_*` and custom variables).
    pub env: Vec<(String, String)>,
    pub launch: Launch,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission_mode: PermissionMode,
    /// A `claude` to run instead of the one on the PATH.
    pub claude_path: Option<PathBuf>,
    /// Added at the end of the command line.
    pub extra_args: Vec<String>,
    /// Added after those, exactly as they are, empty items included. For arguments the app
    /// builds itself, such as `--tools ""`.
    pub fixed_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessExit {
    pub code: Option<i32>,
    pub signaled: bool,
    /// The last lines the process printed on stderr, for an error message.
    pub stderr_tail: String,
}

/// What a running process reports. Calls come from background tasks and must not block.
pub trait HarnessSink: Send + Sync {
    /// One line of output, without the newline.
    fn line(&self, session_id: &str, raw: &str);
    /// The process ended. Every line it printed has been delivered before this.
    fn exited(&self, session_id: &str, exit: ProcessExit);
}

#[async_trait]
pub trait Harness: Send + Sync {
    async fn start(
        &self,
        spec: StartSpec,
        sink: Arc<dyn HarnessSink>,
    ) -> AppResult<Arc<dyn HarnessProcess>>;
}

#[async_trait]
pub trait HarnessProcess: Send + Sync {
    fn pid(&self) -> i32;
    fn is_running(&self) -> bool;
    /// Sends a message. While a turn is running, this steers it. Returns the line that was sent.
    fn send_user(&self, text: &str) -> AppResult<String>;
    /// Stops the current turn. The process stays up. Returns the line that was sent.
    fn interrupt(&self) -> AppResult<String>;
    /// Answers a permission request. Returns the line that was sent.
    fn answer_permission(
        &self,
        request: &permissions::PermissionRequest,
        decision: &PermissionDecision,
    ) -> AppResult<String>;
    /// Refuses a control request this app does not handle, so Claude Code does not wait for it.
    fn reject_control(&self, request_id: &str, reason: &str) -> AppResult<String>;
    /// Changes how Claude Code asks for approval. Returns the line that was sent.
    fn set_permission_mode(&self, mode: PermissionMode) -> AppResult<String>;
    /// Ends the process and everything it started. Returns how it ended.
    async fn stop(&self, grace: Duration) -> Option<ProcessExit>;
    /// Immediate, for shutting the app down.
    fn kill_now(&self);
}
