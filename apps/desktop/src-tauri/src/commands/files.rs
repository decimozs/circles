use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::files::{self, FileContent, FsEntry};
use crate::git::Git;
use crate::ipc::IpcResult;
use crate::review::changes::{self, ChangeScope, ChangedFile, FilePair};
use crate::review::comments::{compose_message, Comment, CommentStatus, NewComment};
use crate::review::editors::{self, Editor};
use crate::state::AppState;

/// Emitted as `workspace:files-changed` when files in a watched workspace change.
#[derive(Debug, Clone, Serialize)]
struct FilesChanged {
    workspace_id: String,
}

async fn folder_of(state: &AppState, workspace_id: &str) -> Result<std::path::PathBuf, AppError> {
    let (ctx, _) = state.workspace_service().await.context_for(workspace_id)?;
    Ok(ctx.cwd)
}

fn scope_from(text: &str) -> Result<ChangeScope, AppError> {
    ChangeScope::parse(text)
        .ok_or_else(|| AppError::Invalid(format!("\"{text}\" is not a change scope.")))
}

#[tauri::command]
pub async fn fs_list(
    state: State<'_, AppState>,
    workspace_id: String,
    path: Option<String>,
    show_ignored: Option<bool>,
) -> IpcResult<Vec<FsEntry>> {
    let root = folder_of(&state, &workspace_id).await?;
    let path = path.unwrap_or_default();
    let show = show_ignored.unwrap_or(false);
    tokio::task::spawn_blocking(move || files::list_dir(&root, &path, show))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn fs_read(
    state: State<'_, AppState>,
    workspace_id: String,
    path: String,
) -> IpcResult<FileContent> {
    let root = folder_of(&state, &workspace_id).await?;
    tokio::task::spawn_blocking(move || files::read_file(&root, &path))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn fs_write(
    state: State<'_, AppState>,
    workspace_id: String,
    path: String,
    text: String,
) -> IpcResult<()> {
    let root = folder_of(&state, &workspace_id).await?;
    files::write_file(&root, &path, &text)
}

/// Shows a file or folder of the workspace in Finder.
#[tauri::command]
pub async fn fs_reveal(
    state: State<'_, AppState>,
    workspace_id: String,
    path: Option<String>,
) -> IpcResult<()> {
    let root = folder_of(&state, &workspace_id).await?;
    let target = match path.as_deref() {
        Some(p) if !p.is_empty() => files::resolve_inside(&root, p)?,
        _ => root,
    };
    std::process::Command::new("open")
        .arg("-R")
        .arg(target)
        .spawn()?;
    Ok(())
}

#[tauri::command]
pub async fn changes_list(
    state: State<'_, AppState>,
    workspace_id: String,
    scope: String,
) -> IpcResult<Vec<ChangedFile>> {
    let service = state.workspace_service().await;
    let (ctx, _) = service.context_for(&workspace_id)?;
    let base = service.get(&workspace_id)?.base_branch;
    changes::changed_files(
        &Git::new(service.env.clone()),
        &ctx.cwd,
        &base,
        scope_from(&scope)?,
    )
    .await
}

/// The history of the workspace's branch next to its base.
#[tauri::command]
pub async fn git_graph(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<crate::review::graph::GitGraph> {
    let service = state.workspace_service().await;
    let (ctx, _) = service.context_for(&workspace_id)?;
    let base = service.get(&workspace_id)?.base_branch;
    crate::review::graph::graph(&Git::new(service.env.clone()), &ctx.cwd, &base).await
}

#[tauri::command]
pub async fn diff_files(
    state: State<'_, AppState>,
    workspace_id: String,
    scope: String,
    path: String,
    old_path: Option<String>,
) -> IpcResult<FilePair> {
    let service = state.workspace_service().await;
    let (ctx, _) = service.context_for(&workspace_id)?;
    let base = service.get(&workspace_id)?.base_branch;
    changes::file_pair(
        &Git::new(service.env.clone()),
        &ctx.cwd,
        &base,
        scope_from(&scope)?,
        &path,
        old_path.as_deref(),
    )
    .await
}

/// Starts telling the UI when files of the workspace change.
#[tauri::command]
pub async fn files_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<()> {
    let root = folder_of(&state, &workspace_id).await?;
    let id = workspace_id.clone();
    state.watchers.watch(&workspace_id, &root, move || {
        let _ = app.emit(
            "workspace:files-changed",
            FilesChanged {
                workspace_id: id.clone(),
            },
        );
    })
}

#[tauri::command]
pub async fn files_unwatch(state: State<'_, AppState>, workspace_id: String) -> IpcResult<()> {
    state.watchers.unwatch(&workspace_id);
    Ok(())
}

#[tauri::command]
pub async fn editor_list(state: State<'_, AppState>) -> IpcResult<Vec<Editor>> {
    let env = state.env.get().await;
    Ok(editors::detect(env.path(), dirs::home_dir().as_deref()))
}

/// Opens the workspace folder, or one file in it, in an editor.
#[tauri::command]
pub async fn editor_open(
    state: State<'_, AppState>,
    workspace_id: String,
    editor: String,
    path: Option<String>,
) -> IpcResult<()> {
    let root = folder_of(&state, &workspace_id).await?;
    let target = match path.as_deref() {
        Some(p) if !p.is_empty() => files::resolve_inside(&root, p)?,
        _ => root,
    };
    let env = state.env.get().await;
    editors::open(&editor, env.path(), dirs::home_dir().as_deref(), &target)
}

#[tauri::command]
pub async fn comment_list(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Vec<Comment>> {
    state.comments.list(&workspace_id)
}

#[tauri::command]
pub async fn comment_create(
    state: State<'_, AppState>,
    workspace_id: String,
    comment: NewComment,
) -> IpcResult<Comment> {
    // The file must exist in the workspace, so a comment cannot point outside it.
    let root = folder_of(&state, &workspace_id).await?;
    if files::resolve_inside(&root, &comment.path).is_err() && !comment.path.is_empty() {
        let relative = std::path::Path::new(&comment.path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(AppError::Invalid(
                "That path is outside the workspace.".into(),
            ));
        }
    }
    state.comments.create(&workspace_id, comment)
}

#[tauri::command]
pub async fn comment_set_status(
    state: State<'_, AppState>,
    id: String,
    resolved: bool,
) -> IpcResult<Comment> {
    state.comments.set_status(
        &id,
        if resolved {
            CommentStatus::Resolved
        } else {
            CommentStatus::Open
        },
    )
}

#[tauri::command]
pub async fn comment_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.comments.delete(&id)
}

/// Sends comments to a chat as one message, and marks them as sent.
#[tauri::command]
pub async fn comment_send(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
    ids: Vec<String>,
) -> IpcResult<()> {
    let mut chosen = Vec::new();
    for id in &ids {
        let comment = state.comments.get(id)?;
        if comment.workspace_id != workspace_id {
            return Err(AppError::Invalid(
                "That comment belongs to another workspace.".into(),
            ));
        }
        chosen.push(comment);
    }
    if chosen.is_empty() {
        return Err(AppError::Invalid("There are no comments to send.".into()));
    }
    let (ctx, _) = state.workspace_service().await.context_for(&workspace_id)?;
    crate::commands::session::refresh_agent(&state, &workspace_id).await;
    state
        .sessions
        .send(&ctx, &session_id, &compose_message(&chosen))
        .await?;
    state.comments.mark_sent(&ids)
}
