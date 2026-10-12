//! Chat sessions: one Claude Code process per session, and everything around it.
//!
//! The manager starts a process when a message is sent, keeps it for later turns, and stores
//! everything it prints. It tracks whether the session is working, waiting for the user, or
//! idle, asks the user about tool permissions, and keeps the workspace's state in step.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use ts_rs::TS;

use super::parser::{
    normalize, parse_line, ControlKind, DeltaKind, Event, MessageKind, NewMessage, RateLimit, Role,
};
use super::permissions::{PermissionDecision, PermissionRequest, Permissions};
use super::store::{
    EventRow, Message, NewSession, PermissionMode, Session, SessionStatus, SessionStore,
};
use super::{Harness, HarnessProcess, HarnessSink, Launch, ProcessExit, StartSpec};
use crate::error::{AppError, AppResult};
use crate::terminal::Context;

/// How long a stopped process gets between SIGTERM and SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Everything the UI is sent about a session, in order.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamItem {
    /// A raw line, for the developer view.
    Raw {
        #[ts(type = "number")]
        seq: i64,
        kind: String,
        line: String,
    },
    /// A chat message, as it was stored.
    Message(Message),
    /// Part of a message that is still being written. Never stored.
    Delta {
        block_index: u32,
        kind: DeltaKind,
        text: String,
        parent_tool_use_id: Option<String>,
    },
    Status {
        status: SessionStatus,
        running: bool,
        error: Option<String>,
    },
    Init {
        model: Option<String>,
        version: Option<String>,
        permission_mode: Option<String>,
        slash_commands: Vec<String>,
        tools: Vec<String>,
    },
    Permission(PermissionRequest),
    PermissionResolved {
        request_id: String,
    },
    Result {
        is_error: bool,
        subtype: String,
        cost_usd: f64,
        #[ts(type = "number")]
        turns: i64,
        #[ts(type = "number")]
        duration_ms: u64,
    },
    RateLimit(RateLimit),
}

/// Where a view receives its items. The app uses a Tauri channel.
pub trait StreamSink: Send + Sync {
    fn item(&self, item: &StreamItem);
}

/// Emitted as `session:changed` whenever a session's status or process changes.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SessionChange {
    pub session_id: String,
    pub workspace_id: String,
    pub status: SessionStatus,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    /// A turn ended.
    Finished,
    /// A tool is waiting for the user's approval.
    NeedsApproval,
    /// The process ended in the middle of a turn, or the turn ended in an error.
    Failed,
}

/// Emitted as `agent:notification`. The settings in phase 13 decide what becomes a native
/// notification or a sound.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct AgentNotification {
    pub workspace_id: String,
    pub session_id: String,
    pub kind: NotificationKind,
    pub detail: Option<String>,
}

pub trait AgentEvents: Send + Sync {
    fn session_changed(&self, change: &SessionChange);
    fn notify(&self, notification: &AgentNotification);
    fn rate_limit(&self, limit: &RateLimit);
}

pub struct NullEvents;

impl AgentEvents for NullEvents {
    fn session_changed(&self, _change: &SessionChange) {}
    fn notify(&self, _notification: &AgentNotification) {}
    fn rate_limit(&self, _limit: &RateLimit) {}
}

/// What a workspace's sessions add up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Blocked,
}

/// Keeps the workspace's state (the sidebar marker) in step with its sessions.
pub trait WorkspaceSync: Send + Sync {
    fn agent_state(&self, workspace_id: &str, state: AgentState);
}

pub struct NoSync;

impl WorkspaceSync for NoSync {
    fn agent_state(&self, _workspace_id: &str, _state: AgentState) {}
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AttachSnapshot {
    pub session: Session,
    /// Permission requests that are still waiting for an answer.
    pub pending: Vec<PermissionRequest>,
}

enum Incoming {
    Line(String),
    Exited(ProcessExit),
}

struct Live {
    process: Arc<dyn HarnessProcess>,
    permissions: Permissions,
    /// Set when the app ended the process on purpose, so its exit is not reported as a crash.
    stopping: AtomicBool,
}

struct ChannelSink(mpsc::UnboundedSender<Incoming>);

impl HarnessSink for ChannelSink {
    fn line(&self, _session_id: &str, raw: &str) {
        let _ = self.0.send(Incoming::Line(raw.to_string()));
    }

    fn exited(&self, _session_id: &str, exit: ProcessExit) {
        let _ = self.0.send(Incoming::Exited(exit));
    }
}

/// Something that wants to know when a turn starts, so it can save the state before it.
#[async_trait]
pub trait TurnHook: Send + Sync {
    /// Called before a message starts a turn. Returns a handle for `after_message`.
    async fn before_turn(
        &self,
        workspace_id: &str,
        session_id: &str,
        claude_session_id: Option<&str>,
        cwd: &std::path::Path,
    ) -> Option<String>;

    /// Called once the message that started the turn is stored.
    fn after_message(&self, handle: &str, message_seq: i64);
}

/// What a chat needs added to its command line and environment when its process starts, beyond
/// what the settings say. Composers use it to hand their tools to Claude Code.
#[derive(Debug, Clone, Default)]
pub struct LaunchExtras {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub trait LaunchHook: Send + Sync {
    fn extras(&self, session: &Session) -> Option<LaunchExtras>;
}

/// What the settings say about starting Claude Code.
#[derive(Debug, Clone, Default)]
pub struct AgentSettings {
    pub claude_path: Option<std::path::PathBuf>,
    pub extra_args: Vec<String>,
}

pub struct SessionManager {
    store: SessionStore,
    agent: Mutex<AgentSettings>,
    hook: Mutex<Option<Arc<dyn TurnHook>>>,
    launch_hook: Mutex<Option<Arc<dyn LaunchHook>>>,
    harness: Arc<dyn Harness>,
    events: Arc<dyn AgentEvents>,
    sync: Arc<dyn WorkspaceSync>,
    live: Mutex<HashMap<String, Arc<Live>>>,
    views: Mutex<HashMap<String, Arc<dyn StreamSink>>>,
    rate_limit: Mutex<Option<RateLimit>>,
    /// Starting a process is rare, so two starts never overlap.
    start_guard: AsyncMutex<()>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

fn system_note(text: impl Into<String>, error: bool) -> NewMessage {
    NewMessage {
        role: Role::System,
        kind: MessageKind::System,
        content: serde_json::json!({ "text": text.into(), "error": error }),
    }
}

impl SessionManager {
    pub fn new(
        store: SessionStore,
        harness: Arc<dyn Harness>,
        events: Arc<dyn AgentEvents>,
        sync: Arc<dyn WorkspaceSync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            hook: Mutex::new(None),
            launch_hook: Mutex::new(None),
            agent: Mutex::new(AgentSettings::default()),
            harness,
            events,
            sync,
            live: Mutex::new(HashMap::new()),
            views: Mutex::new(HashMap::new()),
            rate_limit: Mutex::new(None),
            start_guard: AsyncMutex::new(()),
        })
    }

    pub fn store(&self) -> &SessionStore {
        &self.store
    }

    /// Sets how Claude Code is started from now on, from the settings.
    pub fn set_agent_settings(&self, settings: AgentSettings) {
        *self.agent.lock().expect("sessions poisoned") = settings;
    }

    /// Sets what is told when a turn starts. Done once, when the app starts.
    pub fn set_turn_hook(&self, hook: Arc<dyn TurnHook>) {
        *self.hook.lock().expect("sessions poisoned") = Some(hook);
    }

    /// Sets what is added when a chat's process starts. Done once, when the app starts.
    pub fn set_launch_hook(&self, hook: Arc<dyn LaunchHook>) {
        *self.launch_hook.lock().expect("sessions poisoned") = Some(hook);
    }

    fn launch_extras(&self, session: &Session) -> LaunchExtras {
        self.launch_hook
            .lock()
            .expect("sessions poisoned")
            .clone()
            .and_then(|hook| hook.extras(session))
            .unwrap_or_default()
    }

    fn turn_hook(&self) -> Option<Arc<dyn TurnHook>> {
        self.hook.lock().expect("sessions poisoned").clone()
    }

    fn live(&self, session_id: &str) -> Option<Arc<Live>> {
        self.live
            .lock()
            .expect("sessions poisoned")
            .get(session_id)
            .cloned()
    }

    fn is_running(&self, session_id: &str) -> bool {
        self.live(session_id)
            .is_some_and(|l| l.process.is_running())
    }

    fn with_running(&self, mut session: Session) -> Session {
        session.running = self.is_running(&session.id);
        session
    }

    // Reading

    pub fn create(&self, workspace_id: &str, new: NewSession) -> AppResult<Session> {
        self.store.create(workspace_id, new)
    }

    pub fn get(&self, session_id: &str) -> AppResult<Session> {
        self.store.get(session_id).map(|s| self.with_running(s))
    }

    pub fn list(&self, workspace_id: &str) -> AppResult<Vec<Session>> {
        self.list_with(workspace_id, false)
    }

    /// Chats of a workspace, closed ones too when `include_closed`, for the history menu.
    pub fn list_with(&self, workspace_id: &str, include_closed: bool) -> AppResult<Vec<Session>> {
        Ok(self
            .store
            .list(workspace_id, include_closed)?
            .into_iter()
            .map(|s| self.with_running(s))
            .collect())
    }

    pub fn messages(&self, session_id: &str, after_seq: Option<i64>) -> AppResult<Vec<Message>> {
        self.store.get(session_id)?;
        self.store.messages(session_id, after_seq)
    }

    pub fn events(
        &self,
        session_id: &str,
        after_seq: Option<i64>,
        limit: u32,
    ) -> AppResult<Vec<EventRow>> {
        self.store.get(session_id)?;
        self.store.events(session_id, after_seq, limit.min(5000))
    }

    pub fn rate_limit(&self) -> Option<RateLimit> {
        self.rate_limit.lock().expect("sessions poisoned").clone()
    }

    pub fn rename(&self, session_id: &str, title: &str) -> AppResult<Session> {
        self.store
            .rename(session_id, title)
            .map(|s| self.with_running(s))
    }

    /// Sessions that are working or waiting right now, for the quit confirmation.
    pub fn active_count(&self) -> usize {
        let ids: Vec<String> = self
            .live
            .lock()
            .expect("sessions poisoned")
            .keys()
            .cloned()
            .collect();
        ids.iter()
            .filter(|id| self.store.get(id).is_ok_and(|s| s.status.is_active()))
            .count()
    }

    // Views

    pub fn attach(&self, session_id: &str, view: Arc<dyn StreamSink>) -> AppResult<AttachSnapshot> {
        let session = self.get(session_id)?;
        self.views
            .lock()
            .expect("sessions poisoned")
            .insert(session_id.to_string(), view);
        let pending = self
            .live(session_id)
            .map(|l| l.permissions.pending())
            .unwrap_or_default();
        Ok(AttachSnapshot { session, pending })
    }

    pub fn detach(&self, session_id: &str) {
        self.views
            .lock()
            .expect("sessions poisoned")
            .remove(session_id);
    }

