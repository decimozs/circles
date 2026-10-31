//! The accounts Circles works as: the GitHub account the GitHub CLI uses, and the Claude account
//! Claude Code is signed in with. Circles never holds either login. It reads who they are and,
//! for GitHub, which one is active.

use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::cache::{swr, CacheStore};
use crate::commands::settings::check_claude;
use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct GithubLogin {
    pub login: String,
    /// The account the GitHub CLI uses now.
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct GithubAccounts {
    /// The GitHub CLI (`gh`) is installed.
    pub installed: bool,
    /// The accounts it is signed in to, the active one first.
    pub accounts: Vec<GithubLogin>,
    /// A command that fixes what is missing.
    pub fix: Option<String>,
}

async fn github_accounts_now(env: std::sync::Arc<crate::env::ResolvedEnv>) -> GithubAccounts {
    if env.which("gh").is_none() {
        return GithubAccounts {
            installed: false,
            accounts: Vec::new(),
            fix: Some("brew install gh".into()),
        };
    }
    let accounts: Vec<GithubLogin> = crate::github::signed_in_accounts(&env)
        .await
        .into_iter()
        .map(|(login, active)| GithubLogin { login, active })
        .collect();
    GithubAccounts {
        installed: true,
        fix: accounts.is_empty().then(|| "gh auth login".to_string()),
        accounts,
    }
}

/// Where the accounts are kept, and how long before they are asked again in the background. A
/// person who signs in or out in a terminal sees it here a couple of minutes later at most, or at
/// once with the refresh.
const ACCOUNTS_KEY: &str = "github-accounts";
const ACCOUNTS_MAX_AGE_MS: i64 = 2 * 60 * 1000;
/// The repository picker belongs to the active account, so it goes when that changes.
pub(crate) const REPOS_KEY: &str = "github-repos";

/// The account `gh` uses now, from what was kept, asked again in the background when it is old.
/// A change of the active account drops the kept repositories, which were the old one's.
#[tauri::command]
pub async fn github_accounts(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> IpcResult<GithubAccounts> {
    let env = state.env.get().await;
    let cache = CacheStore::new(state.db.clone());
    let kept = cache.clone();
    swr::serve(
        &app,
        &cache,
        ACCOUNTS_KEY,
        ACCOUNTS_MAX_AGE_MS,
        "github-accounts",
        move || {
            let (env, kept) = (env.clone(), kept.clone());
            async move {
                let fresh = github_accounts_now(env).await;
                let before = kept
                    .get::<GithubAccounts>(ACCOUNTS_KEY)
                    .ok()
                    .flatten()
                    .map(|e| active_login(&e.value));
                if before.is_some_and(|b| b != active_login(&fresh)) {
                    let _ = kept.delete(REPOS_KEY);
                }
                Ok(fresh)
            }
        },
    )
    .await
}

fn active_login(accounts: &GithubAccounts) -> Option<String> {
    accounts
        .accounts
        .iter()
        .find(|a| a.active)
        .map(|a| a.login.clone())
}

/// Makes another signed in account the one the GitHub CLI uses. It is the CLI's own setting, so
/// the change shows in a terminal too.
#[tauri::command]
pub async fn github_switch_account(
    state: State<'_, AppState>,
    login: String,
) -> IpcResult<GithubAccounts> {
    let env = state.env.get().await;
    crate::github::switch_account(&env, &login)
        .await
        .map_err(AppError::from)?;
    let accounts = github_accounts_now(env).await;
    let cache = CacheStore::new(state.db.clone());
    let _ = cache.put(ACCOUNTS_KEY, &accounts);
    let _ = cache.delete(REPOS_KEY);
    Ok(accounts)
}

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ClaudeAccount {
    /// Claude Code was found and answered.
    pub found: bool,
    pub signed_in: bool,
    pub email: Option<String>,
    /// How it is signed in: `claude.ai` (a Claude subscription), `api_key` and so on.
    pub method: Option<String>,
    /// The subscription, for example `pro` or `max`.
    pub plan: Option<String>,
    /// A command that fixes what is missing.
    pub fix: Option<String>,
}

/// What `claude auth status` printed. Only what the card shows is kept: not the organization.
pub fn parse_claude_account(output: &str) -> Option<ClaudeAccount> {
    let value: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    let text = |key: &str| {
        value[key]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let signed_in = value["loggedIn"].as_bool().unwrap_or(false);
    Some(ClaudeAccount {
        found: true,
        signed_in,
        email: text("email"),
        method: text("authMethod"),
        plan: text("subscriptionType"),
        fix: (!signed_in).then(|| "claude".to_string()),
    })
}

#[tauri::command]
pub async fn claude_account(state: State<'_, AppState>) -> IpcResult<ClaudeAccount> {
    let check = check_claude(&state).await?;
    let Some(path) = check.path.filter(|_| check.found) else {
        return Ok(ClaudeAccount {
            found: false,
            signed_in: false,
            email: None,
            method: None,
            plan: None,
            fix: None,
        });
    };
    // As the chats see it: the setting for `CLAUDE_CONFIG_DIR` and the like is on top.
    let env = crate::commands::app::env_with_settings(&state, None).await;
    Ok(read_claude_account(&path, &env).await)
}

/// Asks Claude Code at `path`, as `env` has it, who it is signed in as.
async fn read_claude_account(path: &str, env: &crate::env::ResolvedEnv) -> ClaudeAccount {
    let mut command = tokio::process::Command::new(path);
    env.apply(&mut command);
    command
        .args(["auth", "status"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output()).await;
    // Signed out may exit with an error and still print what it knows.
    let parsed = match output {
        Ok(Ok(out)) => parse_claude_account(&String::from_utf8_lossy(&out.stdout)),
        _ => None,
    };
    parsed.unwrap_or(ClaudeAccount {
        found: true,
        signed_in: false,
        email: None,
        method: None,
        plan: None,
        fix: Some("claude".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signed_in_account_keeps_the_email_the_method_and_the_plan_only() {
        let out = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"me@x.com","orgId":"secret-id","orgName":"Org","subscriptionType":"pro"}"#;
        let account = parse_claude_account(out).unwrap();
        assert_eq!(
            account,
            ClaudeAccount {
                found: true,
                signed_in: true,
                email: Some("me@x.com".into()),
                method: Some("claude.ai".into()),
                plan: Some("pro".into()),
                fix: None,
            }
        );
    }

    #[tokio::test]
    async fn the_account_is_read_with_the_variables_from_the_settings_on_top() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("claude");
        // The fake answers with whichever config folder it was started with.
        std::fs::write(
            &script,
            "#!/bin/sh\necho \"{\\\"loggedIn\\\":true,\\\"email\\\":\\\"$(basename \"$CLAUDE_CONFIG_DIR\")\\\"}\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let login = crate::env::ResolvedEnv::for_test(&[
            ("PATH", "/usr/bin:/bin"),
            ("CLAUDE_CONFIG_DIR", "/home/me/.claude"),
        ]);
        let path = script.to_str().unwrap();
        assert_eq!(
            read_claude_account(path, &login).await.email.as_deref(),
            Some(".claude")
        );
        let chosen = login.with_overrides([(
            "CLAUDE_CONFIG_DIR".to_string(),
            "/home/me/.claude-work".to_string(),
        )]);
        assert_eq!(
            read_claude_account(path, &chosen).await.email.as_deref(),
            Some(".claude-work")
        );
    }

    #[test]
    fn a_signed_out_account_says_how_to_sign_in() {
        let account = parse_claude_account(r#"{"loggedIn":false}"#).unwrap();
        assert!(!account.signed_in);
        assert_eq!(account.fix.as_deref(), Some("claude"));
        assert_eq!(parse_claude_account("not json"), None);
    }
}
