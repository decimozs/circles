//! Terminal tabs of a workspace and the scripts that run in them.
//!
//! Every workspace has a `setup` tab and a `run` tab, plus free `terminal` tabs. A tab is a row
//! in the database. The process behind it is started when needed, keeps running while no view is
//! attached. The output of a setup or run script is saved to disk so its tab still shows what the
//! script printed after a restart. A free terminal starts clean each time: it is a new shell, not
//! a replay of an old one.

pub mod tauri;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::env::ResolvedEnv;
use crate::error::{AppError, AppResult};
use crate::ipc::{new_id, now_ms};
use crate::logging::Redactor;
use crate::paths::AppPaths;
use crate::pty::{ExitInfo, Pty, PtySink, SpawnSpec, STOP_GRACE};
use crate::scripts::{RunMode, RunScript};

const DEFAULT_COLS: u16 = 100;
const DEFAULT_ROWS: u16 = 28;
const PERSIST_EVERY: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    Setup,
    Run,
    Terminal,
}

impl TerminalKind {
    fn as_str(self) -> &'static str {
        match self {
            TerminalKind::Setup => "setup",
            TerminalKind::Run => "run",
            TerminalKind::Terminal => "terminal",
        }
    }

    fn parse(s: &str) -> TerminalKind {
        match s {
            "setup" => TerminalKind::Setup,
            "run" => TerminalKind::Run,
            _ => TerminalKind::Terminal,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Terminal {
    pub id: String,
    pub workspace_id: String,
    pub kind: TerminalKind,
    pub title: String,
    #[ts(type = "number")]
    pub position: i64,
    pub running: bool,
    /// Output exists, live or saved, so the tab is worth showing instead of an empty state.
    pub has_output: bool,
    /// How the last process ended, when it has ended.
    pub exit: Option<ExitInfo>,
    /// Name of the run script that is (or was last) running in a run tab.
    pub script: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct AttachInfo {
    pub running: bool,
}

/// Emitted as `terminal:changed` when a process starts or ends, so lists refresh.
#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct TerminalChange {
    pub terminal_id: String,
    pub workspace_id: String,
    pub running: bool,
}

pub trait TerminalNotifier: Send + Sync {
    fn changed(&self, change: &TerminalChange);
}

pub struct NullNotifier;

impl TerminalNotifier for NullNotifier {
    fn changed(&self, _change: &TerminalChange) {}
}

/// What a terminal needs to know about its workspace.
#[derive(Debug, Clone)]
pub struct Context {
    pub workspace_id: String,
    pub project_id: String,
    pub cwd: PathBuf,
    /// `CIRCLES_*` variables and the custom `[environment_variables]`.
    pub env: Vec<(String, String)>,
}

struct Row {
    id: String,
    workspace_id: String,
    kind: TerminalKind,
    title: String,
    position: i64,
}

pub struct Terminals {
    db: Arc<Db>,
    paths: AppPaths,
    notifier: Arc<dyn TerminalNotifier>,
    live: Mutex<HashMap<String, Arc<Pty>>>,
    /// The view attached to each tab. It outlives any one process, so a script started later
    /// shows up in a tab that was opened earlier.
    sinks: Mutex<HashMap<String, Arc<dyn PtySink>>>,
    /// Name of the script in each run tab.
    scripts: Mutex<HashMap<String, String>>,
    /// Removes known secret values from output before it is kept on disk.
    redactor: Mutex<Option<Redactor>>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

impl Terminals {
    pub fn new(db: Arc<Db>, paths: AppPaths, notifier: Arc<dyn TerminalNotifier>) -> Arc<Self> {
        Arc::new(Self {
            db,
            paths,
            notifier,
            live: Mutex::new(HashMap::new()),
            sinks: Mutex::new(HashMap::new()),
            scripts: Mutex::new(HashMap::new()),
            redactor: Mutex::new(None),
        })
    }

    pub fn set_redactor(&self, redactor: Redactor) {
        *self.redactor.lock().expect("redactor poisoned") = Some(redactor);
    }

    /// The bytes with known secret values removed. Untouched when there is nothing to remove, so
    /// terminal escape sequences are never altered by a lossy round trip.
    fn scrub(&self, bytes: Vec<u8>) -> Vec<u8> {
        let Some(redactor) = self.redactor.lock().expect("redactor poisoned").clone() else {
            return bytes;
        };
        let text = String::from_utf8_lossy(&bytes);
        let clean = redactor.redact(&text);
        if clean == text {
            bytes
        } else {
            clean.into_bytes()
        }
    }

    fn scrollback_file(&self, id: &str) -> PathBuf {
        self.paths
            .data_dir
            .join("scrollback")
            .join(format!("{id}.log"))
    }

    fn live_pty(&self, id: &str) -> Option<Arc<Pty>> {
        self.live
            .lock()
            .expect("terminals poisoned")
            .get(id)
            .cloned()
    }

    fn sink_for(&self, id: &str) -> Option<Arc<dyn PtySink>> {
        self.sinks
            .lock()
            .expect("terminals poisoned")
            .get(id)
            .cloned()
    }

    // Rows

    fn row(&self, id: &str) -> AppResult<Row> {
        self.db
            .read(|c| {
                c.query_row(
                    "SELECT id, workspace_id, kind, title, position FROM terminals WHERE id = ?1",
                    params![id],
                    |r| {
                        Ok(Row {
                            id: r.get(0)?,
                            workspace_id: r.get(1)?,
                            kind: TerminalKind::parse(&r.get::<_, String>(2)?),
                            title: r.get(3)?,
                            position: r.get(4)?,
                        })
                    },
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("terminal {id}"))
                }
                other => other,
            })
    }

    fn to_terminal(&self, row: Row) -> Terminal {
        let pty = self.live_pty(&row.id);
        let running = pty.as_ref().is_some_and(|p| p.is_running());
        let has_output = pty.is_some() || self.scrollback_file(&row.id).exists();
        let script = self
            .scripts
            .lock()
            .expect("terminals poisoned")
            .get(&row.id)
            .cloned();
        Terminal {
            exit: pty.as_ref().and_then(|p| p.exit_info()),
            id: row.id,
            workspace_id: row.workspace_id,
            kind: row.kind,
            title: row.title,
            position: row.position,
            running,
            has_output,
            script,
        }
    }

    pub fn get(&self, id: &str) -> AppResult<Terminal> {
        self.row(id).map(|row| self.to_terminal(row))
    }

    pub fn workspace_of(&self, id: &str) -> AppResult<String> {
        self.row(id).map(|r| r.workspace_id)
    }

    /// Makes sure the workspace has its setup and run tabs. A workspace seen for the first time
    /// also gets one free terminal tab, so there is a shell ready to use.
    fn ensure_defaults(&self, workspace_id: &str) -> AppResult<()> {
        self.db.write(|c| {
            let existing: i64 = c.query_row(
                "SELECT COUNT(*) FROM terminals WHERE workspace_id = ?1",
                params![workspace_id],
                |r| r.get(0),
            )?;
            for (position, (kind, title)) in [("setup", "Setup"), ("run", "Run")]
                .into_iter()
                .enumerate()
            {
                c.execute(
                    "INSERT OR IGNORE INTO terminals (id, workspace_id, kind, title, position, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![new_id(), workspace_id, kind, title, position as i64, now_ms()],
                )?;
            }
            if existing == 0 {
                c.execute(
                    "INSERT INTO terminals (id, workspace_id, kind, title, position, created_at)
                     VALUES (?1, ?2, 'terminal', 'Terminal', 2, ?3)",
                    params![new_id(), workspace_id, now_ms()],
                )?;
            }
            Ok(())
        })
    }

    pub fn list(&self, workspace_id: &str) -> AppResult<Vec<Terminal>> {
        self.ensure_defaults(workspace_id)?;
        let rows = self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, workspace_id, kind, title, position FROM terminals
                 WHERE workspace_id = ?1 ORDER BY position, rowid",
            )?;
            let rows = stmt.query_map(params![workspace_id], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    kind: TerminalKind::parse(&r.get::<_, String>(2)?),
                    title: r.get(3)?,
                    position: r.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })?;
        Ok(rows.into_iter().map(|row| self.to_terminal(row)).collect())
    }

    fn tab_of_kind(&self, workspace_id: &str, kind: TerminalKind) -> AppResult<Terminal> {
        self.list(workspace_id)?
            .into_iter()
            .find(|t| t.kind == kind)
            .ok_or_else(|| {
                AppError::Internal("a workspace is missing its default terminal tabs".into())
            })
    }

    /// A new free terminal tab.
    pub fn create(&self, workspace_id: &str, title: Option<&str>) -> AppResult<Terminal> {
        self.ensure_defaults(workspace_id)?;
        let id = new_id();
        let title = title
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        self.db.write(|c| {
            let existing: i64 = c.query_row(
                "SELECT COUNT(*) FROM terminals WHERE workspace_id = ?1 AND kind = 'terminal'",
                params![workspace_id],
                |r| r.get(0),
            )?;
            let next: i64 = c.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM terminals WHERE workspace_id = ?1",
                params![workspace_id],
                |r| r.get(0),
            )?;
            let title = title.unwrap_or_else(|| {
                if existing == 0 { "Terminal".to_string() } else { format!("Terminal {}", existing + 1) }
            });
            c.execute(
                "INSERT INTO terminals (id, workspace_id, kind, title, position, created_at) VALUES (?1, ?2, 'terminal', ?3, ?4, ?5)",
                params![id, workspace_id, title, next, now_ms()],
            )
        })?;
        self.get(&id)
    }

    pub fn rename(&self, id: &str, title: &str) -> AppResult<Terminal> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 40 {
            return Err(invalid("Tab names need 1 to 40 characters."));
        }
        self.row(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE terminals SET title = ?1 WHERE id = ?2",
                params![title, id],
            )
        })?;
        self.get(id)
    }

    /// Removes a free terminal tab and ends its shell. Setup and run tabs cannot be removed.
    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let row = self.row(id)?;
        if row.kind != TerminalKind::Terminal {
            return Err(invalid("The Setup and Run tabs cannot be closed."));
        }
        self.stop(id).await;
        self.forget(id);
        self.db
            .write(|c| c.execute("DELETE FROM terminals WHERE id = ?1", params![id]))?;
        Ok(())
    }

    fn forget(&self, id: &str) {
        self.live.lock().expect("terminals poisoned").remove(id);
        self.sinks.lock().expect("terminals poisoned").remove(id);
        self.scripts.lock().expect("terminals poisoned").remove(id);
        let _ = std::fs::remove_file(self.scrollback_file(id));
    }

    // Processes

    fn announce(&self, id: &str, workspace_id: &str, running: bool) {
        self.notifier.changed(&TerminalChange {
            terminal_id: id.to_string(),
            workspace_id: workspace_id.to_string(),
            running,
        });
    }

    fn saved_output(&self, id: &str) -> Vec<u8> {
        std::fs::read(self.scrollback_file(id)).unwrap_or_default()
    }

    fn save(&self, id: &str, pty: &Pty) {
        // A free terminal is not brought back after a restart, so its output is not kept.
        if self.row(id).is_ok_and(|r| r.kind == TerminalKind::Terminal) {
            return;
        }
        let bytes = self.scrub(pty.scrollback());
        let file = self.scrollback_file(id);
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&file, bytes) {
            tracing::warn!(error = %e, "cannot save terminal output");
        }
    }

    fn start(
        self: &Arc<Self>,
        env: &ResolvedEnv,
        row: &Row,
        ctx: &Context,
        command: Option<String>,
        kind: &str,
        initial_output: Vec<u8>,
    ) -> AppResult<Arc<Pty>> {
        let pty = Pty::spawn(
            env,
            Some(self.db.clone()),
            SpawnSpec {
                cwd: ctx.cwd.clone(),
                command,
                env: ctx.env.clone(),
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
                kind: kind.to_string(),
                owner_id: Some(row.workspace_id.clone()),
                initial_output,
            },
            None,
        )?;
        self.live
            .lock()
            .expect("terminals poisoned")
            .insert(row.id.clone(), pty.clone());
        self.announce(&row.id, &row.workspace_id, true);

        // When it ends: keep the output, tell the views, tell the lists.
        let this = self.clone();
        let (id, workspace_id, watched) = (row.id.clone(), row.workspace_id.clone(), pty.clone());
        tokio::spawn(async move {
            watched.wait_exit().await;
            this.save(&id, &watched);
            this.announce(&id, &workspace_id, false);
        });

        if let Some(sink) = self.sink_for(&row.id) {
            pty.attach(sink);
        }
        Ok(pty)
    }

    /// Connects a view to a tab. A free terminal starts a new, clean shell if none is running. A
    /// setup or run tab shows what its script last printed.
    pub fn attach(
        self: &Arc<Self>,
        env: &ResolvedEnv,
        ctx: &Context,
        id: &str,
        cols: u16,
        rows: u16,
        sink: Arc<dyn PtySink>,
    ) -> AppResult<AttachInfo> {
        let row = self.row(id)?;
        self.sinks
            .lock()
            .expect("terminals poisoned")
            .insert(id.to_string(), sink.clone());

        if let Some(pty) = self.live_pty(id) {
            let _ = pty.resize(cols, rows);
            pty.attach(sink);
            return Ok(AttachInfo {
                running: pty.is_running(),
            });
        }

        if row.kind != TerminalKind::Terminal {
            // Setup and run tabs only show what a script left behind.
            let saved = self.saved_output(id);
            if !saved.is_empty() {
                sink.data(saved);
            }
            return Ok(AttachInfo { running: false });
        }

        // Output kept by an earlier version is dropped: the shell starts clean.
        let _ = std::fs::remove_file(self.scrollback_file(id));
        let pty = self.start(env, &row, ctx, None, "pty", Vec::new())?;
        let _ = pty.resize(cols, rows);
        Ok(AttachInfo { running: true })
    }

    /// Forgets the shell of a free terminal that ended, so the next view starts a new, clean one in
    /// the same tab. Does nothing to a script tab, which keeps showing what the script printed.
    pub async fn restart_shell(&self, id: &str) -> AppResult<()> {
        let row = self.row(id)?;
        if row.kind != TerminalKind::Terminal {
            return Err(invalid("Only a terminal can be restarted this way."));
        }
        self.stop(id).await;
        self.live.lock().expect("terminals poisoned").remove(id);
        let _ = std::fs::remove_file(self.scrollback_file(id));
        self.announce(&row.id, &row.workspace_id, false);
        Ok(())
    }

    /// Disconnects the view. The process keeps running.
    pub fn detach(&self, id: &str) {
        self.sinks.lock().expect("terminals poisoned").remove(id);
        if let Some(pty) = self.live_pty(id) {
            pty.detach();
            self.save(id, &pty);
        }
    }

    pub fn write(&self, id: &str, data: &[u8]) -> AppResult<()> {
        self.live_pty(id)
            .ok_or_else(|| invalid("Nothing is running in this terminal."))?
            .write(data)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        match self.live_pty(id) {
            Some(pty) => pty.resize(cols, rows),
            None => Ok(()),
        }
    }

    /// Ends the process of a tab, keeps the tab and its output.
    pub async fn stop(&self, id: &str) -> Option<ExitInfo> {
        let pty = self.live_pty(id)?;
        let info = pty.stop(STOP_GRACE).await;
        self.save(id, &pty);
        info
    }

    /// Ends every process of a workspace, for archiving and deleting.
    pub async fn stop_workspace(&self, workspace_id: &str) {
        let ids: Vec<String> = self
            .list_ids(workspace_id)
            .into_iter()
            .filter(|id| self.live_pty(id).is_some())
            .collect();
        for id in ids {
            self.stop(&id).await;
            self.live.lock().expect("terminals poisoned").remove(&id);
        }
    }

    fn list_ids(&self, workspace_id: &str) -> Vec<String> {
        self.db
            .read(|c| {
                let mut stmt = c.prepare("SELECT id FROM terminals WHERE workspace_id = ?1")?;
                let rows = stmt.query_map(params![workspace_id], |r| r.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
    }

    /// Removes the saved output of a deleted workspace's tabs.
    pub fn forget_workspace(&self, workspace_id: &str) {
        for id in self.list_ids(workspace_id) {
            self.forget(&id);
        }
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.live_pty(id).is_some_and(|p| p.is_running())
    }

    /// Processes that are running right now, for the quit confirmation.
    pub fn running_count(&self) -> usize {
        self.live
            .lock()
            .expect("terminals poisoned")
            .values()
            .filter(|p| p.is_running())
            .count()
    }

    /// Ends everything. Used when the app quits.
    pub fn kill_all(&self) {
        let ptys: Vec<(String, Arc<Pty>)> = self
            .live
            .lock()
            .expect("terminals poisoned")
            .iter()
            .map(|(id, pty)| (id.clone(), pty.clone()))
            .collect();
        for (id, pty) in &ptys {
            self.save(id, pty);
            pty.kill_now();
        }
    }

    /// Saves the output of every running terminal. Called on a timer, so a crash loses little.
    pub fn persist_all(&self) {
        let ptys: Vec<(String, Arc<Pty>)> = self
            .live
            .lock()
            .expect("terminals poisoned")
            .iter()
            .map(|(id, pty)| (id.clone(), pty.clone()))
            .collect();
        for (id, pty) in ptys {
            self.save(&id, &pty);
        }
    }

    /// Starts the periodic save. Runs for the life of the app.
    ///
    /// Called while the app starts, on the main thread, where no Tokio runtime is current, so it
    /// uses Tauri's own runtime instead of `tokio::spawn`.
    pub fn start_persisting(self: &Arc<Self>) {
        let this = self.clone();
        ::tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(PERSIST_EVERY);
            loop {
                tick.tick().await;
                this.persist_all();
            }
        });
    }

    // Scripts

    /// Runs a run script in the workspace's Run tab, replacing whatever ran there. With
    /// `nonconcurrent`, the run scripts of the project's other workspaces stop first.
    pub async fn run_script(
        self: &Arc<Self>,
        env: &ResolvedEnv,
        ctx: &Context,
        script: &RunScript,
        mode: RunMode,
    ) -> AppResult<Terminal> {
        let tab = self.tab_of_kind(&ctx.workspace_id, TerminalKind::Run)?;
        let row = self.row(&tab.id)?;

        if mode == RunMode::Nonconcurrent {
            for other in self.run_tabs_of_project(&ctx.project_id, &ctx.workspace_id) {
                self.stop(&other).await;
            }
        }
        self.stop(&tab.id).await;

        self.scripts
            .lock()
            .expect("terminals poisoned")
            .insert(tab.id.clone(), script.name.clone());
        if let Some(sink) = self.sink_for(&tab.id) {
            sink.reset();
        }
        let header = format!("\x1b[2m$ {}\x1b[0m\r\n", script.command);
        self.start(
            env,
            &row,
            ctx,
            Some(script.command.clone()),
            "run",
            header.into_bytes(),
        )?;
        self.get(&tab.id)
    }

    fn run_tabs_of_project(&self, project_id: &str, except_workspace: &str) -> Vec<String> {
        self.db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT t.id FROM terminals t JOIN workspaces w ON w.id = t.workspace_id
                     WHERE t.kind = 'run' AND w.project_id = ?1 AND t.workspace_id != ?2",
                )?;
                let rows = stmt.query_map(params![project_id, except_workspace], |r| {
                    r.get::<_, String>(0)
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|id| self.is_running(id))
            .collect()
    }

    pub async fn stop_script(&self, workspace_id: &str) -> AppResult<Terminal> {
        let tab = self.tab_of_kind(workspace_id, TerminalKind::Run)?;
        self.stop(&tab.id).await;
        self.get(&tab.id)
    }

    /// Runs the setup script in the Setup tab and returns the process, so the caller can wait
    /// for it to end.
    pub async fn run_setup(
        self: &Arc<Self>,
        env: &ResolvedEnv,
        ctx: &Context,
        command: &str,
    ) -> AppResult<Arc<Pty>> {
        let tab = self.tab_of_kind(&ctx.workspace_id, TerminalKind::Setup)?;
        let row = self.row(&tab.id)?;
        self.stop(&tab.id).await;
        if let Some(sink) = self.sink_for(&tab.id) {
            sink.reset();
        }
        let header = format!("\x1b[2m$ {command}\x1b[0m\r\n");
        self.start(
            env,
            &row,
            ctx,
            Some(command.to_string()),
            "setup",
            header.into_bytes(),
        )
    }

    /// Runs a command to completion without a tab, for the archive script. Ends it if it takes
    /// longer than `limit`.
    pub async fn run_to_completion(
        &self,
        env: &ResolvedEnv,
        ctx: &Context,
        command: &str,
        limit: Duration,
    ) -> AppResult<(ExitInfo, String)> {
        let pty = Pty::spawn(
            env,
            Some(self.db.clone()),
            SpawnSpec {
                cwd: ctx.cwd.clone(),
                command: Some(command.to_string()),
                env: ctx.env.clone(),
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
                kind: "archive".into(),
                owner_id: Some(ctx.workspace_id.clone()),
                initial_output: Vec::new(),
            },
            None,
        )?;
        let finished = tokio::time::timeout(limit, pty.wait_exit()).await;
        let info = match finished {
            Ok(info) => info,
            Err(_) => {
                pty.stop(Duration::from_secs(2)).await;
                return Err(AppError::Internal(format!(
                    "it took longer than {} seconds",
                    limit.as_secs()
                )));
            }
        };
        let tail = String::from_utf8_lossy(&pty.scrollback()).into_owned();
        Ok((info, tail))
    }

    /// The saved output of a tab, for tests and diagnostics.
    pub fn saved_len(&self, id: &str) -> usize {
        self.saved_output(id).len()
    }

    pub fn has_row(&self, id: &str) -> bool {
        self.db
            .read(|c| {
                c.query_row("SELECT 1 FROM terminals WHERE id = ?1", params![id], |_| {
                    Ok(())
                })
                .optional()
            })
            .ok()
            .flatten()
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::CollectPtySink;
    use std::sync::Mutex as StdMutex;
    use std::time::Instant;

    #[derive(Default)]
    struct Changes(StdMutex<Vec<TerminalChange>>);

    impl TerminalNotifier for Changes {
        fn changed(&self, change: &TerminalChange) {
            self.0.lock().unwrap().push(change.clone());
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        db: Arc<Db>,
        terminals: Arc<Terminals>,
        changes: Arc<Changes>,
        env: ResolvedEnv,
        cwd: PathBuf,
    }

    fn fixture() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let paths = AppPaths::from_roots(&root.join("data"), &root.join("home"));
        paths.ensure().unwrap();
        let db = Arc::new(Db::open(&paths.db_file).unwrap());
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES ('p1','roster','/r','main',NULL,1,1);
                 INSERT INTO projects VALUES ('p2','other','/o','main',NULL,1,1);
                 INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                   VALUES ('w1','p1','Algiers','b1','main','/w1','ready',1,1),
                          ('w2','p1','Lima','b2','main','/w2','ready',1,1),
                          ('w3','p2','Oslo','b3','main','/w3','ready',1,1);",
            )
        })
        .unwrap();
        let changes = Arc::new(Changes::default());
        let terminals = Terminals::new(db.clone(), paths, changes.clone());
        let path = std::env::var("PATH").unwrap_or_default();
        let env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("SHELL", "/bin/sh"),
            ("HOME", "/tmp"),
        ]);
        Fx {
            _tmp: tmp,
            db,
            terminals,
            changes,
            env,
            cwd: root,
        }
    }

    fn ctx(fx: &Fx, workspace: &str, project: &str) -> Context {
        Context {
            workspace_id: workspace.into(),
            project_id: project.into(),
            cwd: fx.cwd.clone(),
            env: vec![("CIRCLES_PORT".into(), "55010".into())],
        }
    }

    fn script(name: &str, command: &str) -> RunScript {
        RunScript {
            name: name.into(),
            command: command.into(),
            default: true,
            icon: None,
        }
    }

    async fn until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn tab(fx: &Fx, workspace: &str, kind: TerminalKind) -> Terminal {
        fx.terminals
            .list(workspace)
            .unwrap()
            .into_iter()
            .find(|t| t.kind == kind)
            .unwrap()
    }

    /// Regression test: the app starts this from its setup hook, which has no Tokio runtime.
    /// Using `tokio::spawn` there aborted the whole app on launch.
    #[test]
    fn the_saver_starts_without_a_tokio_runtime() {
        assert!(
            tokio::runtime::Handle::try_current().is_err(),
            "this test must run outside a runtime"
        );
        let fx = fixture();
        fx.terminals.start_persisting();
    }

    #[test]
    fn saved_output_has_known_secrets_removed_and_nothing_else_touched() {
        let fx = fixture();
        let plain = b"\x1b[31mred\x1b[0m \xff\xfe raw bytes".to_vec();
        assert_eq!(fx.terminals.scrub(plain.clone()), plain, "no redactor set");

        let redactor = Redactor::default();
        redactor.set_values(vec!["hunter2-token".into()]);
        fx.terminals.set_redactor(redactor);
        assert_eq!(
            fx.terminals.scrub(plain.clone()),
            plain,
            "nothing to remove leaves the bytes exactly as they were"
        );
        let out = fx
            .terminals
            .scrub(b"export TOKEN=hunter2-token done".to_vec());
        assert_eq!(out, b"export TOKEN=[redacted] done".to_vec());
    }

    #[tokio::test]
    async fn every_workspace_has_a_setup_a_run_and_a_terminal_tab() {
        let fx = fixture();
        let tabs = fx.terminals.list("w1").unwrap();
        assert_eq!(
            tabs.iter()
                .map(|t| (t.kind, t.title.as_str()))
                .collect::<Vec<_>>(),
            [
                (TerminalKind::Setup, "Setup"),
                (TerminalKind::Run, "Run"),
                (TerminalKind::Terminal, "Terminal"),
            ]
        );
        assert!(tabs
            .iter()
            .all(|t| !t.running && !t.has_output && t.exit.is_none()));
        // Listing again changes nothing.
        let again = fx.terminals.list("w1").unwrap();
        assert_eq!(
            again.iter().map(|t| &t.id).collect::<Vec<_>>(),
            tabs.iter().map(|t| &t.id).collect::<Vec<_>>()
        );
        assert_eq!(fx.terminals.list("w2").unwrap().len(), 3);
    }

    #[tokio::test]
    async fn free_terminals_can_be_added_renamed_and_closed() {
        let fx = fixture();
        let a = fx.terminals.create("w1", None).unwrap();
        let b = fx.terminals.create("w1", None).unwrap();
        let named = fx.terminals.create("w1", Some("  logs  ")).unwrap();
        assert_eq!(
            (a.title.as_str(), b.title.as_str(), named.title.as_str()),
            // A new workspace already has one "Terminal" tab, so the next ones are numbered on.
            ("Terminal 2", "Terminal 3", "logs")
        );
        assert_eq!(fx.terminals.list("w1").unwrap().len(), 6);
        assert!(a.position < b.position && b.position < named.position);

        assert_eq!(fx.terminals.rename(&a.id, "build").unwrap().title, "build");
        assert_eq!(
            fx.terminals.rename(&a.id, "   ").unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            fx.terminals
                .rename(&a.id, &"x".repeat(41))
                .unwrap_err()
                .code(),
            "invalid"
        );

        fx.terminals.delete(&b.id).await.unwrap();
        assert_eq!(fx.terminals.list("w1").unwrap().len(), 5);
        assert_eq!(fx.terminals.get(&b.id).unwrap_err().code(), "not_found");

        let setup = tab(&fx, "w1", TerminalKind::Setup);
        assert_eq!(
            fx.terminals.delete(&setup.id).await.unwrap_err().code(),
            "invalid"
        );
    }

    #[tokio::test]
    async fn opening_a_free_terminal_starts_a_shell_in_the_workspace() {
        let fx = fixture();
        let t = fx.terminals.create("w1", None).unwrap();
        let sink = CollectPtySink::new();
        let info = fx
            .terminals
            .attach(&fx.env, &ctx(&fx, "w1", "p1"), &t.id, 90, 30, sink.clone())
            .unwrap();
        assert_eq!(info, AttachInfo { running: true });

        fx.terminals
            .write(&t.id, b"echo here=$(pwd) port=$CIRCLES_PORT\n")
            .unwrap();
        until("the shell to answer", || sink.text().contains("port=55010")).await;
        assert!(
            sink.text().contains(&format!("here={}", fx.cwd.display())),
            "{}",
            sink.text()
        );
        assert!(fx.terminals.is_running(&t.id));
        assert_eq!(fx.terminals.running_count(), 1);
        assert!(fx.terminals.get(&t.id).unwrap().running);
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn reopening_a_tab_reuses_the_running_shell_and_replays_its_output() {
        let fx = fixture();
        let t = fx.terminals.create("w1", None).unwrap();
        let first = CollectPtySink::new();
        fx.terminals
            .attach(&fx.env, &ctx(&fx, "w1", "p1"), &t.id, 80, 24, first.clone())
            .unwrap();
        fx.terminals
            .write(&t.id, b"export KEPT=alive; echo marker-one\n")
            .unwrap();
        until("first output", || first.text().contains("marker-one")).await;

        // The user switches away and comes back.
        fx.terminals.detach(&t.id);
        let second = CollectPtySink::new();
        let info = fx
            .terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &t.id,
                80,
                24,
                second.clone(),
            )
            .unwrap();
        assert_eq!(info, AttachInfo { running: true });
        assert!(second.text().contains("marker-one"), "history is replayed");

        // Same shell: the variable is still set.
        fx.terminals.write(&t.id, b"echo kept=$KEPT\n").unwrap();
        until("same shell", || second.text().contains("kept=alive")).await;
        assert_eq!(fx.terminals.running_count(), 1);
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn a_free_terminal_starts_clean_each_time_with_no_old_output_and_no_banner() {
        let fx = fixture();
        let t = fx.terminals.create("w1", None).unwrap();
        let first = CollectPtySink::new();
        fx.terminals
            .attach(&fx.env, &ctx(&fx, "w1", "p1"), &t.id, 80, 24, first.clone())
            .unwrap();
        fx.terminals.write(&t.id, b"echo remember-me\n").unwrap();
        until("output", || first.text().contains("remember-me")).await;
        fx.terminals.detach(&t.id);
        fx.terminals.stop(&t.id).await;
        assert_eq!(
            fx.terminals.saved_len(&t.id),
            0,
            "its output is not kept on disk"
        );
        // Simulate a restart: nothing is alive any more, and an old version left output behind.
        fx.terminals.live.lock().unwrap().clear();
        std::fs::create_dir_all(fx.terminals.scrollback_file(&t.id).parent().unwrap()).unwrap();
        std::fs::write(
            fx.terminals.scrollback_file(&t.id),
            b"left by an older version\n",
        )
        .unwrap();

        let second = CollectPtySink::new();
        let info = fx
            .terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &t.id,
                80,
                24,
                second.clone(),
            )
            .unwrap();
        assert_eq!(info, AttachInfo { running: true });
        fx.terminals.write(&t.id, b"echo second-session\n").unwrap();
        until("the new shell", || second.text().contains("second-session")).await;
        let text = second.text();
        assert!(!text.contains("remember-me"), "{text}");
        assert!(!text.contains("older version"), "{text}");
        assert!(!text.contains("Previous terminal session"), "{text}");
        assert_eq!(fx.terminals.saved_len(&t.id), 0, "the stale file is gone");
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn a_shell_that_ended_is_replaced_by_a_clean_one_in_the_same_tab() {
        let fx = fixture();
        let t = fx.terminals.create("w1", None).unwrap();
        let first = CollectPtySink::new();
        fx.terminals
            .attach(&fx.env, &ctx(&fx, "w1", "p1"), &t.id, 80, 24, first.clone())
            .unwrap();
        fx.terminals
            .write(&t.id, b"echo before-exit\nexit\n")
            .unwrap();
        until("the shell ended", || {
            fx.terminals.get(&t.id).unwrap().exit.is_some()
        })
        .await;
        assert!(!fx.terminals.get(&t.id).unwrap().running);

        fx.terminals.restart_shell(&t.id).await.unwrap();
        assert!(
            fx.terminals.get(&t.id).unwrap().exit.is_none(),
            "the old exit is forgotten"
        );
        assert_eq!(
            fx.terminals.running_count(),
            0,
            "nothing runs until a view attaches"
        );

        let second = CollectPtySink::new();
        let info = fx
            .terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &t.id,
                80,
                24,
                second.clone(),
            )
            .unwrap();
        assert_eq!(info, AttachInfo { running: true });
        fx.terminals.write(&t.id, b"echo fresh-shell\n").unwrap();
        until("the new shell", || second.text().contains("fresh-shell")).await;
        assert!(
            !second.text().contains("before-exit"),
            "the screen starts clean"
        );
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn only_a_free_terminal_can_be_restarted() {
        let fx = fixture();
        let tabs = fx.terminals.list("w1").unwrap();
        let setup = tabs.iter().find(|t| t.kind == TerminalKind::Setup).unwrap();
        assert_eq!(
            fx.terminals
                .restart_shell(&setup.id)
                .await
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            fx.terminals.restart_shell("nope").await.unwrap_err().code(),
            "not_found"
        );
    }

    #[tokio::test]
    async fn setup_and_run_tabs_only_show_what_scripts_left_behind() {
        let fx = fixture();
        let run = tab(&fx, "w1", TerminalKind::Run);
        let sink = CollectPtySink::new();
        let info = fx
            .terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &run.id,
                80,
                24,
                sink.clone(),
            )
            .unwrap();
        assert_eq!(info, AttachInfo { running: false });
        assert!(sink.text().is_empty());
        assert_eq!(
            fx.terminals.running_count(),
            0,
            "no shell is started for a run tab"
        );
        assert_eq!(
            fx.terminals.write(&run.id, b"x").unwrap_err().code(),
            "invalid"
        );
    }

    #[tokio::test]
    async fn a_run_script_shows_up_in_a_tab_that_was_opened_earlier() {
        let fx = fixture();
        let run = tab(&fx, "w1", TerminalKind::Run);
        let sink = CollectPtySink::new();
        fx.terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &run.id,
                80,
                24,
                sink.clone(),
            )
            .unwrap();

        let started = fx
            .terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("web", "echo serving on $CIRCLES_PORT; sleep 0.3"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        assert!(started.running);
        assert_eq!(started.script.as_deref(), Some("web"));
        until("script output", || sink.text().contains("serving on 55010")).await;
        assert!(
            sink.text().contains("$ echo serving on $CIRCLES_PORT"),
            "the command is shown first"
        );
        assert_eq!(
            sink.resets(),
            1,
            "the view was cleared when the script started"
        );

        until("exit", || sink.exit().is_some()).await;
        assert_eq!(sink.exit().unwrap().code, Some(0));
        until("the list to notice", || {
            !fx.terminals.get(&run.id).unwrap().running
        })
        .await;
        let after = fx.terminals.get(&run.id).unwrap();
        assert!(after.has_output);
        assert_eq!(after.exit.unwrap().code, Some(0));
        assert!(
            fx.terminals.saved_len(&run.id) > 0,
            "the output is saved when the script ends"
        );
    }

    #[tokio::test]
    async fn starting_a_script_again_replaces_the_one_that_is_running() {
        let fx = fixture();
        let run = tab(&fx, "w1", TerminalKind::Run);
        let sink = CollectPtySink::new();
        fx.terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &run.id,
                80,
                24,
                sink.clone(),
            )
            .unwrap();

        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("a", "echo first-server; sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        until("first output", || sink.text().contains("first-server")).await;
        let first_pid = fx.terminals.live_pty(&run.id).unwrap().pid();

        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("b", "echo second-server; sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        until("second output", || sink.text().contains("second-server")).await;
        let pty = fx.terminals.live_pty(&run.id).unwrap();
        assert_ne!(pty.pid(), first_pid);
        assert!(
            unsafe { libc::killpg(first_pid, 0) } != 0,
            "the first script's process group is gone"
        );
        assert!(
            !sink.text().contains("first-server"),
            "the view shows only the new run"
        );
        assert_eq!(
            fx.terminals.get(&run.id).unwrap().script.as_deref(),
            Some("b")
        );
        assert_eq!(fx.terminals.running_count(), 1);
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn a_script_can_be_stopped() {
        let fx = fixture();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("web", "sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        let run = tab(&fx, "w1", TerminalKind::Run);
        assert!(run.running);
        let stopped = fx.terminals.stop_script("w1").await.unwrap();
        assert!(!stopped.running);
        assert!(stopped.exit.is_some());
        assert_eq!(fx.terminals.running_count(), 0);
        // Stopping when nothing runs is harmless.
        fx.terminals.stop_script("w1").await.unwrap();
    }

    #[tokio::test]
    async fn nonconcurrent_scripts_stop_the_projects_other_workspaces_but_not_other_projects() {
        let fx = fixture();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w2", "p1"),
                &script("web", "sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w3", "p2"),
                &script("web", "sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        assert_eq!(fx.terminals.running_count(), 2);

        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("web", "sleep 60"),
                RunMode::Nonconcurrent,
            )
            .await
            .unwrap();
        assert!(
            !tab(&fx, "w2", TerminalKind::Run).running,
            "same project: stopped"
        );
        assert!(
            tab(&fx, "w3", TerminalKind::Run).running,
            "another project: untouched"
        );
        assert!(tab(&fx, "w1", TerminalKind::Run).running);
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn concurrent_scripts_leave_other_workspaces_alone() {
        let fx = fixture();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w2", "p1"),
                &script("web", "sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("web", "sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        assert_eq!(fx.terminals.running_count(), 2);
        fx.terminals.kill_all();
    }

    #[tokio::test]
    async fn the_setup_script_runs_in_the_setup_tab_and_can_be_waited_for() {
        let fx = fixture();
        let sink = CollectPtySink::new();
        let setup = tab(&fx, "w1", TerminalKind::Setup);
        fx.terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &setup.id,
                80,
                24,
                sink.clone(),
            )
            .unwrap();

        let ok = fx
            .terminals
            .run_setup(&fx.env, &ctx(&fx, "w1", "p1"), "echo installing; exit 0")
            .await
            .unwrap();
        assert_eq!(ok.wait_exit().await.code, Some(0));
        assert!(sink.text().contains("installing"));

        let bad = fx
            .terminals
            .run_setup(&fx.env, &ctx(&fx, "w1", "p1"), "echo oops; exit 4")
            .await
            .unwrap();
        assert_eq!(bad.wait_exit().await.code, Some(4));
        until("the second run's output", || sink.text().contains("oops")).await;
        assert!(
            !sink.text().contains("installing"),
            "the tab was cleared for the new run"
        );
    }

    #[tokio::test]
    async fn tabs_announce_when_their_process_starts_and_ends() {
        let fx = fixture();
        let run = tab(&fx, "w1", TerminalKind::Run);
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("web", "true"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        until("the end", || {
            fx.changes.0.lock().unwrap().iter().any(|c| !c.running)
        })
        .await;
        let changes = fx.changes.0.lock().unwrap().clone();
        assert_eq!(
            changes.first().unwrap(),
            &TerminalChange {
                terminal_id: run.id.clone(),
                workspace_id: "w1".into(),
                running: true
            }
        );
        assert!(!changes.last().unwrap().running);
    }

    #[tokio::test]
    async fn script_output_is_saved_regularly_and_forgotten_with_the_workspace() {
        let fx = fixture();
        let tab = fx
            .terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("a", "echo saved-output; sleep 30"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        let free = fx.terminals.create("w1", None).unwrap();
        let sink = CollectPtySink::new();
        fx.terminals
            .attach(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &free.id,
                80,
                24,
                sink.clone(),
            )
            .unwrap();
        fx.terminals.write(&free.id, b"echo free-output\n").unwrap();
        until("free output", || sink.text().contains("free-output")).await;
        assert_eq!(fx.terminals.saved_len(&tab.id), 0, "nothing saved yet");

        until("the script's output is saved", || {
            fx.terminals.persist_all();
            fx.terminals.saved_len(&tab.id) > 0
        })
        .await;
        assert_eq!(
            fx.terminals.saved_len(&free.id),
            0,
            "a free terminal is never saved"
        );

        fx.terminals.stop_workspace("w1").await;
        assert_eq!(fx.terminals.running_count(), 0);
        fx.terminals.forget_workspace("w1");
        assert_eq!(fx.terminals.saved_len(&tab.id), 0, "files are removed");
    }

    #[tokio::test]
    async fn quitting_ends_every_process_group() {
        let fx = fixture();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                &script("a", "sleep 60 & sleep 60 & wait"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        fx.terminals
            .run_script(
                &fx.env,
                &ctx(&fx, "w3", "p2"),
                &script("b", "sleep 60"),
                RunMode::Concurrent,
            )
            .await
            .unwrap();
        let pids: Vec<i32> = ["w1", "w3"]
            .iter()
            .map(|w| {
                fx.terminals
                    .live_pty(&tab(&fx, w, TerminalKind::Run).id)
                    .unwrap()
                    .pid()
            })
            .collect();
        assert_eq!(fx.terminals.running_count(), 2);

        fx.terminals.kill_all();
        until("everything to end", || {
            pids.iter().all(|pid| unsafe { libc::killpg(*pid, 0) } != 0)
        })
        .await;
        until("the exits", || fx.terminals.running_count() == 0).await;
        assert!(
            fx.db
                .read(|c| c.query_row("SELECT COUNT(*) FROM processes", [], |r| r.get::<_, i64>(0)))
                .unwrap()
                <= 2
        );
    }

    #[tokio::test]
    async fn run_to_completion_reports_the_exit_and_the_output() {
        let fx = fixture();
        let (info, output) = fx
            .terminals
            .run_to_completion(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                "echo cleaned up $CIRCLES_PORT; exit 2",
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert_eq!(info.code, Some(2));
        assert!(output.contains("cleaned up 55010"), "{output}");
    }

    #[tokio::test]
    async fn run_to_completion_gives_up_after_the_limit_and_ends_the_process() {
        let fx = fixture();
        let started = Instant::now();
        let err = fx
            .terminals
            .run_to_completion(
                &fx.env,
                &ctx(&fx, "w1", "p1"),
                "sleep 60",
                Duration::from_millis(400),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("took longer"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(8));
    }

    /// Runs terminals and scripts with the developer's real login shell and tools, in a scratch
    /// folder. Run with `cargo test --lib -- --ignored --nocapture real_login_shell`.
    #[tokio::test]
    #[ignore = "uses the developer's real shell and tools"]
    async fn real_login_shell() {
        let fx = fixture();
        let env = crate::env::resolve().await;
        println!("shell: {:?}", env.get("SHELL"));
        let c = ctx(&fx, "w1", "p1");

        // An interactive login shell, like the Terminal tab.
        let t = fx.terminals.create("w1", None).unwrap();
        let sink = CollectPtySink::new();
        fx.terminals
            .attach(&env, &c, &t.id, 120, 30, sink.clone())
            .unwrap();
        fx.terminals
            .write(&t.id, b"echo __START__; which claude gh git node pnpm; echo port=$CIRCLES_PORT; echo __END__$((40+2)); exit\n")
            .unwrap();
        until("the shell to finish", || sink.text().contains("__END__42")).await;
        let text = sink.text();
        let clean: String = text
            .lines()
            .filter(|l| l.contains('/') || l.contains("port=") || l.contains("__"))
            .collect::<Vec<_>>()
            .join("\n");
        println!("--- interactive shell output (filtered)\n{clean}");
        for tool in ["claude", "gh", "git", "node", "pnpm"] {
            assert!(
                text.contains(&format!("/{tool}")),
                "{tool} not found in the terminal's PATH"
            );
        }
        assert!(text.contains("port=55010"));

        // A script, like Run and Setup.
        let run = RunScript {
            name: "versions".into(),
            command: "node --version && pnpm --version && echo done-$CIRCLES_PORT".into(),
            default: true,
            icon: None,
        };
        let rtab = tab(&fx, "w1", TerminalKind::Run);
        let rsink = CollectPtySink::new();
        fx.terminals
            .attach(&env, &c, &rtab.id, 120, 30, rsink.clone())
            .unwrap();
        fx.terminals
            .run_script(&env, &c, &run, RunMode::Concurrent)
            .await
            .unwrap();
        until("the script to finish", || rsink.exit().is_some()).await;
        println!("--- run script output\n{}", rsink.text().trim());
        assert_eq!(rsink.exit().unwrap().code, Some(0));
        assert!(rsink.text().contains("done-55010"));

        // The archive style runner.
        let (info, out) = fx
            .terminals
            .run_to_completion(&env, &c, "git --version", Duration::from_secs(10))
            .await
            .unwrap();
        println!("--- run_to_completion: {:?} {}", info, out.trim());
        assert_eq!(info.code, Some(0));
    }

    #[tokio::test]
    async fn deleting_the_workspace_removes_its_tabs() {
        let fx = fixture();
        fx.terminals.list("w2").unwrap();
        fx.db
            .write(|c| c.execute("DELETE FROM workspaces WHERE id = 'w2'", []))
            .unwrap();
        let remaining: i64 = fx
            .db
            .read(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM terminals WHERE workspace_id = 'w2'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