    fn emit(&self, session_id: &str, item: StreamItem) {
        let view = self
            .views
            .lock()
            .expect("sessions poisoned")
            .get(session_id)
            .cloned();
        if let Some(view) = view {
            view.item(&item);
        }
    }

    // Status

    fn workspace_of(&self, session_id: &str) -> Option<String> {
        self.store.workspace_of(session_id).ok()
    }

    fn sync_workspace(&self, workspace_id: &str) {
        let statuses = self.store.active_statuses(workspace_id).unwrap_or_default();
        let state = if statuses.contains(&SessionStatus::Blocked) {
            AgentState::Blocked
        } else if statuses.contains(&SessionStatus::Working) {
            AgentState::Working
        } else {
            AgentState::Idle
        };
        self.sync.agent_state(workspace_id, state);
    }

    fn set_status(&self, session_id: &str, status: SessionStatus, error: Option<&str>) {
        if let Err(e) = self.store.set_status(session_id, status, error) {
            tracing::warn!(error = %e, "cannot store the session status");
            return;
        }
        let running = self.is_running(session_id);
        self.emit(
            session_id,
            StreamItem::Status {
                status,
                running,
                error: error.map(str::to_string),
            },
        );
        if let Some(workspace_id) = self.workspace_of(session_id) {
            self.events.session_changed(&SessionChange {
                session_id: session_id.to_string(),
                workspace_id: workspace_id.clone(),
                status,
                running,
            });
            self.sync_workspace(&workspace_id);
        }
    }

    fn notify(&self, session_id: &str, kind: NotificationKind, detail: Option<String>) {
        if let Some(workspace_id) = self.workspace_of(session_id) {
            self.events.notify(&AgentNotification {
                workspace_id,
                session_id: session_id.to_string(),
                kind,
                detail,
            });
        }
    }

    fn store_messages(&self, session_id: &str, event_id: Option<&str>, messages: &[NewMessage]) {
        match self.store.append_messages(session_id, event_id, messages) {
            Ok(stored) => {
                for message in stored {
                    self.emit(session_id, StreamItem::Message(message));
                }
            }
            Err(e) => tracing::warn!(error = %e, "cannot store chat messages"),
        }
    }

    fn record_sent(&self, session_id: &str, kind: &str, line: &str) {
        if let Err(e) = self.store.append_event(session_id, kind, line.trim_end()) {
            tracing::warn!(error = %e, "cannot store a sent line");
        }
    }

    // Starting

    /// The running process of a session, started if there is none. `env` is the login
    /// environment and `ctx` says where the workspace is.
    async fn ensure_process(
        self: &Arc<Self>,
        ctx: &Context,
        session: &Session,
    ) -> AppResult<Arc<Live>> {
        let _guard = self.start_guard.lock().await;
        if let Some(live) = self.live(&session.id).filter(|l| l.process.is_running()) {
            return Ok(live);
        }

        let fork_of = self.store.fork_of(&session.id)?;
        let has_history = self.store.event_count(&session.id)? > 0;
        let claude_id = match (&fork_of, &session.claude_session_id) {
            (None, Some(id)) => id.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };
        let launch = match fork_of {
            Some(from) => Launch::Fork {
                from,
                claude_session_id: claude_id.clone(),
            },
            // A chat whose agent session was started over, after a revert, has history of its own
            // but nothing to resume.
            None if has_history && session.claude_session_id.is_some() => Launch::Resume {
                claude_session_id: claude_id.clone(),
            },
            None => Launch::New {
                claude_session_id: claude_id.clone(),
            },
        };
        // Stored first, so even a process that dies before saying anything can be resumed.
        self.store
            .set_claude_session(&session.id, &claude_id, None)?;

        let mut env = ctx.env.clone();
        env.push(("CIRCLES_SESSION_ID".into(), session.id.clone()));
        let mut extras = self.launch_extras(session);
        env.extend(extras.env.clone());
        // The project's own skills and instruction files come before anyone's habits. A Composer's
        // own process has its prompt already and no project folder of its own.
        if !extras.args.iter().any(|a| a == "--append-system-prompt") {
            if let Some(note) = crate::chat::project_skills::note(&ctx.cwd) {
                extras.args.extend(["--append-system-prompt".into(), note]);
            }
        }

        let agent = self.agent.lock().expect("sessions poisoned").clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let process = self
            .harness
            .start(
                StartSpec {
                    session_id: session.id.clone(),
                    cwd: ctx.cwd.clone(),
                    env,
                    launch,
                    model: session.model.clone(),
                    effort: session.effort.clone(),
                    permission_mode: session.permission_mode,
                    claude_path: agent.claude_path,
                    extra_args: agent.extra_args,
                    fixed_args: extras.args,
                },
                Arc::new(ChannelSink(tx)),
            )
            .await?;

        let live = Arc::new(Live {
            process,
            permissions: Permissions::default(),
            stopping: AtomicBool::new(false),
        });
        self.live
            .lock()
            .expect("sessions poisoned")
            .insert(session.id.clone(), live.clone());

        // One task handles the process's output in order, off the reader's path.
        let this = self.clone();
        let session_id = session.id.clone();
        let task_live = live.clone();
        tokio::spawn(async move {
            let mut rx = rx;
            while let Some(incoming) = rx.recv().await {
                match incoming {
                    Incoming::Line(raw) => this.handle_line(&session_id, &task_live, &raw),
                    Incoming::Exited(exit) => {
                        this.handle_exit(&session_id, &task_live, exit);
                        break;
                    }
                }
            }
        });
        Ok(live)
    }

    // Sending

    /// Sends a message. If a turn is running this steers it. Returns the stored message.
    pub async fn send(
        self: &Arc<Self>,
        ctx: &Context,
        session_id: &str,
        text: &str,
    ) -> AppResult<Message> {
        self.send_with(ctx, session_id, text, &[]).await
    }

    /// Like `send`, with files the user attached. Claude is told where they are.
    pub async fn send_with(
        self: &Arc<Self>,
        ctx: &Context,
        session_id: &str,
        text: &str,
        attachments: &[crate::chat::Attachment],
    ) -> AppResult<Message> {
        let text = text.trim();
        if text.is_empty() && attachments.is_empty() {
            return Err(invalid("Write a message first."));
        }
        let session = self.store.get(session_id)?;
        if session.status == SessionStatus::Closed {
            return Err(invalid("This chat was closed."));
        }
        let live = self.ensure_process(ctx, &session).await?;

        let steering = session.status.is_active();
        // The state before a turn is saved so it can be gone back to. Steering is part of a turn
        // that already started.
        let hook = self.turn_hook();
        let checkpoint = match (&hook, steering) {
            (Some(hook), false) => {
                hook.before_turn(
                    &ctx.workspace_id,
                    session_id,
                    session.claude_session_id.as_deref(),
                    &ctx.cwd,
                )
                .await
            }
            _ => None,
        };
        let mut prompt = text.to_string();
        if !attachments.is_empty() {
            prompt.push_str("\n\nAttached files:\n");
            for attachment in attachments {
                prompt.push_str(&format!("- {}\n", attachment.path));
            }
        }
        // A chat whose agent session was started over, after a revert, is told what came before.
        if !steering {
            if let Some(context) = self.store.take_replay_context(session_id)? {
                prompt = format!("{context}\n\n{prompt}");
            }
        }
        let line = live.process.send_user(prompt.trim())?;
        self.record_sent(session_id, "sent:user", &line);
        let mut content = serde_json::json!({ "text": text, "steered": steering });
        if !attachments.is_empty() {
            content["attachments"] = serde_json::to_value(attachments).unwrap_or_default();
        }
        let stored = self.store.append_messages(
            session_id,
            None,
            &[NewMessage {
                role: Role::User,
                kind: MessageKind::Text,
                content,
            }],
        )?;
        let message = stored.into_iter().next().expect("one message was stored");
        if let (Some(hook), Some(handle)) = (&hook, &checkpoint) {
            hook.after_message(handle, message.seq);
        }
        self.emit(session_id, StreamItem::Message(message.clone()));
        if !steering {
            self.set_status(session_id, SessionStatus::Working, None);
        }
        Ok(message)
    }

    /// Starts the process without a message, so a resumed chat is ready before the user types.
    pub async fn resume(self: &Arc<Self>, ctx: &Context, session_id: &str) -> AppResult<Session> {
        let session = self.store.get(session_id)?;
        if session.status == SessionStatus::Closed {
            return Err(invalid("This chat was closed."));
        }
        self.ensure_process(ctx, &session).await?;
        if session.status == SessionStatus::Interrupted {
            self.set_status(session_id, SessionStatus::Idle, None);
        }
        self.get(session_id)
    }

    /// Stops the current turn. The chat stays open and the process keeps running.
    pub fn interrupt(&self, session_id: &str) -> AppResult<()> {
        let session = self.store.get(session_id)?;
        let Some(live) = self
            .live(session_id)
            .filter(|l| session.status.is_active() && l.process.is_running())
        else {
            return Ok(());
        };
        let line = live.process.interrupt()?;
        self.record_sent(session_id, "sent:control_request", &line);
        // The permission request that was open is cancelled along with the turn.
        for request_id in live.permissions.clear_pending() {
            self.emit(session_id, StreamItem::PermissionResolved { request_id });
        }
        Ok(())
    }

