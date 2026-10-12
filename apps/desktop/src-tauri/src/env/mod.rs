//! Login-shell environment resolution.
//!
//! A GUI app launched from Finder has no shell PATH, so `claude`, `gh`, `node` and friends are
//! missing. We run the user's login shell once, read its environment, cache it, and apply it to
//! every child process we spawn.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;
use tokio::sync::OnceCell;
use ts_rs::TS;

/// Tools Circles shells out to. Reported in diagnostics so a missing one is obvious.
pub const TOOLS: &[&str] = &["claude", "gh", "git", "node", "pnpm"];

const START_MARKER: &str = "__CIRCLES_ENV_START__";
const END_MARKER: &str = "__CIRCLES_ENV_END__";
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Directories added to PATH when the login shell cannot be read.
const FALLBACK_PATH_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EnvSource {
    LoginShell,
    Fallback { reason: String },
}

#[derive(Debug, Clone)]
pub struct ResolvedEnv {
    vars: BTreeMap<String, String>,
    source: EnvSource,
}

impl ResolvedEnv {
    pub fn source(&self) -> &EnvSource {
        &self.source
    }

    /// Every variable, for building a child's environment.
    pub fn vars(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn var_count(&self) -> usize {
        self.vars.len()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    pub fn path(&self) -> &str {
        self.get("PATH").unwrap_or("")
    }

    pub fn path_entries(&self) -> Vec<String> {
        self.path()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Finds an executable on the resolved PATH.
    pub fn which(&self, name: &str) -> Option<PathBuf> {
        which_in(self.path(), name)
    }

    /// Replaces the child's environment with the resolved one.
    pub fn apply(&self, cmd: &mut Command) {
        cmd.env_clear().envs(&self.vars);
    }

    pub fn apply_std(&self, cmd: &mut std::process::Command) {
        cmd.env_clear().envs(&self.vars);
    }

    /// Values that must never reach a log file.
    pub fn secret_values(&self) -> Vec<String> {
        self.vars
            .iter()
            .filter(|(k, v)| is_secret_key(k) && v.len() >= 8)
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// Key names only. Values are never exposed through `Debug`-style summaries.
    pub fn keys(&self) -> Vec<&str> {
        self.vars.keys().map(String::as_str).collect()
    }

    #[cfg(test)]
    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }

    #[cfg(test)]
    pub fn without_var(mut self, key: &str) -> Self {
        self.vars.remove(key);
        self
    }

    #[cfg(test)]
    pub fn for_test(vars: &[(&str, &str)]) -> Self {
        Self {
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            source: EnvSource::LoginShell,
        }
    }
}

fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    [
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "AUTH",
    ]
    .iter()
    .any(|needle| k.contains(needle))
}

fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parses the NUL separated `env -0` block between our markers. Shell banners and prompts
/// printed outside the markers are ignored.
pub fn parse_env_block(raw: &str) -> Option<BTreeMap<String, String>> {
    let start = raw.find(START_MARKER)? + START_MARKER.len();
    let end = raw[start..].find(END_MARKER)? + start;
    let mut vars = BTreeMap::new();
    for entry in raw[start..end].split('\0') {
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        if is_valid_key(key) {
            vars.insert(key.to_string(), value.to_string());
        }
    }
    if vars.is_empty() {
        None
    } else {
        Some(vars)
    }
}

pub fn which_in(path: &str, name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    path.split(':').filter(|d| !d.is_empty()).find_map(|dir| {
        let candidate = Path::new(dir).join(name);
        let meta = std::fs::metadata(&candidate).ok()?;
        (meta.is_file() && meta.permissions().mode() & 0o111 != 0).then_some(candidate)
    })
}

fn fallback_env(reason: String) -> ResolvedEnv {
    let mut vars: BTreeMap<String, String> = std::env::vars().collect();
    let mut dirs: Vec<String> = vars
        .get("PATH")
        .map(|p| {
            p.split(':')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut extra: Vec<String> = FALLBACK_PATH_DIRS.iter().map(|s| s.to_string()).collect();
    if let Some(home) = dirs::home_dir() {
        extra.push(home.join(".local/bin").to_string_lossy().into_owned());
        extra.push(home.join(".cargo/bin").to_string_lossy().into_owned());
    }
    for dir in extra {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    vars.insert("PATH".into(), dirs.join(":"));
    ResolvedEnv {
        vars,
        source: EnvSource::Fallback { reason },
    }
}

async fn read_login_shell_env() -> Result<BTreeMap<String, String>, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let script = format!("printf '{START_MARKER}'; env -0; printf '{END_MARKER}'");

    let mut cmd = Command::new(&shell);
    cmd.args(["-ilc", &script])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let output = tokio::time::timeout(RESOLVE_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            format!(
                "`{shell} -ilc` timed out after {}s",
                RESOLVE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("cannot run `{shell}`: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    parse_env_block(&text).ok_or_else(|| format!("`{shell}` printed no environment block"))
}

/// Resolves once: login shell first, then a fallback PATH.
pub async fn resolve() -> ResolvedEnv {
    match read_login_shell_env().await {
        Ok(login) => {
            // The login shell wins, but keep anything only the app process knows about.
            let mut vars: BTreeMap<String, String> = std::env::vars().collect();
            vars.extend(login);
            ResolvedEnv {
                vars,
                source: EnvSource::LoginShell,
            }
        }
        Err(reason) => {
            tracing::warn!(%reason, "login shell environment unavailable, using fallback PATH");
            fallback_env(reason)
        }
    }
}

/// Shared, lazily initialised handle. `get` resolves on first use and caches the result.
#[derive(Clone, Default)]
pub struct EnvResolver {
    cell: Arc<OnceCell<Arc<ResolvedEnv>>>,
}

impl EnvResolver {
    pub async fn get(&self) -> Arc<ResolvedEnv> {
        self.cell
            .get_or_init(|| async { Arc::new(resolve().await) })
            .await
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_block_and_ignores_noise() {
        let raw = format!(
            "Welcome to zsh!\n{START_MARKER}PATH=/a:/b\0HOME=/Users/x\0MULTI=line1\nline2\0{END_MARKER}trailing prompt"
        );
        let vars = parse_env_block(&raw).unwrap();
        assert_eq!(vars["PATH"], "/a:/b");
        assert_eq!(vars["HOME"], "/Users/x");
        assert_eq!(vars["MULTI"], "line1\nline2");
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn keeps_equals_signs_in_values() {
        let raw = format!("{START_MARKER}LS_COLORS=di=01;34:ln=01;36\0{END_MARKER}");
        assert_eq!(
            parse_env_block(&raw).unwrap()["LS_COLORS"],
            "di=01;34:ln=01;36"
        );
    }

    #[test]
    fn rejects_missing_markers_and_bad_keys() {
        assert!(parse_env_block("PATH=/a\0").is_none());
        let raw = format!("{START_MARKER}1BAD=x\0has space=y\0{END_MARKER}");
        assert!(parse_env_block(&raw).is_none());
    }

    #[test]
    fn secret_values_are_detected_by_key() {
        let env = ResolvedEnv::for_test(&[
            ("GITHUB_TOKEN", "ghp_supersecretvalue"),
            ("SHORT_KEY", "abc"),
            ("EDITOR", "vim-the-editor"),
        ]);
        assert_eq!(
            env.secret_values(),
            vec!["ghp_supersecretvalue".to_string()]
        );
    }

    #[test]
    fn finds_executables_on_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("mytool");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let mut perms = std::fs::metadata(&bin).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&bin, perms).unwrap();
        let path = format!("/nonexistent:{}", tmp.path().display());
        assert_eq!(which_in(&path, "mytool"), Some(bin));
        assert_eq!(which_in(&path, "nothere"), None);
    }

    #[test]
    fn fallback_path_contains_homebrew() {
        let env = fallback_env("test".into());
        assert!(env.path_entries().iter().any(|p| p == "/opt/homebrew/bin"));
        assert!(matches!(env.source(), EnvSource::Fallback { .. }));
    }

    #[tokio::test]
    async fn resolves_real_login_shell() {
        let env = resolve().await;
        assert!(!env.path().is_empty());
        assert!(env.which("git").is_some());
    }
}
