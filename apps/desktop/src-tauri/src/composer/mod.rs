//! Composers: manager agents that use Circles' own tools to run projects, workspaces and agents.
//!
//! A Composer's runs are chats in a hidden scratch workspace, so every chat feature works for
//! them. Claude Code is started in that chat with no built-in tools, and with Circles' tools
//! through a small MCP helper. Every tool call comes back to the app and is checked there.

pub mod awake;
pub mod dispatch;
pub mod launch;
pub mod mcp;
pub mod prompt;
pub mod runtime;
pub mod server;
pub mod shared;
pub mod store;
pub mod tray;
pub mod triggers;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tauri::{AppHandle, Emitter};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::integrations::Integrations;
use crate::logging::Redactor;
use crate::tools::audit::AuditStore;
use dispatch::{ComposerEvents, Dispatcher};
use server::ServerInfo;
use store::ComposerStore;

/// Tells the UI, as `composer:approvals-changed`, that approvals changed.
struct TauriEvents(AppHandle);

impl ComposerEvents for TauriEvents {
    fn approval_requested(&self, composer_id: &str, tool: &str) {
        use tauri::Manager;
        let state = self.0.state::<crate::state::AppState>();
        let settings = crate::commands::app::settings_layers(&state, None)
            .ok()
            .and_then(|layers| crate::settings::load_merged(&layers).ok())
            .map(|payload| payload.settings)
            .unwrap_or_default();
        if !settings.composers.notify_waiting.unwrap_or(true) {
            return;
        }
        // Someone looking at the window sees the card already.
        let focused = self
            .0
            .get_webview_window("main")
            .is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false));
        if focused {
            return;
        }
        let name = state
            .composers
            .store
            .get(composer_id)
            .map(|c| c.name)
            .unwrap_or_else(|_| "A Composer".into());
        let what = tool.replace('_', " ");
        let _ = crate::system::notify(&name, &format!("Wants to {what}. It is waiting for you."));
        if settings.general.sound.unwrap_or(true) {
            let _ = crate::system::play_sound();
        }
        tray::refresh(&self.0);
    }

    fn approvals_changed(&self, composer_id: &str, session_id: &str) {
        tray::refresh(&self.0);
        let _ = self.0.emit(
            "composer:approvals-changed",
            serde_json::json!({ "composer_id": composer_id, "session_id": session_id }),
        );
    }
}

/// Tells the UI, as `genui:changed`, that a card was shown, answered or closed, and refreshes the
/// views that count what waits for a person.
pub(crate) struct TauriCardEvents(pub AppHandle);

impl crate::genui::service::CardEvents for TauriCardEvents {
    fn cards_changed(&self, session_id: &str, composer_id: Option<&str>) {
        tray::refresh(&self.0);
        let _ = self.0.emit(
            "genui:changed",
            serde_json::json!({ "session_id": session_id, "composer_id": composer_id }),
        );
        // The counts of what waits for the person are the same ones approvals use.
        let _ = self.0.emit(
            "composer:approvals-changed",
            serde_json::json!({ "composer_id": composer_id, "session_id": session_id }),
        );
    }

    fn card_requested(&self, composer_id: Option<&str>) {
        use tauri::Manager;
        let state = self.0.state::<crate::state::AppState>();
        let settings = crate::commands::app::settings_layers(&state, None)
            .ok()
            .and_then(|layers| crate::settings::load_merged(&layers).ok())
            .map(|payload| payload.settings)
            .unwrap_or_default();
        if !settings.composers.notify_waiting.unwrap_or(true) {
            return;
        }
        let focused = self
            .0
            .get_webview_window("main")
            .is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false));
        if focused {
            return;
        }
        let name = composer_id
            .and_then(|id| state.composers.store.get(id).ok())
            .map(|c| c.name)
            .unwrap_or_else(|| "An agent".into());
        let _ = crate::system::notify(&name, "Has a question for you. It is waiting.");
    }
}

pub(crate) struct Started {
    pub(crate) dispatcher: Arc<Dispatcher>,
    pub(crate) server: ServerInfo,
}

/// Everything Composers need, kept in the app state.
pub struct ComposerHub {
    pub store: ComposerStore,
    pub audit: AuditStore,
    pub(crate) started: OnceLock<Started>,
    /// Jira and the like, for the chats that use them.
    pub(crate) integrations: OnceLock<Arc<Integrations>>,
    /// Cards, for the prompt that tells agents they can ask.
    pub(crate) genui: OnceLock<Arc<crate::genui::service::GenuiService>>,
    /// Keeps the Mac awake while a run nobody watches is going.
    pub awake: awake::KeepAwake,
    data_dir: PathBuf,
}

impl ComposerHub {
    pub fn new(db: Arc<Db>, data_dir: &std::path::Path) -> Arc<Self> {
        Arc::new(Self {
            store: ComposerStore::new(db.clone(), data_dir),
            audit: AuditStore::new(db),
            started: OnceLock::new(),
            integrations: OnceLock::new(),
            genui: OnceLock::new(),
            awake: awake::KeepAwake::default(),
            data_dir: data_dir.to_path_buf(),
        })
    }

    /// Opens the tool socket and builds the dispatcher. Done once, when the app has started.
    pub fn start(
        &self,
        app: &AppHandle,
        redactor: Redactor,
        integrations: Arc<Integrations>,
        genui: Arc<crate::genui::service::GenuiService>,
    ) -> AppResult<()> {
        // What waited for an answer before the app stopped is gone.
        match self.store.expire_pending() {
            Ok(0) => {}
            Ok(n) => tracing::info!(approvals = n, "approvals expired by the restart"),
            Err(e) => tracing::warn!(error = %e, "cannot expire old approvals"),
        }
        let dispatcher = Arc::new(Dispatcher::new(
            self.store.clone(),
            self.audit.clone(),
            Arc::new(runtime::AppRuntime { app: app.clone() }),
            Arc::new(TauriEvents(app.clone())),
            redactor,
        ));
        dispatcher.set_integrations(integrations.clone());
        dispatcher.set_genui(genui.clone());
        let _ = self.genui.set(genui);
        let _ = self.integrations.set(integrations);
        let server = tauri::async_runtime::block_on(async {
            server::start(
                dispatcher.clone(),
                &self.data_dir.join("composers").join("run"),
            )
        })?;
        let _ = self.started.set(Started { dispatcher, server });
        Ok(())
    }

    pub fn dispatcher(&self) -> AppResult<Arc<Dispatcher>> {
        self.started
            .get()
            .map(|s| s.dispatcher.clone())
            .ok_or_else(|| AppError::Internal("Composers have not started yet.".into()))
    }

    pub fn server(&self) -> Option<ServerInfo> {
        self.started.get().map(|s| s.server.clone())
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }
}
