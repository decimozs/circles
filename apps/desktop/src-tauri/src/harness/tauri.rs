//! The Tauri side of the agent: a channel that carries a session's stream to the webview, the
//! events that tell the UI something changed, and the link that keeps workspace state in step.

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

use super::from_settings;
use super::parser::RateLimit;
use super::sessions::{
    AgentEvents, AgentNotification, AgentState, SessionChange, StreamItem, StreamSink,
    WorkspaceSync,
};
use crate::state::AppState;

pub const SESSION_CHANGED_EVENT: &str = "session:changed";
pub const NOTIFICATION_EVENT: &str = "agent:notification";
pub const RATE_LIMIT_EVENT: &str = "claude:rate-limit";

/// Sends a session's items to one view over a Tauri channel, as JSON.
pub struct ChannelView(pub Channel<StreamItem>);

impl StreamSink for ChannelView {
    fn item(&self, item: &StreamItem) {
        let _ = self.0.send(item.clone());
    }
}

pub struct TauriAgentEvents(pub AppHandle);

impl AgentEvents for TauriAgentEvents {
    fn session_changed(&self, change: &SessionChange) {
        if let Err(e) = self.0.emit(SESSION_CHANGED_EVENT, change) {
            tracing::warn!(error = %e, "cannot emit {SESSION_CHANGED_EVENT}");
        }
    }

    fn notify(&self, notification: &AgentNotification) {
        if let Err(e) = self.0.emit(NOTIFICATION_EVENT, notification) {
            tracing::warn!(error = %e, "cannot emit {NOTIFICATION_EVENT}");
        }
        // A message and a sound, when the app is in the background and the settings allow them.
        let app = self.0.clone();
        let notification = notification.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let settings = crate::settings::load_merged(&[(
                crate::settings::Scope::User,
                state.paths.settings_file.clone(),
            )])
            .map(|payload| payload.settings)
            .unwrap_or_default();
            let focused = app
                .get_webview_window("main")
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false);
            let plan = from_settings::notify_plan(&settings, focused);
            if plan.notification {
                let title = state
                    .workspace_service()
                    .await
                    .get(&notification.workspace_id)
                    .map(|ws| ws.title.unwrap_or(ws.city))
                    .unwrap_or_else(|_| "Circles".into());
                let body = from_settings::notification_text(notification.kind);
                if let Err(e) = crate::system::notify(&title, body) {
                    tracing::warn!(error = %e, "cannot show a notification");
                }
            }
            if plan.sound {
                if let Err(e) = crate::system::play_sound() {
                    tracing::warn!(error = %e, "cannot play the notification sound");
                }
            }
        });
    }

    fn rate_limit(&self, limit: &RateLimit) {
        if let Err(e) = self.0.emit(RATE_LIMIT_EVENT, limit) {
            tracing::warn!(error = %e, "cannot emit {RATE_LIMIT_EVENT}");
        }
    }
}

/// Moves a workspace between working, waiting and idle as its sessions do.
pub struct TauriWorkspaceSync(pub AppHandle);

impl WorkspaceSync for TauriWorkspaceSync {
    fn agent_state(&self, workspace_id: &str, state: AgentState) {
        let handle = self.0.clone();
        let workspace_id = workspace_id.to_string();
        // Changing workspace state needs the login environment, so it happens off this thread.
        tauri::async_runtime::spawn(async move {
            let app = handle.state::<AppState>();
            let service = app.workspace_service().await;
            if let Err(e) = service.apply_agent_state(&workspace_id, state) {
                tracing::warn!(error = %e, "cannot update the workspace state");
            }
        });
    }
}