    /// Ends the process. The conversation is kept and the next message resumes it.
    pub async fn stop(&self, session_id: &str) -> AppResult<()> {
        self.store.get(session_id)?;
        let Some(live) = self.live(session_id) else {
            return Ok(());
        };
        live.stopping.store(true, Ordering::SeqCst);
        for request_id in live.permissions.clear_pending() {
            self.emit(session_id, StreamItem::PermissionResolved { request_id });
        }
        live.process.stop(STOP_GRACE).await;
        // Wait for the exit to be processed, so the status is settled when this returns.
        for _ in 0..200 {
            if self
                .live(session_id)
                .is_none_or(|l| !Arc::ptr_eq(&l, &live))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    /// Closes the chat tab: stops the process and hides the session.
    pub async fn close(&self, session_id: &str) -> AppResult<()> {
        self.stop(session_id).await?;
        self.set_status(session_id, SessionStatus::Closed, None);
        Ok(())
    }

    /// Brings a closed chat back as an open tab.
    pub fn reopen(&self, session_id: &str) -> AppResult<Session> {
        let session = self.store.get(session_id)?;
        if session.status == SessionStatus::Closed {
            self.set_status(session_id, SessionStatus::Idle, None);
        }
        self.get(session_id)
    }

    pub fn reorder(&self, workspace_id: &str, ids: &[String]) -> AppResult<()> {
        self.store.reorder(workspace_id, ids)
    }

    /// Stops every session of a workspace, for archiving and deleting it.
    pub async fn stop_workspace(&self, workspace_id: &str) {
        let sessions = self.store.list(workspace_id, true).unwrap_or_default();
        for session in sessions {
            let _ = self.stop(&session.id).await;
        }
    }

    /// Answers a permission request.
    pub fn reply_permission(
        &self,
        session_id: &str,
        request_id: &str,
        decision: PermissionDecision,
    ) -> AppResult<()> {
        let live = self
            .live(session_id)
            .ok_or_else(|| invalid("The agent is not running, so there is nothing to answer."))?;
        let request = live
            .permissions
            .resolve(request_id, &decision)
            .ok_or_else(|| AppError::NotFound(format!("permission request {request_id}")))?;
        let line = live.process.answer_permission(&request, &decision)?;
        self.record_sent(session_id, "sent:control_response", &line);
        self.emit(
            session_id,
            StreamItem::PermissionResolved {
                request_id: request_id.to_string(),
            },
        );
        if live.permissions.pending_count() == 0
            && self.store.get(session_id)?.status == SessionStatus::Blocked
        {
            self.set_status(session_id, SessionStatus::Working, None);
        }
        Ok(())
    }

    /// Changes model, effort or permission mode. The mode applies to the running process at
    /// once. A new model or effort restarts the process on the next message, which is only
    /// possible between turns.
    pub async fn update_options(
        &self,
        session_id: &str,
        model: Option<&str>,
        effort: Option<&str>,
        mode: Option<PermissionMode>,
    ) -> AppResult<Session> {
        let before = self.store.get(session_id)?;
        let restart = model.is_some_and(|m| Some(m) != before.model.as_deref())
            || effort.is_some_and(|e| Some(e) != before.effort.as_deref());
        if restart && before.status.is_active() {
            return Err(invalid(
                "Wait for the current turn to finish before changing the model or effort.",
            ));
        }
        let session = self.store.update_options(session_id, model, effort, mode)?;

        if let (Some(mode), Some(live)) = (mode, self.live(session_id)) {
            if mode != before.permission_mode && live.process.is_running() {
                let line = live.process.set_permission_mode(mode)?;
                self.record_sent(session_id, "sent:control_request", &line);
            }
        }
        if restart && self.is_running(session_id) {
            self.stop(session_id).await?;
        }
        Ok(self.with_running(session))
    }

    // Output

    fn handle_line(&self, session_id: &str, live: &Arc<Live>, raw: &str) {
        let parsed = match parse_line(raw) {
            Ok(parsed) => parsed,
            Err(e) => {
                tracing::debug!(error = %e, "unparsable line from the agent");
                if let Ok((_, seq)) = self.store.append_event(session_id, "unparsed", raw) {
                    self.emit(
                        session_id,
                        StreamItem::Raw {
                            seq,
                            kind: "unparsed".into(),
                            line: raw.to_string(),
                        },
                    );
                }
                return;
            }
        };
        let (event_id, seq) = match self.store.append_event(session_id, &parsed.kind, raw) {
            Ok(stored) => stored,
            Err(e) => {
                tracing::warn!(error = %e, "cannot store an agent event");
                return;
            }
        };
        self.emit(
            session_id,
            StreamItem::Raw {
                seq,
                kind: parsed.kind.clone(),
                line: raw.to_string(),
            },
        );

        match &parsed.event {
            Event::Init(init) => {
                if let Err(e) = self.store.set_claude_session(
                    session_id,
                    &init.session_id,
                    init.model.as_deref(),
                ) {
                    tracing::warn!(error = %e, "cannot store the Claude session id");
                }
                self.emit(
                    session_id,
                    StreamItem::Init {
                        model: init.model.clone(),
                        version: init.version.clone(),
                        permission_mode: init.permission_mode.clone(),
                        slash_commands: init.slash_commands.clone(),
                        tools: init.tools.clone(),
                    },
                );
            }
            Event::Delta(delta) => self.emit(
                session_id,
                StreamItem::Delta {
                    block_index: delta.block_index,
                    kind: delta.kind,
                    text: delta.text.clone(),
                    parent_tool_use_id: delta.parent_tool_use_id.clone(),
                },
            ),
            Event::Assistant(_) | Event::User(_) => {
                self.store_messages(session_id, Some(&event_id), &normalize(&parsed.event));
            }
            Event::Result(result) => {
                self.store_messages(session_id, Some(&event_id), &normalize(&parsed.event));
                if let Err(e) = self.store.record_turn(session_id, result.total_cost_usd) {
                    tracing::warn!(error = %e, "cannot store the turn");
                }
                let turns = self.store.get(session_id).map(|s| s.turns).unwrap_or(0);
                self.emit(
                    session_id,
                    StreamItem::Result {
                        is_error: result.is_error,
                        subtype: result.subtype.clone(),
                        cost_usd: result.total_cost_usd,
                        turns,
                        duration_ms: result.duration_ms,
                    },
                );
                let error = result.is_error.then(|| {
                    result
                        .result
                        .clone()
                        .unwrap_or_else(|| result.subtype.clone())
                });
                self.set_status(session_id, SessionStatus::Idle, error.as_deref());
                if result.is_error {
                    self.notify(session_id, NotificationKind::Failed, error);
                } else {
                    self.notify(session_id, NotificationKind::Finished, None);
                }
            }
            Event::ControlRequest(control) => match &control.kind {
                ControlKind::CanUseTool { .. } => self.handle_permission(session_id, live, control),
                ControlKind::Other(subtype) => {
                    let reason = format!("Circles does not handle the \"{subtype}\" request.");
                    if let Ok(line) = live.process.reject_control(&control.request_id, &reason) {
                        self.record_sent(session_id, "sent:control_response", &line);
                    }
                }
            },
            Event::RateLimit(limit) => {
                *self.rate_limit.lock().expect("sessions poisoned") = Some(limit.clone());
                self.events.rate_limit(limit);
                self.emit(session_id, StreamItem::RateLimit(limit.clone()));
            }
            Event::Status(_)
            | Event::StreamMarker(_)
            | Event::ControlResponse { .. }
            | Event::System(_)
            | Event::Unknown(_) => {}
        }
    }

    fn handle_permission(
        &self,
        session_id: &str,
        live: &Arc<Live>,
        control: &super::parser::ControlRequest,
    ) {
        let Some(request) = PermissionRequest::from_control(session_id, control) else {
            return;
        };
        // Something the user already allowed for this session goes through without asking.
        if live
            .permissions
            .is_allowed(&request.tool_name, &request.input)
        {
            match live
                .process
                .answer_permission(&request, &PermissionDecision::Allow)
            {
                Ok(line) => self.record_sent(session_id, "sent:control_response", &line),
                Err(e) => tracing::warn!(error = %e, "cannot auto-allow a tool use"),
            }
            return;
        }
        let tool = request
            .display_name
            .clone()
            .unwrap_or_else(|| request.tool_name.clone());
        live.permissions.add_pending(request.clone());
        self.emit(session_id, StreamItem::Permission(request));
        self.set_status(session_id, SessionStatus::Blocked, None);
        self.notify(session_id, NotificationKind::NeedsApproval, Some(tool));
    }

    fn handle_exit(&self, session_id: &str, live: &Arc<Live>, exit: ProcessExit) {
        {
            let mut map = self.live.lock().expect("sessions poisoned");
            if map.get(session_id).is_some_and(|l| Arc::ptr_eq(l, live)) {
                map.remove(session_id);
            }
        }
        for request_id in live.permissions.clear_pending() {
            self.emit(session_id, StreamItem::PermissionResolved { request_id });
        }
        let status = self
            .store
            .get(session_id)
            .map(|s| s.status)
            .unwrap_or(SessionStatus::Idle);
        let on_purpose = live.stopping.load(Ordering::SeqCst);

        if status.is_active() && on_purpose {
            self.store_messages(session_id, None, &[system_note("Stopped.", false)]);
            self.set_status(session_id, SessionStatus::Idle, None);
        } else if status.is_active() {
            let how = match exit.code {
                Some(code) => format!("The agent stopped unexpectedly (exit code {code})."),
                None => "The agent stopped unexpectedly.".to_string(),
            };
            let detail = exit.stderr_tail.trim();
            let text = if detail.is_empty() {
                how.clone()
            } else {
                format!("{how} {detail}")
            };
            self.store_messages(session_id, None, &[system_note(text.clone(), true)]);
            self.set_status(session_id, SessionStatus::Interrupted, Some(&text));
            self.notify(session_id, NotificationKind::Failed, Some(how));
        } else {
            // An idle process ending is not news, but views should know it is no longer running.
            self.set_status(session_id, status, None);
        }
    }

    /// Ends every process at once. Used when the app quits.
    pub fn kill_all(&self) {
        let all: Vec<Arc<Live>> = self
            .live
            .lock()
            .expect("sessions poisoned")
            .values()
            .cloned()
            .collect();
        for live in all {
            live.stopping.store(true, Ordering::SeqCst);
            live.process.kill_now();
        }
    }

    /// At startup: chats that were mid turn when the app stopped are marked interrupted.
    pub fn recover_after_restart(&self) -> AppResult<usize> {
        let stale = self.store.interrupt_stale()?;
        for (session_id, workspace_id) in &stale {
            self.store_messages(
                session_id,
                None,
                &[system_note(
                    "Circles stopped while this chat was working. Send a message to continue.",
                    true,
                )],
            );
            self.events.session_changed(&SessionChange {
                session_id: session_id.clone(),
                workspace_id: workspace_id.clone(),
                status: SessionStatus::Interrupted,
                running: false,
            });
        }
        Ok(stale.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::env::ResolvedEnv;
    use crate::harness::claude::ClaudeCode;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    const FAKE: &str = include_str!("fixtures/fake_claude.py");

    #[derive(Default)]
    struct View(Mutex<Vec<StreamItem>>);

    impl View {
        fn items(&self) -> Vec<StreamItem> {
            self.0.lock().unwrap().clone()
        }
        fn statuses(&self) -> Vec<SessionStatus> {
            self.items()
                .iter()
                .filter_map(|i| {
                    if let StreamItem::Status { status, .. } = i {
                        Some(*status)
                    } else {
                        None
                    }
                })
                .collect()
        }
        fn messages(&self) -> Vec<Message> {
            self.items()
                .into_iter()
                .filter_map(|i| {
                    if let StreamItem::Message(m) = i {
                        Some(m)
                    } else {
                        None
                    }
                })
                .collect()
        }
        fn text_of(&self, kind: MessageKind) -> Vec<String> {
            self.messages()
                .iter()
                .filter(|m| m.kind == kind)
                .map(|m| m.content["text"].as_str().unwrap_or_default().to_string())
                .collect()
        }
        fn permissions(&self) -> Vec<PermissionRequest> {
            self.items()
                .into_iter()
                .filter_map(|i| {
                    if let StreamItem::Permission(p) = i {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect()
        }
    }

    impl StreamSink for View {
        fn item(&self, item: &StreamItem) {
            self.0.lock().unwrap().push(item.clone());
        }
    }

    #[derive(Default)]
    struct Events {
        changes: Mutex<Vec<SessionChange>>,
        notes: Mutex<Vec<AgentNotification>>,
        limits: Mutex<Vec<RateLimit>>,
    }

    impl AgentEvents for Events {
        fn session_changed(&self, change: &SessionChange) {
            self.changes.lock().unwrap().push(change.clone());
        }
        fn notify(&self, n: &AgentNotification) {
            self.notes.lock().unwrap().push(n.clone());
        }
        fn rate_limit(&self, l: &RateLimit) {
            self.limits.lock().unwrap().push(l.clone());
        }
    }

    #[derive(Default)]
    struct Sync(Mutex<Vec<(String, AgentState)>>);

    impl WorkspaceSync for Sync {
        fn agent_state(&self, workspace_id: &str, state: AgentState) {
            self.0
                .lock()
                .unwrap()
                .push((workspace_id.to_string(), state));
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        log: PathBuf,
        manager: Arc<SessionManager>,
        events: Arc<Events>,
        sync: Arc<Sync>,
        ctx: Context,
        ctx2: Context,
    }

    fn fixture_with(claude_present: bool) -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(tmp.path()).unwrap();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        if claude_present {
            let claude = bin.join("claude");
            std::fs::write(&claude, FAKE).unwrap();
            std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let log = dir.join("fake.log");
        // Only the fake `claude`, python (the fake needs it) and the system folders. The real
        // `claude` must never be reachable from a test, or a test would spend real API credit.
        let python = crate::env::which_in(&std::env::var("PATH").unwrap_or_default(), "python3")
            .expect("python3 is needed for the fake claude");
        let path = format!(
            "{}:{}:/usr/bin:/bin",
            bin.display(),
            python.parent().unwrap().display()
        );
        let found = crate::env::which_in(&path, "claude");
        if claude_present {
            assert_eq!(found, Some(bin.join("claude")));
        } else {
            assert_eq!(
                found, None,
                "the real claude is on the test PATH: {found:?}"
            );
        }
        let env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("HOME", "/tmp"),
            ("FAKE_CLAUDE_LOG", log.to_str().unwrap()),
        ]);

        let db = Arc::new(Db::open(&dir.join("t.db")).unwrap());
        for name in ["w1", "w2"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1);
                 INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                   VALUES ('w1','p','Oslo','b','main','/w1','ready',1,1),
                          ('w2','p','Lima','b2','main','/w2','ready',1,1);",
            )
        })
        .unwrap();
        let events = Arc::new(Events::default());
        let sync = Arc::new(Sync::default());
        let manager = SessionManager::new(
            SessionStore::new(db.clone()),
            Arc::new(ClaudeCode::new(Arc::new(env), Some(db))),
            events.clone(),
            sync.clone(),
        );
        let ctx_for = |ws: &str| Context {
            workspace_id: ws.into(),
            project_id: "p".into(),
            cwd: dir.join(ws),
            env: vec![("CIRCLES_PORT".into(), "55010".into())],
        };
        let (ctx, ctx2) = (ctx_for("w1"), ctx_for("w2"));
        Fx {
            _tmp: tmp,
            dir,
            log,
            manager,
            events,
            sync,
            ctx,
            ctx2,
        }
    }

    fn fixture() -> Fx {
        fixture_with(true)
    }

    async fn until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn log(fx: &Fx) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&fx.log)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn starts(fx: &Fx) -> Vec<Vec<String>> {
        log(fx)
            .iter()
            .filter_map(|e| e.get("argv"))
            .map(|a| {
                a.as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().to_string())
                    .collect()
            })
            .collect()
    }

    fn new_session(fx: &Fx) -> (Session, Arc<View>) {
        let session = fx.manager.create("w1", NewSession::default()).unwrap();
        let view = Arc::new(View::default());
        fx.manager.attach(&session.id, view.clone()).unwrap();
        (session, view)
    }

    fn status(fx: &Fx, id: &str) -> SessionStatus {
        fx.manager.store().get(id).unwrap().status
    }

    async fn settle(fx: &Fx, id: &str, want: SessionStatus) {
        until(&format!("{id} to be {want:?}"), || status(fx, id) == want).await;
    }

    #[tokio::test]
    async fn a_new_session_has_no_process_until_the_first_message() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        assert!(!fx.manager.get(&session.id).unwrap().running);
        assert!(starts(&fx).is_empty());
        assert_eq!(fx.manager.list("w1").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_message_starts_a_turn_that_ends_idle_with_everything_stored() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        let sent = fx
            .manager
            .send(&fx.ctx, &session.id, "  echo hello  ")
            .await
            .unwrap();
        assert_eq!((sent.role, sent.kind), (Role::User, MessageKind::Text));
        assert_eq!(sent.content["text"], "echo hello");
        assert_eq!(sent.content["steered"], false);
        assert_eq!(status(&fx, &session.id), SessionStatus::Working);

        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert_eq!(
            view.text_of(MessageKind::Text),
            ["echo hello", "echo: hello"]
        );
        assert_eq!(
            view.statuses(),
            [SessionStatus::Working, SessionStatus::Idle]
        );

        let after = fx.manager.get(&session.id).unwrap();
        assert_eq!(after.turns, 1);
        assert!((after.total_cost_usd - 0.01).abs() < 1e-9);
        assert!(after.running, "the process stays up between turns");
        assert!(after.claude_session_id.is_some());
        assert_eq!(after.model.as_deref(), Some("fake-model"));

        let result = view.items().into_iter().find_map(|i| {
            if let StreamItem::Result {
                turns, is_error, ..
            } = i
            {
                Some((turns, is_error))
            } else {
                None
            }
        });
        assert_eq!(result, Some((1, false)));
        assert!(view.items().iter().any(|i| matches!(i, StreamItem::Init { slash_commands, .. } if slash_commands == &["compact"])));

        let stored = fx.manager.messages(&session.id, None).unwrap();
        assert_eq!(stored.iter().map(|m| m.seq).collect::<Vec<_>>(), [1, 2]);
        let events = fx.manager.events(&session.id, None, 100).unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert!(
            kinds.contains(&"sent:user")
                && kinds.contains(&"system:init")
                && kinds.contains(&"result:success"),
            "{kinds:?}"
        );
        assert!(
            events.iter().all(|e| !e.raw.is_empty()),
            "raw lines are kept"
        );
    }

    #[tokio::test]
    async fn the_workspace_follows_the_sessions_and_a_notification_marks_the_end() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;

        let states: Vec<AgentState> = fx.sync.0.lock().unwrap().iter().map(|(_, s)| *s).collect();
        assert_eq!(states, [AgentState::Working, AgentState::Idle]);
        assert!(fx.sync.0.lock().unwrap().iter().all(|(w, _)| w == "w1"));

        let notes = fx.events.notes.lock().unwrap().clone();
        assert_eq!(
            notes,
            vec![AgentNotification {
                workspace_id: "w1".into(),
                session_id: session.id.clone(),
                kind: NotificationKind::Finished,
                detail: None
            }]
        );
        let changes = fx.events.changes.lock().unwrap().clone();
        assert_eq!(changes.first().unwrap().status, SessionStatus::Working);
        assert!(changes
            .iter()
            .all(|c| c.workspace_id == "w1" && c.session_id == session.id));
    }

