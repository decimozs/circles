use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::commands::settings::check_claude;
use crate::env::ResolvedEnv;
use crate::github::classify;
use crate::ipc::IpcResult;
use crate::state::AppState;

/// One tool the app needs, and whether it is ready to use.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Dependency {
    pub name: String,
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// Found, runs and (where it applies) signed in.
    pub ok: bool,
    pub problem: Option<String>,
    /// A command that fixes the problem.
    pub fix: Option<String>,
}

async fn run(env: &ResolvedEnv, program: &Path, args: &[&str]) -> Option<(bool, String, String)> {
    let mut command = tokio::process::Command::new(program);
    env.apply(&mut command);
    command.args(args).stdin(Stdio::null()).kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .ok()?
        .ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

async fn check_git(env: &ResolvedEnv) -> Dependency {
    let Some(path) = env.which("git") else {
        return Dependency {
            name: "git".into(),
            found: false,
            path: None,
            version: None,
            ok: false,
            problem: Some("git was not found.".into()),
            fix: Some("xcode-select --install".into()),
        };
    };
    let version = run(env, &path, &["--version"])
        .await
        .filter(|(ok, ..)| *ok)
        .and_then(|(_, out, _)| first_line(&out));
    Dependency {
        name: "git".into(),
        found: true,
        path: Some(path.display().to_string()),
        ok: version.is_some(),
        problem: version
            .is_none()
            .then(|| "git was found but did not answer.".into()),
        version,
        fix: None,
    }
}

async fn check_gh(env: &ResolvedEnv) -> Dependency {
    let Some(path) = env.which("gh") else {
        return Dependency {
            name: "gh".into(),
            found: false,
            path: None,
            version: None,
            ok: false,
            problem: Some("The GitHub CLI (gh) is not installed.".into()),
            fix: Some("brew install gh".into()),
        };
    };
    let version = run(env, &path, &["--version"])
        .await
        .and_then(|(_, out, _)| first_line(&out));
    let (ok, problem, fix) = match run(env, &path, &["auth", "status"]).await {
        Some((true, ..)) => (true, None, None),
        Some((false, out, err)) => {
            let e = classify(&format!("{err}\n{out}"));
            (false, Some(e.message.clone()), e.fix.clone())
        }
        None => (false, Some("gh did not answer.".into()), None),
    };
    Dependency {
        name: "gh".into(),
        found: true,
        path: Some(path.display().to_string()),
        version,
        ok,
        problem,
        fix,
    }
}

/// Checks `git`, `gh` and `claude` for the first-run screen and the diagnostics.
pub(crate) async fn dependencies(state: &AppState) -> Vec<Dependency> {
    let env = state.env.get().await;
    let claude = match check_claude(state).await {
        Ok(c) => Dependency {
            name: "claude".into(),
            found: c.found || c.path.is_some(),
            ok: c.found,
            path: c.path,
            version: c.version,
            fix: (!c.found).then(|| "claude".to_string()),
            problem: c.problem,
        },
        Err(e) => Dependency {
            name: "claude".into(),
            found: false,
            path: None,
            version: None,
            ok: false,
            problem: Some(e.to_string()),
            fix: None,
        },
    };
    vec![check_git(&env).await, check_gh(&env).await, claude]
}

#[tauri::command]
pub async fn dependency_check(state: State<'_, AppState>) -> IpcResult<Vec<Dependency>> {
    Ok(dependencies(&state).await)
}

/// A plain text report to paste into a bug report. Secrets are removed before it leaves.
#[tauri::command]
pub async fn diagnostics_report(state: State<'_, AppState>) -> IpcResult<String> {
    let mut out = format!(
        "Circles {} on {} ({})\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    out.push_str("\nTools\n");
    for d in dependencies(&state).await {
        out.push_str(&format!(
            "  {:<7} {}  {}  {}\n",
            d.name,
            if d.ok { "ok" } else { "problem" },
            d.version.unwrap_or_default(),
            d.problem.unwrap_or_default()
        ));
    }
    let p = &state.paths;
    out.push_str(&format!(
        "\nFolders\n  data {}\n  logs {}\n",
        p.data_dir.display(),
        p.logs_dir.display()
    ));
    out.push_str("\nRecent log\n");
    if let Some(log) = latest_log(&p.logs_dir) {
        let lines: Vec<&str> = log.lines().collect();
        let from = lines.len().saturating_sub(80);
        for line in &lines[from..] {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(state.redactor.redact(&out))
}

fn latest_log(dir: &Path) -> Option<String> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?;
    std::fs::read_to_string(newest.path()).ok()
}