    #[tokio::test]
    async fn two_workspaces_working_at_once_do_not_mix() {
        let fx = fixture();
        let (a, view_a) = new_session(&fx);
        let b = fx.manager.create("w2", NewSession::default()).unwrap();
        let view_b = Arc::new(View::default());
        fx.manager.attach(&b.id, view_b.clone()).unwrap();

        fx.manager
            .send(&fx.ctx, &a.id, "echo from a")
            .await
            .unwrap();
        fx.manager
            .send(&fx.ctx2, &b.id, "echo from b")
            .await
            .unwrap();
        settle(&fx, &a.id, SessionStatus::Idle).await;
        settle(&fx, &b.id, SessionStatus::Idle).await;
        assert_eq!(
            view_a.text_of(MessageKind::Text),
            ["echo from a", "echo: from a"]
        );
        assert_eq!(
            view_b.text_of(MessageKind::Text),
            ["echo from b", "echo: from b"]
        );
        let cwds: Vec<String> = log(&fx)
            .iter()
            .filter_map(|e| e.get("cwd"))
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        assert!(
            cwds.contains(&fx.dir.join("w1").display().to_string())
                && cwds.contains(&fx.dir.join("w2").display().to_string())
        );
        assert_eq!(
            fx.sync
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(w, _)| w == "w2")
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn the_process_is_reused_and_a_stopped_chat_resumes_the_same_conversation() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo one")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager
            .send(&fx.ctx, &session.id, "echo two")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert_eq!(starts(&fx).len(), 1, "two turns, one process");
        let claude_id = fx
            .manager
            .get(&session.id)
            .unwrap()
            .claude_session_id
            .unwrap();

        fx.manager.stop(&session.id).await.unwrap();
        assert!(!fx.manager.get(&session.id).unwrap().running);
        assert_eq!(status(&fx, &session.id), SessionStatus::Idle);

        fx.manager
            .send(&fx.ctx, &session.id, "echo three")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let all = starts(&fx);
        assert_eq!(all.len(), 2);
        assert!(all[0]
            .windows(2)
            .any(|w| w == ["--session-id", claude_id.as_str()]));
        assert!(
            all[1]
                .windows(2)
                .any(|w| w == ["--resume", claude_id.as_str()]),
            "{:?}",
            all[1]
        );
        assert_eq!(fx.manager.get(&session.id).unwrap().turns, 3);
    }

    #[tokio::test]
    async fn streamed_text_reaches_the_view_but_is_never_stored() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "stream")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let deltas: Vec<String> = view
            .items()
            .into_iter()
            .filter_map(|i| {
                if let StreamItem::Delta {
                    text,
                    kind: DeltaKind::Text,
                    ..
                } = i
                {
                    Some(text)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(deltas.concat(), "alpha beta gamma");
        let stored = fx.manager.messages(&session.id, None).unwrap();
        assert_eq!(
            stored
                .iter()
                .filter(|m| m.kind == MessageKind::Text && m.role == Role::Assistant)
                .count(),
            1
        );
        assert_eq!(stored.last().unwrap().content["text"], "alpha beta gamma");
    }

    #[tokio::test]
    async fn thinking_text_and_tool_calls_are_separate_messages() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "think")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let kinds: Vec<MessageKind> = fx
            .manager
            .messages(&session.id, None)
            .unwrap()
            .iter()
            .map(|m| m.kind)
            .collect();
        use MessageKind::*;
        assert_eq!(kinds, [Text, Thinking, Text, ToolCall, ToolResult, Text]);
    }

    #[tokio::test]
    async fn a_permission_request_blocks_the_session_until_answered() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "tool").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Blocked).await;

        let request = view.permissions().remove(0);
        assert_eq!(request.tool_name, "Write");
        assert_eq!(request.session_id, session.id);
        assert!(request.input["file_path"]
            .as_str()
            .unwrap()
            .ends_with("out.txt"));
        let snapshot = fx.manager.attach(&session.id, view.clone()).unwrap();
        assert_eq!(
            snapshot.pending,
            vec![request.clone()],
            "a view that attaches later still sees the request"
        );
        assert_eq!(fx.manager.active_count(), 1);
        assert!(fx
            .sync
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(_, s)| *s == AgentState::Blocked));
        let notes = fx.events.notes.lock().unwrap().clone();
        assert!(notes
            .iter()
            .any(|n| n.kind == NotificationKind::NeedsApproval
                && n.detail.as_deref() == Some("Write")));

        fx.manager
            .reply_permission(&session.id, &request.request_id, PermissionDecision::Allow)
            .unwrap();
        assert_eq!(status(&fx, &session.id), SessionStatus::Working);
        assert!(view.items().iter().any(|i| matches!(i, StreamItem::PermissionResolved { request_id } if request_id == &request.request_id)));
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert!(view
            .text_of(MessageKind::Text)
            .contains(&"Write done".to_string()));
        assert!(fx
            .manager
            .attach(&session.id, view.clone())
            .unwrap()
            .pending
            .is_empty());

        let states: Vec<AgentState> = fx.sync.0.lock().unwrap().iter().map(|(_, s)| *s).collect();
        assert_eq!(
            states,
            [
                AgentState::Working,
                AgentState::Blocked,
                AgentState::Working,
                AgentState::Idle
            ]
        );
        assert_eq!(
            view.statuses(),
            [
                SessionStatus::Working,
                SessionStatus::Blocked,
                SessionStatus::Working,
                SessionStatus::Idle
            ]
        );
    }

    #[tokio::test]
    async fn a_denied_tool_use_tells_claude_why() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "bash rm -rf build")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Blocked).await;
        let request = view.permissions().remove(0);
        fx.manager
            .reply_permission(
                &session.id,
                &request.request_id,
                PermissionDecision::Deny {
                    message: Some("Not that.".into()),
                },
            )
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let results: Vec<_> = fx
            .manager
            .messages(&session.id, None)
            .unwrap()
            .into_iter()
            .filter(|m| m.kind == MessageKind::ToolResult)
            .collect();
        assert_eq!(results[0].content["is_error"], true);
        assert_eq!(results[0].content["content"], "Not that.");
        assert!(view
            .text_of(MessageKind::Text)
            .contains(&"Bash denied".to_string()));
    }

    #[tokio::test]
    async fn allowing_for_the_session_skips_the_question_next_time_but_only_for_the_same_command() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "bash ls")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Blocked).await;
        let first = view.permissions().remove(0);
        fx.manager
            .reply_permission(
                &session.id,
                &first.request_id,
                PermissionDecision::AllowSession,
            )
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;

        // The same command goes straight through.
        fx.manager
            .send(&fx.ctx, &session.id, "bash ls")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        until("the second turn", || {
            fx.manager.get(&session.id).unwrap().turns == 2
        })
        .await;
        assert_eq!(view.permissions().len(), 1, "no second question");
        assert!(!view
            .statuses()
            .iter()
            .skip(2)
            .any(|s| *s == SessionStatus::Blocked));
        let auto_answers = fx
            .manager
            .events(&session.id, None, 1000)
            .unwrap()
            .iter()
            .filter(|e| e.kind == "sent:control_response")
            .count();
        assert_eq!(
            auto_answers, 2,
            "one from the user, one automatic, both recorded"
        );

        // A different command still asks.
        fx.manager
            .send(&fx.ctx, &session.id, "bash rm -rf /")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Blocked).await;
        assert_eq!(view.permissions().len(), 2);
        fx.manager
            .reply_permission(
                &session.id,
                &view.permissions()[1].request_id,
                PermissionDecision::Deny { message: None },
            )
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
    }

    #[tokio::test]
    async fn answering_a_request_twice_or_a_made_up_one_is_an_error() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "tool").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Blocked).await;
        let id = view.permissions()[0].request_id.clone();
        assert_eq!(
            fx.manager
                .reply_permission(&session.id, "made-up", PermissionDecision::Allow)
                .unwrap_err()
                .code(),
            "not_found"
        );
        fx.manager
            .reply_permission(&session.id, &id, PermissionDecision::Allow)
            .unwrap();
        assert_eq!(
            fx.manager
                .reply_permission(&session.id, &id, PermissionDecision::Allow)
                .unwrap_err()
                .code(),
            "not_found"
        );
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager.stop(&session.id).await.unwrap();
        assert_eq!(
            fx.manager
                .reply_permission(&session.id, &id, PermissionDecision::Allow)
                .unwrap_err()
                .code(),
            "invalid"
        );
    }

    #[tokio::test]
    async fn interrupting_ends_the_turn_and_the_chat_stays_usable() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "slow").await.unwrap();
        until("deltas", || {
            view.items()
                .iter()
                .filter(|i| matches!(i, StreamItem::Delta { .. }))
                .count()
                >= 2
        })
        .await;

        fx.manager.interrupt(&session.id).unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let notice = view
            .messages()
            .into_iter()
            .find(|m| m.role == Role::System)
            .expect("an interrupt note");
        assert_eq!(notice.content["interrupted"], true);
        assert!(fx.manager.get(&session.id).unwrap().running);
        assert!(fx
            .manager
            .events(&session.id, None, 1000)
            .unwrap()
            .iter()
            .any(|e| e.kind == "sent:control_request"));

        fx.manager
            .send(&fx.ctx, &session.id, "echo back")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        until("the answer", || {
            view.text_of(MessageKind::Text)
                .contains(&"echo: back".to_string())
        })
        .await;
    }

    #[tokio::test]
    async fn interrupting_a_chat_that_is_idle_does_nothing() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager.interrupt(&session.id).unwrap();
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager.interrupt(&session.id).unwrap();
        assert!(!fx
            .manager
            .events(&session.id, None, 1000)
            .unwrap()
            .iter()
            .any(|e| e.kind == "sent:control_request"));
        assert_eq!(
            fx.manager.interrupt("missing").unwrap_err().code(),
            "not_found"
        );
    }

    #[tokio::test]
    async fn interrupting_while_waiting_for_approval_cancels_the_request() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "tool").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Blocked).await;
        let id = view.permissions()[0].request_id.clone();
        fx.manager.interrupt(&session.id).unwrap();
        assert!(view.items().iter().any(
            |i| matches!(i, StreamItem::PermissionResolved { request_id } if request_id == &id)
        ));
        assert!(fx
            .manager
            .attach(&session.id, view.clone())
            .unwrap()
            .pending
            .is_empty());
        fx.manager.stop(&session.id).await.unwrap();
    }

    #[tokio::test]
    async fn a_message_during_a_turn_steers_it() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "steer")
            .await
            .unwrap();
        until("the turn to run", || {
            view.items()
                .iter()
                .any(|i| matches!(i, StreamItem::Delta { .. }))
        })
        .await;
        let steering = fx
            .manager
            .send(&fx.ctx, &session.id, "use tabs")
            .await
            .unwrap();
        assert_eq!(steering.content["steered"], true);
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert!(
            view.text_of(MessageKind::Text)
                .contains(&"steered:use tabs".to_string()),
            "{:?}",
            view.text_of(MessageKind::Text)
        );
        assert_eq!(
            starts(&fx).len(),
            1,
            "steering does not start another process"
        );
        assert_eq!(
            view.statuses(),
            [SessionStatus::Working, SessionStatus::Idle],
            "no extra status change for a steer"
        );
    }

    #[tokio::test]
    async fn a_crash_mid_turn_interrupts_the_session_and_explains_why() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "crash")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Interrupted).await;
        let after = fx.manager.get(&session.id).unwrap();
        assert!(!after.running);
        assert!(
            after.last_error.as_deref().unwrap().contains("exit code 3"),
            "{:?}",
            after.last_error
        );
        let note = view
            .messages()
            .into_iter()
            .find(|m| m.role == Role::System)
            .expect("a system note");
        assert_eq!(note.content["error"], true);
        let text = note.content["text"].as_str().unwrap();
        assert!(
            text.contains("exit code 3") && text.contains("fake claude crashed on purpose"),
            "{text}"
        );
        let notes = fx.events.notes.lock().unwrap().clone();
        assert!(notes.iter().any(|n| n.kind == NotificationKind::Failed));
        assert!(fx.sync.0.lock().unwrap().last().map(|(_, s)| *s) == Some(AgentState::Idle));

        // Sending again resumes the conversation.
        fx.manager
            .send(&fx.ctx, &session.id, "echo recovered")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert!(starts(&fx)[1].contains(&"--resume".to_string()));
        assert_eq!(fx.manager.get(&session.id).unwrap().last_error, None);
    }

    #[tokio::test]
    async fn a_crash_while_idle_is_not_an_error() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager.send(&fx.ctx, &session.id, "bye").await.unwrap();
        until("the process to end", || {
            !fx.manager.get(&session.id).unwrap().running
        })
        .await;
        // It was working on "bye" when it ended, so that one is an interruption.
        assert_eq!(status(&fx, &session.id), SessionStatus::Interrupted);
    }

    #[tokio::test]
    async fn a_failed_turn_is_reported_as_a_failure_but_the_chat_goes_on() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "fail").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let note = view
            .messages()
            .into_iter()
            .find(|m| m.role == Role::System)
            .unwrap();
        assert_eq!(note.content["text"], "Something went wrong");
        assert_eq!(
            fx.manager.get(&session.id).unwrap().last_error.as_deref(),
            Some("Something went wrong")
        );
        let notes = fx.events.notes.lock().unwrap().clone();
        assert!(notes.iter().any(|n| n.kind == NotificationKind::Failed
            && n.detail.as_deref() == Some("Something went wrong")));
        assert!(fx.manager.get(&session.id).unwrap().running);
    }

    #[tokio::test]
    async fn stopping_a_busy_chat_leaves_it_idle_with_a_note() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "slow").await.unwrap();
        until("the turn to run", || {
            view.items()
                .iter()
                .any(|i| matches!(i, StreamItem::Delta { .. }))
        })
        .await;
        fx.manager.stop(&session.id).await.unwrap();
        assert_eq!(status(&fx, &session.id), SessionStatus::Idle);
        assert!(!fx.manager.get(&session.id).unwrap().running);
        assert_eq!(
            view.messages()
                .into_iter()
                .find(|m| m.role == Role::System)
                .unwrap()
                .content["text"],
            "Stopped."
        );
        assert!(
            !fx.events
                .notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.kind == NotificationKind::Failed),
            "a stop is not a failure"
        );
    }

    #[tokio::test]
    async fn closing_a_chat_hides_it_and_refuses_messages() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager.close(&session.id).await.unwrap();
        assert_eq!(status(&fx, &session.id), SessionStatus::Closed);
        assert!(fx.manager.list("w1").unwrap().is_empty());
        assert_eq!(
            fx.manager
                .send(&fx.ctx, &session.id, "echo more")
                .await
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            fx.manager
                .resume(&fx.ctx, &session.id)
                .await
                .unwrap_err()
                .code(),
            "invalid"
        );
    }

    #[tokio::test]
    async fn a_closed_chat_is_in_the_history_and_can_be_reopened() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager.close(&session.id).await.unwrap();
        assert!(fx.manager.list("w1").unwrap().is_empty());
        let history = fx.manager.list_with("w1", true).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, SessionStatus::Closed);

        let reopened = fx.manager.reopen(&session.id).unwrap();
        assert_eq!(reopened.status, SessionStatus::Idle);
        assert_eq!(fx.manager.list("w1").unwrap().len(), 1);
        fx.manager
            .send(&fx.ctx, &session.id, "echo back")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
    }

    #[tokio::test]
    async fn chats_can_be_put_in_a_new_order() {
        let fx = fixture();
        let a = fx.manager.create("w1", NewSession::default()).unwrap();
        let b = fx.manager.create("w1", NewSession::default()).unwrap();
        let c = fx.manager.create("w1", NewSession::default()).unwrap();
        let ids = |fx: &Fx| -> Vec<String> {
            fx.manager
                .list("w1")
                .unwrap()
                .into_iter()
                .map(|s| s.id)
                .collect()
        };
        assert_eq!(ids(&fx), [a.id.clone(), b.id.clone(), c.id.clone()]);
        fx.manager
            .reorder(
                "w1",
                &[c.id.clone(), a.id.clone(), b.id.clone(), "nope".into()],
            )
            .unwrap();
        assert_eq!(ids(&fx), [c.id, a.id, b.id]);
    }

    #[tokio::test]
    async fn attachments_are_listed_in_the_prompt_and_kept_on_the_message() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        let attachment = crate::chat::Attachment {
            name: "shot.png".into(),
            path: "/w/.context/attachments/ab12-shot.png".into(),
            size: 3,
        };
        let message = fx
            .manager
            .send_with(
                &fx.ctx,
                &session.id,
                "echo look",
                std::slice::from_ref(&attachment),
            )
            .await
            .unwrap();
        assert_eq!(message.content["text"], "echo look");
        assert_eq!(message.content["attachments"][0]["name"], "shot.png");
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let log = std::fs::read_to_string(&fx.log).unwrap_or_default();
        assert!(log.contains("Attached files"), "{log}");
        assert!(log.contains("ab12-shot.png"), "{log}");

        // Only attachments, no text, is still a message.
        fx.manager
            .send_with(&fx.ctx, &session.id, "", std::slice::from_ref(&attachment))
            .await
            .unwrap();
        assert_eq!(
            fx.manager
                .send_with(&fx.ctx, &session.id, "  ", &[])
                .await
                .unwrap_err()
                .code(),
            "invalid"
        );
        settle(&fx, &session.id, SessionStatus::Idle).await;
    }

    #[tokio::test]
    async fn stopping_a_workspace_stops_all_of_its_chats_only() {
        let fx = fixture();
        let (a, _) = new_session(&fx);
        let (b, _) = new_session(&fx);
        let other = fx.manager.create("w2", NewSession::default()).unwrap();
        for (ctx, id) in [(&fx.ctx, &a.id), (&fx.ctx, &b.id), (&fx.ctx2, &other.id)] {
            fx.manager.send(ctx, id, "echo hi").await.unwrap();
            settle(&fx, id, SessionStatus::Idle).await;
        }
        fx.manager.stop_workspace("w1").await;
        assert!(!fx.manager.get(&a.id).unwrap().running && !fx.manager.get(&b.id).unwrap().running);
        assert!(fx.manager.get(&other.id).unwrap().running);
        fx.manager.stop_workspace("w2").await;
    }

    #[tokio::test]
    async fn startup_marks_chats_that_were_working_as_interrupted() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager
            .store()
            .set_status(&session.id, SessionStatus::Working, None)
            .unwrap();
        assert_eq!(fx.manager.recover_after_restart().unwrap(), 1);
        assert_eq!(status(&fx, &session.id), SessionStatus::Interrupted);
        let note = view
            .messages()
            .into_iter()
            .find(|m| m.role == Role::System)
            .unwrap();
        assert!(note.content["text"]
            .as_str()
            .unwrap()
            .contains("Circles stopped"));
        assert_eq!(fx.manager.recover_after_restart().unwrap(), 0);
        // Sending a message continues the chat.
        fx.manager
            .send(&fx.ctx, &session.id, "echo again")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
    }

    #[tokio::test]
    async fn the_permission_mode_changes_a_running_process_at_once() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let updated = fx
            .manager
            .update_options(&session.id, None, None, Some(PermissionMode::AcceptEdits))
            .await
            .unwrap();
        assert_eq!(updated.permission_mode, PermissionMode::AcceptEdits);
        assert!(updated.running, "no restart for a mode change");
        until("the mode request", || {
            log(&fx)
                .iter()
                .any(|e| e.get("mode_set") == Some(&serde_json::json!("acceptEdits")))
        })
        .await;
        assert_eq!(starts(&fx).len(), 1);
    }

    #[tokio::test]
    async fn a_new_model_restarts_the_process_between_turns_and_is_refused_during_one() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;

        let updated = fx
            .manager
            .update_options(&session.id, Some("opus"), Some("max"), None)
            .await
            .unwrap();
        assert!(
            !updated.running,
            "the process ends so the next message starts with the new model"
        );
        fx.manager.send(&fx.ctx, &session.id, "slow").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Working).await;
        until("the second process to start", || starts(&fx).len() == 2).await;
        let all = starts(&fx);
        assert_eq!(all.len(), 2);
        assert!(
            all[1].windows(2).any(|w| w == ["--model", "opus"])
                && all[1].windows(2).any(|w| w == ["--effort", "max"])
        );
        assert!(all[1].contains(&"--resume".to_string()));

        let refused = fx
            .manager
            .update_options(&session.id, Some("haiku"), None, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code(), "invalid");
        assert!(refused.to_string().contains("Wait for the current turn"));
        fx.manager.stop(&session.id).await.unwrap();
        // The same model again is not a change, so nothing restarts.
        let same = fx
            .manager
            .update_options(&session.id, Some("opus"), None, None)
            .await
            .unwrap();
        assert_eq!(same.model.as_deref(), Some("opus"));
    }

    #[tokio::test]
    async fn a_fork_starts_from_a_copy_of_the_conversation_and_gets_its_own_id() {
        let fx = fixture();
        let session = fx
            .manager
            .create(
                "w1",
                NewSession {
                    fork_of: Some("parent-claude-id".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        fx.manager
            .send(&fx.ctx, &session.id, "echo branch")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let args = &starts(&fx)[0];
        assert!(args
            .windows(2)
            .any(|w| w == ["--resume", "parent-claude-id"]));
        assert!(args.contains(&"--fork-session".to_string()));
        let own = fx
            .manager
            .get(&session.id)
            .unwrap()
            .claude_session_id
            .unwrap();
        assert_ne!(own, "parent-claude-id");
        assert!(args.windows(2).any(|w| w == ["--session-id", own.as_str()]));

        // Later starts resume the fork itself, not the parent.
        fx.manager.stop(&session.id).await.unwrap();
        fx.manager
            .send(&fx.ctx, &session.id, "echo more")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert!(starts(&fx)[1]
            .windows(2)
            .any(|w| w == ["--resume", own.as_str()]));
        assert!(!starts(&fx)[1].contains(&"--fork-session".to_string()));
    }

    #[tokio::test]
    async fn the_rate_limit_is_kept_and_announced() {
        let fx = fixture();
        assert_eq!(fx.manager.rate_limit(), None);
        let (session, view) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let limit = fx.manager.rate_limit().expect("a rate limit");
        assert_eq!(
            (
                limit.status.as_str(),
                limit.window.as_str(),
                limit.five_hour_utilization
            ),
            ("allowed", "five_hour", Some(0.4))
        );
        assert_eq!(fx.events.limits.lock().unwrap().len(), 1);
        assert!(view
            .items()
            .iter()
            .any(|i| matches!(i, StreamItem::RateLimit(_))));
    }

    #[tokio::test]
    async fn requests_the_app_does_not_handle_are_declined_so_claude_does_not_wait() {
        let fx = fixture();
        let (session, view) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "mcp").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert!(view
            .text_of(MessageKind::Text)
            .contains(&"mcp handled".to_string()));
        let answer = log(&fx)
            .into_iter()
            .find_map(|e| e.get("mcp_answer").cloned())
            .unwrap();
        assert_eq!(answer["subtype"], "error");
        assert!(answer["error"].as_str().unwrap().contains("mcp_message"));
    }

    #[tokio::test]
    async fn a_missing_claude_fails_the_send_clearly_and_leaves_the_session_idle() {
        let fx = fixture_with(false);
        let (session, _) = new_session(&fx);
        let err = fx
            .manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap_err();
        assert_eq!(err.code(), "claude_missing");
        assert_eq!(status(&fx, &session.id), SessionStatus::Idle);
        assert!(
            fx.manager.messages(&session.id, None).unwrap().is_empty(),
            "nothing was recorded for a send that never happened"
        );
    }

    #[tokio::test]
    async fn empty_messages_and_unknown_sessions_are_refused() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        assert_eq!(
            fx.manager
                .send(&fx.ctx, &session.id, "   ")
                .await
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            fx.manager
                .send(&fx.ctx, "nope", "echo hi")
                .await
                .unwrap_err()
                .code(),
            "not_found"
        );
        assert_eq!(
            fx.manager.messages("nope", None).unwrap_err().code(),
            "not_found"
        );
        assert_eq!(
            fx.manager
                .attach("nope", Arc::new(View::default()))
                .unwrap_err()
                .code(),
            "not_found"
        );
        assert!(starts(&fx).is_empty());
    }

    #[tokio::test]
    async fn a_view_that_attaches_late_reads_the_history_from_the_database() {
        let fx = fixture();
        let (session, first) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo one")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager.detach(&session.id);
        fx.manager
            .send(&fx.ctx, &session.id, "echo two")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let seen_by_first = first.messages().len();

        let history = fx.manager.messages(&session.id, None).unwrap();
        assert_eq!(
            history.len(),
            4,
            "everything was stored while no view was attached"
        );
        assert_eq!(
            first.messages().len(),
            seen_by_first,
            "a detached view receives nothing"
        );
        assert_eq!(fx.manager.messages(&session.id, Some(2)).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn the_agent_process_gets_the_workspace_environment_and_its_session_id() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo hi")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let env = log(&fx)[0]["env"].clone();
        assert_eq!(env["CIRCLES_PORT"], "55010");
        assert_eq!(env["CIRCLES_SESSION_ID"], session.id.as_str());
    }

    #[tokio::test]
    async fn quitting_kills_every_process_without_calling_it_a_crash() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager.send(&fx.ctx, &session.id, "slow").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Working).await;
        assert_eq!(fx.manager.active_count(), 1);
        fx.manager.kill_all();
        until("the process to end", || {
            !fx.manager.get(&session.id).unwrap().running
        })
        .await;
        assert_eq!(
            status(&fx, &session.id),
            SessionStatus::Idle,
            "stopping on purpose is not a failure"
        );
        assert_eq!(fx.manager.active_count(), 0);
    }

    #[derive(Default)]
    struct RecordingHook {
        started: Mutex<Vec<(String, String, Option<String>)>>,
        linked: Mutex<Vec<(String, i64)>>,
    }

    #[async_trait]
    impl TurnHook for RecordingHook {
        async fn before_turn(
            &self,
            workspace_id: &str,
            session_id: &str,
            claude_session_id: Option<&str>,
            _cwd: &std::path::Path,
        ) -> Option<String> {
            self.started.lock().unwrap().push((
                workspace_id.into(),
                session_id.into(),
                claude_session_id.map(str::to_string),
            ));
            Some(format!("handle-{}", self.started.lock().unwrap().len()))
        }

        fn after_message(&self, handle: &str, message_seq: i64) {
            self.linked
                .lock()
                .unwrap()
                .push((handle.into(), message_seq));
        }
    }

    #[tokio::test]
    async fn each_new_turn_is_announced_before_it_starts_and_steering_is_not() {
        let fx = fixture();
        let hook = Arc::new(RecordingHook::default());
        fx.manager.set_turn_hook(hook.clone());
        let (session, _) = new_session(&fx);

        let first = fx
            .manager
            .send(&fx.ctx, &session.id, "echo one")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let second = fx
            .manager
            .send(&fx.ctx, &session.id, "echo two")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;

        let started = hook.started.lock().unwrap().clone();
        assert_eq!(started.len(), 2);
        assert_eq!(started[0].0, "w1");
        assert_eq!(started[0].1, session.id);
        assert_eq!(
            started[0].2, None,
            "the first turn has no agent session yet"
        );
        assert!(
            started[1].2.is_some(),
            "the second knows the session it continues"
        );
        assert_eq!(
            *hook.linked.lock().unwrap(),
            [
                ("handle-1".to_string(), first.seq),
                ("handle-2".to_string(), second.seq)
            ]
        );

        // A message sent while the agent works steers the turn, and is not a turn of its own.
        fx.manager.send(&fx.ctx, &session.id, "slow").await.unwrap();
        settle(&fx, &session.id, SessionStatus::Working).await;
        fx.manager
            .send(&fx.ctx, &session.id, "actually, stop")
            .await
            .unwrap();
        assert_eq!(
            hook.started.lock().unwrap().len(),
            3,
            "only the slow turn was announced"
        );
        fx.manager.interrupt(&session.id).unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
    }

    #[tokio::test]
    async fn a_chat_with_history_but_no_agent_session_starts_a_new_one_instead_of_resuming() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .send(&fx.ctx, &session.id, "echo one")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        fx.manager.stop(&session.id).await.unwrap();
        // What a revert does: the messages stay, the agent's session is forgotten.
        fx.manager
            .store()
            .db_for_tests()
            .write(|c| {
                c.execute(
                    "UPDATE sessions SET claude_session_id = NULL WHERE id = ?1",
                    [&session.id],
                )
            })
            .unwrap();

        fx.manager
            .send(&fx.ctx, &session.id, "echo two")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let starts: Vec<String> = std::fs::read_to_string(&fx.log)
            .unwrap()
            .lines()
            .filter(|l| l.contains("\"argv\""))
            .map(str::to_string)
            .collect();
        assert_eq!(starts.len(), 2);
        assert!(starts[1].contains("--session-id"), "{}", starts[1]);
        assert!(!starts[1].contains("--resume"), "{}", starts[1]);

        // With a session to continue, it is resumed as before.
        fx.manager.stop(&session.id).await.unwrap();
        fx.manager
            .send(&fx.ctx, &session.id, "echo three")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let last = std::fs::read_to_string(&fx.log)
            .unwrap()
            .lines()
            .rfind(|l| l.contains("\"argv\""))
            .unwrap()
            .to_string();
        assert!(last.contains("--resume"), "{last}");
    }

    #[tokio::test]
    async fn a_chat_that_starts_over_tells_the_agent_what_came_before_once() {
        let fx = fixture();
        let (session, _) = new_session(&fx);
        fx.manager
            .store()
            .set_replay_context(
                &session.id,
                Some("EARLIER: the user asked for a login form."),
            )
            .unwrap();
        let first = fx
            .manager
            .send(&fx.ctx, &session.id, "echo hello")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        assert_eq!(
            first.content["text"], "echo hello",
            "the chat shows only what was typed"
        );
        let log = std::fs::read_to_string(&fx.log).unwrap();
        assert!(
            log.contains("EARLIER: the user asked for a login form."),
            "{log}"
        );
        assert!(log.contains("echo hello"));

        fx.manager
            .send(&fx.ctx, &session.id, "echo again")
            .await
            .unwrap();
        settle(&fx, &session.id, SessionStatus::Idle).await;
        let log = std::fs::read_to_string(&fx.log).unwrap();
        assert_eq!(log.matches("EARLIER:").count(), 1, "it is told once");
        assert_eq!(
            fx.manager.store().take_replay_context(&session.id).unwrap(),
            None
        );
    }

    /// Tests against the real Claude Code, with the cheapest model. They spend a little real API
    /// credit, so they only run on request:
    /// `cargo test --lib -- --ignored --nocapture --test-threads=1 real_claude`
    mod real {
        use super::*;

        struct Rx {
            _tmp: tempfile::TempDir,
            dir: PathBuf,
            manager: Arc<SessionManager>,
            ctx: Context,
        }

        async fn real() -> Rx {
            let tmp = tempfile::tempdir().unwrap();
            let dir = std::fs::canonicalize(tmp.path()).unwrap();
            let work = dir.join("work");
            std::fs::create_dir_all(&work).unwrap();
            let db = Arc::new(Db::open(&dir.join("t.db")).unwrap());
            db.write(|c| {
                c.execute_batch(
                    "INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1);
                     INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                       VALUES ('w1','p','Oslo','b','main','/w1','ready',1,1);",
                )
            })
            .unwrap();
            let env = Arc::new(crate::env::resolve().await);
            let manager = SessionManager::new(
                SessionStore::new(db.clone()),
                Arc::new(ClaudeCode::new(env, Some(db))),
                Arc::new(Events::default()),
                Arc::new(Sync::default()),
            );
            let ctx = Context {
                workspace_id: "w1".into(),
                project_id: "p".into(),
                cwd: work,
                env: vec![],
            };
            Rx {
                _tmp: tmp,
                dir,
                manager,
                ctx,
            }
        }

        fn haiku() -> NewSession {
            NewSession {
                model: Some("haiku".into()),
                effort: Some("low".into()),
                ..Default::default()
            }
        }

        async fn wait_status(rx: &Rx, id: &str, want: SessionStatus) {
            let deadline = Instant::now() + Duration::from_secs(120);
            while rx.manager.store().get(id).unwrap().status != want {
                assert!(Instant::now() < deadline, "timed out waiting for {want:?}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        fn assistant_text(rx: &Rx, id: &str) -> String {
            rx.manager
                .messages(id, None)
                .unwrap()
                .iter()
                .filter(|m| m.role == Role::Assistant && m.kind == MessageKind::Text)
                .map(|m| {
                    m.content["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase()
                })
                .collect::<Vec<_>>()
                .join(" ")
        }

        #[tokio::test]
        #[ignore = "uses the real Claude Code and a little API credit"]
        async fn real_claude_turn_resume_and_fork() {
            let rx = real().await;
            let session = rx.manager.create("w1", haiku()).unwrap();
            let view = Arc::new(View::default());
            rx.manager.attach(&session.id, view.clone()).unwrap();

            rx.manager
                .send(
                    &rx.ctx,
                    &session.id,
                    "Remember the secret word: kiwi. Reply with only the word: stored",
                )
                .await
                .unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            let first = rx.manager.get(&session.id).unwrap();
            println!(
                "turn 1: model={:?} turns={} cost=${:.4} claude_id={:?}",
                first.model, first.turns, first.total_cost_usd, first.claude_session_id
            );
            println!("assistant said: {:?}", assistant_text(&rx, &session.id));
            assert_eq!(first.turns, 1);
            assert!(first.total_cost_usd > 0.0);
            assert!(assistant_text(&rx, &session.id).contains("stored"));
            assert!(
                view.items()
                    .iter()
                    .any(|i| matches!(i, StreamItem::Delta { .. })),
                "partial messages stream"
            );
            assert!(view
                .items()
                .iter()
                .any(|i| matches!(i, StreamItem::Init { .. })));
            println!("rate limit: {:?}", rx.manager.rate_limit());
            let claude_id = first.claude_session_id.clone().unwrap();

            // Resume: the process ends, and the next message continues the conversation.
            rx.manager.stop(&session.id).await.unwrap();
            rx.manager
                .send(
                    &rx.ctx,
                    &session.id,
                    "What was the secret word? Reply with only the word.",
                )
                .await
                .unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            println!("after resume: {:?}", assistant_text(&rx, &session.id));
            assert!(
                assistant_text(&rx, &session.id).matches("kiwi").count() >= 1,
                "resume kept the conversation"
            );
            assert_eq!(
                rx.manager
                    .get(&session.id)
                    .unwrap()
                    .claude_session_id
                    .as_deref(),
                Some(claude_id.as_str()),
                "same conversation id"
            );

            // Fork: a copy of the conversation under its own id.
            let fork = rx
                .manager
                .create(
                    "w1",
                    NewSession {
                        fork_of: Some(claude_id.clone()),
                        ..haiku()
                    },
                )
                .unwrap();
            rx.manager
                .send(
                    &rx.ctx,
                    &fork.id,
                    "What was the secret word? Reply with only the word.",
                )
                .await
                .unwrap();
            wait_status(&rx, &fork.id, SessionStatus::Idle).await;
            let forked = rx.manager.get(&fork.id).unwrap();
            println!(
                "fork: claude_id={:?} said {:?}",
                forked.claude_session_id,
                assistant_text(&rx, &fork.id)
            );
            assert!(
                assistant_text(&rx, &fork.id).contains("kiwi"),
                "the fork knows what the original knew"
            );
            assert_ne!(
                forked.claude_session_id.as_deref(),
                Some(claude_id.as_str()),
                "the fork has its own id"
            );
            rx.manager.stop_workspace("w1").await;
        }

        #[tokio::test]
        #[ignore = "uses the real Claude Code and a little API credit"]
        async fn real_claude_forgets_the_turns_that_were_reverted() {
            let rx = real().await;
            let work = rx.ctx.cwd.clone();
            let git = |args: &[&str]| {
                let ok = std::process::Command::new("git")
                    .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
                    .args(args)
                    .current_dir(&work)
                    .status()
                    .unwrap()
                    .success();
                assert!(ok, "git {args:?}");
            };
            git(&["init", "-q"]);
            std::fs::write(work.join("README.md"), "hi\n").unwrap();
            git(&["add", "."]);
            git(&["commit", "-q", "-m", "start"]);

            let env = Arc::new(crate::env::resolve().await);
            let db = rx.manager.store().db_for_tests();
            let checkpoints = crate::checkpoint::Checkpoints::new(crate::git::Git::new(env), db);
            rx.manager.set_turn_hook(Arc::new(checkpoints.clone()));
            let session = rx.manager.create("w1", haiku()).unwrap();

            rx.manager
                .send(
                    &rx.ctx,
                    &session.id,
                    "Remember this secret word: kiwi. Reply with only: stored",
                )
                .await
                .unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            rx.manager
                .send(
                    &rx.ctx,
                    &session.id,
                    "Also remember a second secret word: mango. Reply with only: stored",
                )
                .await
                .unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;

            let turns = checkpoints.list(&session.id).unwrap();
            assert_eq!(turns.len(), 2);
            rx.manager.stop(&session.id).await.unwrap();
            checkpoints.revert(&turns[1].id, &work).await.unwrap();

            rx.manager
                .send(
                    &rx.ctx,
                    &session.id,
                    "What secret word did I give you earlier? Answer with just that word, or say none.",
                )
                .await
                .unwrap();
            // The chat is idle right after a revert, so wait for the new turn itself to finish.
            let finished_before = rx.manager.get(&session.id).unwrap().turns;
            let deadline = Instant::now() + Duration::from_secs(120);
            while rx.manager.get(&session.id).unwrap().turns <= finished_before {
                assert!(Instant::now() < deadline, "the turn never finished");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let said = rx
                .manager
                .messages(&session.id, None)
                .unwrap()
                .iter()
                .filter(|m| m.role == Role::Assistant && m.kind == MessageKind::Text)
                .map(|m| {
                    m.content["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase()
                })
                .next_back()
                .unwrap_or_default();
            println!("after the revert the agent said: {said:?}");
            assert!(
                said.contains("kiwi"),
                "it still knows what came before the revert point"
            );
            assert!(
                !said.contains("mango"),
                "it has no memory of the reverted turn"
            );
            rx.manager.stop_workspace("w1").await;
        }

        #[tokio::test]
        #[ignore = "uses the real Claude Code and a little API credit"]
        async fn real_claude_permissions() {
            let rx = real().await;
            let session = rx.manager.create("w1", haiku()).unwrap();
            let view = Arc::new(View::default());
            rx.manager.attach(&session.id, view.clone()).unwrap();

            rx.manager.send(&rx.ctx, &session.id, "Use the Write tool to create a file named allowed.txt in the current directory containing exactly: yes. Then reply done.").await.unwrap();
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                let status = rx.manager.store().get(&session.id).unwrap().status;
                if status == SessionStatus::Blocked || status == SessionStatus::Idle {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "no permission request and no result"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let requests = view.permissions();
            if requests.is_empty() {
                println!("NOTE: no permission request arrived. Your Claude settings probably already allow Write, so this scenario cannot be checked here.");
                rx.manager.stop_workspace("w1").await;
                return;
            }
            let request = &requests[0];
            println!("permission request: tool={} display={:?} input={} tool_use_id={:?} blocked_path={:?} description={:?}", request.tool_name, request.display_name, request.input, request.tool_use_id, request.blocked_path, request.description);
            assert_eq!(request.tool_name, "Write");
            assert_eq!(
                rx.manager.store().get(&session.id).unwrap().status,
                SessionStatus::Blocked
            );
            assert!(
                !rx.ctx.cwd.join("allowed.txt").exists(),
                "nothing is written before the user answers"
            );

            rx.manager
                .reply_permission(
                    &session.id,
                    &request.request_id,
                    PermissionDecision::AllowSession,
                )
                .unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            println!("assistant: {:?}", assistant_text(&rx, &session.id));
            assert_eq!(
                std::fs::read_to_string(rx.ctx.cwd.join("allowed.txt"))
                    .unwrap()
                    .trim(),
                "yes"
            );

            // Allowed for the session: a second write goes through without asking.
            let asked = view.permissions().len();
            rx.manager.send(&rx.ctx, &session.id, "Use the Write tool to create a file named second.txt containing exactly: two. Then reply done.").await.unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            println!(
                "second write asked again: {}",
                view.permissions().len() > asked
            );
            assert_eq!(view.permissions().len(), asked, "AllowSession is honoured");
            assert!(rx.ctx.cwd.join("second.txt").exists());

            // A new session, denying.
            let denied = rx.manager.create("w1", haiku()).unwrap();
            let denied_view = Arc::new(View::default());
            rx.manager.attach(&denied.id, denied_view.clone()).unwrap();
            rx.manager.send(&rx.ctx, &denied.id, "Use the Write tool to create a file named denied.txt containing exactly: no. Then reply done.").await.unwrap();
            wait_status(&rx, &denied.id, SessionStatus::Blocked).await;
            let request = denied_view.permissions().remove(0);
            rx.manager
                .reply_permission(
                    &denied.id,
                    &request.request_id,
                    PermissionDecision::Deny {
                        message: Some("The user does not want that file.".into()),
                    },
                )
                .unwrap();
            wait_status(&rx, &denied.id, SessionStatus::Idle).await;
            println!("after deny: {:?}", assistant_text(&rx, &denied.id));
            assert!(
                !rx.ctx.cwd.join("denied.txt").exists(),
                "a denied write does not happen"
            );
            rx.manager.stop_workspace("w1").await;
        }

        #[tokio::test]
        #[ignore = "uses the real Claude Code and a little API credit"]
        async fn real_claude_interrupt_and_steer() {
            let rx = real().await;
            let session = rx.manager.create("w1", haiku()).unwrap();
            let view = Arc::new(View::default());
            rx.manager.attach(&session.id, view.clone()).unwrap();

            rx.manager
                .send(
                    &rx.ctx,
                    &session.id,
                    "Count from 1 to 400, one number per line, with no other text and no tools.",
                )
                .await
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(60);
            while !view
                .items()
                .iter()
                .any(|i| matches!(i, StreamItem::Delta { .. }))
            {
                assert!(Instant::now() < deadline, "no streamed text");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            rx.manager.interrupt(&session.id).unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            let notes: Vec<_> = view
                .messages()
                .into_iter()
                .filter(|m| m.role == Role::System)
                .collect();
            println!(
                "system notes after interrupt: {:?}",
                notes
                    .iter()
                    .map(|m| m.content.to_string())
                    .collect::<Vec<_>>()
            );
            assert!(
                notes.iter().any(|m| m.content["interrupted"] == true),
                "the interrupt is noted"
            );
            assert!(
                rx.manager.get(&session.id).unwrap().running,
                "the process survives an interrupt"
            );

            rx.manager
                .send(&rx.ctx, &session.id, "Reply with only the word: alive")
                .await
                .unwrap();
            wait_status(&rx, &session.id, SessionStatus::Idle).await;
            println!(
                "after interrupt: {:?}",
                assistant_text(&rx, &session.id)
                    .chars()
                    .rev()
                    .take(40)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            );
            assert!(assistant_text(&rx, &session.id).contains("alive"));
            rx.manager.stop_workspace("w1").await;
        }

        #[tokio::test]
        #[ignore = "uses the real Claude Code and a little API credit"]
        async fn real_claude_permission_modes() {
            let rx = real().await;

            // Bypass: nothing asks.
            let bypass = rx
                .manager
                .create(
                    "w1",
                    NewSession {
                        permission_mode: Some(PermissionMode::Bypass),
                        ..haiku()
                    },
                )
                .unwrap();
            let bypass_view = Arc::new(View::default());
            rx.manager.attach(&bypass.id, bypass_view.clone()).unwrap();
            rx.manager.send(&rx.ctx, &bypass.id, "Use the Write tool to create a file named bypass.txt containing exactly: ok. Then reply done.").await.unwrap();
            wait_status(&rx, &bypass.id, SessionStatus::Idle).await;
            println!(
                "bypass: asked={} file={}",
                !bypass_view.permissions().is_empty(),
                rx.ctx.cwd.join("bypass.txt").exists()
            );
            if let Some(StreamItem::Init {
                permission_mode, ..
            }) = bypass_view
                .items()
                .into_iter()
                .find(|i| matches!(i, StreamItem::Init { .. }))
            {
                println!("bypass init permission mode: {permission_mode:?}");
            }
            assert!(bypass_view.permissions().is_empty());
            assert!(rx.ctx.cwd.join("bypass.txt").exists());

            // Plan: Claude does not change anything.
            let plan = rx
                .manager
                .create(
                    "w1",
                    NewSession {
                        permission_mode: Some(PermissionMode::Plan),
                        ..haiku()
                    },
                )
                .unwrap();
            let plan_view = Arc::new(View::default());
            rx.manager.attach(&plan.id, plan_view.clone()).unwrap();
            rx.manager
                .send(
                    &rx.ctx,
                    &plan.id,
                    "Create a file named planned.txt containing exactly: p. Then reply done.",
                )
                .await
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                let status = rx.manager.store().get(&plan.id).unwrap().status;
                if status != SessionStatus::Working {
                    if status == SessionStatus::Blocked {
                        // Finishing a plan asks for approval of the plan itself. Decline it.
                        let request = plan_view.permissions().remove(0);
                        println!("plan mode asked about: {}", request.tool_name);
                        rx.manager
                            .reply_permission(
                                &plan.id,
                                &request.request_id,
                                PermissionDecision::Deny {
                                    message: Some("Not now.".into()),
                                },
                            )
                            .unwrap();
                        wait_status(&rx, &plan.id, SessionStatus::Idle).await;
                    }
                    break;
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            println!(
                "plan: file written={}",
                rx.ctx.cwd.join("planned.txt").exists()
            );
            assert!(
                !rx.ctx.cwd.join("planned.txt").exists(),
                "plan mode changes nothing"
            );

            // Changing the mode of a running chat takes effect without a restart.
            let live = rx.manager.create("w1", haiku()).unwrap();
            let live_view = Arc::new(View::default());
            rx.manager.attach(&live.id, live_view.clone()).unwrap();
            rx.manager
                .send(&rx.ctx, &live.id, "Reply with only the word: hi")
                .await
                .unwrap();
            wait_status(&rx, &live.id, SessionStatus::Idle).await;
            let pid_before = rx.manager.live(&live.id).unwrap().process.pid();
            rx.manager
                .update_options(&live.id, None, None, Some(PermissionMode::AcceptEdits))
                .await
                .unwrap();
            rx.manager.send(&rx.ctx, &live.id, "Use the Write tool to create a file named accepted.txt containing exactly: a. Then reply done.").await.unwrap();
            wait_status(&rx, &live.id, SessionStatus::Idle).await;
            println!(
                "acceptEdits switched live: asked={} file={} same process={}",
                !live_view.permissions().is_empty(),
                rx.ctx.cwd.join("accepted.txt").exists(),
                rx.manager.live(&live.id).unwrap().process.pid() == pid_before
            );
            assert_eq!(
                rx.manager.live(&live.id).unwrap().process.pid(),
                pid_before,
                "no restart"
            );
            assert!(
                live_view.permissions().is_empty(),
                "edits go through after the switch"
            );
            assert!(rx.ctx.cwd.join("accepted.txt").exists());
            let _ = &rx.dir;
            rx.manager.stop_workspace("w1").await;
        }
    }

    #[test]
    fn unused_helpers_stay_honest() {
        let _ = Path::new("/");
    }
}
