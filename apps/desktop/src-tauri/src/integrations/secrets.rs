//! Where the Jira login is kept: a file in the app's data folder that only the person who runs
//! Amplify can read. Hashing is not an option, since every call to Jira needs the real token.
//! Nothing here logs or prints it.
//!
//! It was the macOS Keychain, which asks for the account's password whenever the program asking is
//! not the one that was allowed before, and every build of an app that is not signed is a new
//! program. A file has no such prompt, and anything running as the same person can read it, which is
//! also true of the Keychain once a program is allowed.

use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::jira::client::JiraFailure;
use crate::error::{AppError, AppResult};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Credentials {
    pub site: String,
    pub email: String,
    pub token: String,
}

// Never prints the token, wherever a `{:?}` ends up.
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("site", &self.site)
            .field("email", &self.email)
            .field("token", &"<hidden>")
            .finish()
    }
}

/// `name.atlassian.net`: the host of a Jira Cloud site, nothing else.
pub fn clean_site(raw: &str) -> Result<String, JiraFailure> {
    let site = raw
        .trim()
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    let name = site.strip_suffix(".atlassian.net").unwrap_or("");
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-');
    if valid {
        Ok(site)
    } else {
        Err(JiraFailure::BadSite)
    }
}

impl Credentials {
    pub fn new(site: &str, email: &str, token: &str) -> Result<Self, JiraFailure> {
        let email = email.trim();
        let token = token.trim();
        if !email.contains('@') || token.is_empty() {
            return Err(JiraFailure::BadLogin);
        }
        Ok(Self {
            site: clean_site(site)?,
            email: email.into(),
            token: token.into(),
        })
    }
}

/// A sign-in through Atlassian: what is needed to call its MCP server, and to renew the access.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct OAuthSession {
    pub client_id: String,
    pub token_endpoint: String,
    /// The MCP server the tokens are for.
    pub resource: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at_ms: i64,
    pub cloud_id: String,
    /// `name.atlassian.net` or the like, for display.
    pub site: String,
    pub account_name: String,
}

impl fmt::Debug for OAuthSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthSession")
            .field("site", &self.site)
            .field("account_name", &self.account_name)
            .field("access_token", &"<hidden>")
            .field("refresh_token", &"<hidden>")
            .finish()
    }
}

pub trait SecretStore: Send + Sync {
    fn load(&self) -> AppResult<Option<Credentials>>;
    fn save(&self, credentials: &Credentials) -> AppResult<()>;
    fn delete(&self) -> AppResult<()>;
    fn load_oauth(&self) -> AppResult<Option<OAuthSession>>;
    fn save_oauth(&self, session: &OAuthSession) -> AppResult<()>;
    fn delete_oauth(&self) -> AppResult<()>;
}

/// What is in the file: the token login and the Atlassian sign-in, each when there is one.
#[derive(Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    jira: Option<Credentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    oauth: Option<OAuthSession>,
}

/// The logins, in a file made readable and writable by its owner only (mode 0600).
pub struct FileSecrets {
    path: PathBuf,
    /// One change at a time, so two writes cannot lose each other.
    lock: Mutex<()>,
}

impl FileSecrets {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// What is in the file. A file that is missing, empty or cannot be read as it should is the same
    /// as nothing saved: the person connects again.
    fn read(&self) -> Stored {
        tighten(&self.path);
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Replaces the file, or removes it when nothing is left. It is written under another name
    /// first, with its permissions set before anything secret goes in, and then moved into place,
    /// so a crash never leaves half a file and the secret is never readable by others.
    fn write(&self, stored: &Stored) -> AppResult<()> {
        if stored.jira.is_none() && stored.oauth.is_none() {
            return match std::fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            };
        }
        let text = serde_json::to_string(stored).map_err(|e| AppError::Internal(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        write_private(&temporary, text.as_bytes())?;
        std::fs::rename(&temporary, &self.path)?;
        Ok(())
    }

    fn change(&self, edit: impl FnOnce(&mut Stored)) -> AppResult<()> {
        let _one_at_a_time = self
            .lock
            .lock()
            .map_err(|_| AppError::Internal("secrets lock poisoned".into()))?;
        let mut stored = self.read();
        edit(&mut stored);
        self.write(&stored)
    }
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // A file left from an earlier run keeps its own mode, so it is set again.
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

/// Takes group and other access off a file that has it, for one that was changed by hand.
#[cfg(unix)]
fn tighten(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.permissions().mode() & 0o077 != 0 {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

#[cfg(not(unix))]
fn tighten(_path: &std::path::Path) {}

impl SecretStore for FileSecrets {
    fn load(&self) -> AppResult<Option<Credentials>> {
        Ok(self.read().jira)
    }

    fn save(&self, credentials: &Credentials) -> AppResult<()> {
        self.change(|s| s.jira = Some(credentials.clone()))
    }

    fn delete(&self) -> AppResult<()> {
        self.change(|s| s.jira = None)
    }

    fn load_oauth(&self) -> AppResult<Option<OAuthSession>> {
        Ok(self.read().oauth)
    }

    fn save_oauth(&self, session: &OAuthSession) -> AppResult<()> {
        self.change(|s| s.oauth = Some(session.clone()))
    }

    fn delete_oauth(&self) -> AppResult<()> {
        self.change(|s| s.oauth = None)
    }
}

/// For tests: the same, in memory.
#[derive(Default)]
pub struct MemorySecrets(
    pub Mutex<Option<Credentials>>,
    pub Mutex<Option<OAuthSession>>,
);

impl SecretStore for MemorySecrets {
    fn load(&self) -> AppResult<Option<Credentials>> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, credentials: &Credentials) -> AppResult<()> {
        *self.0.lock().unwrap() = Some(credentials.clone());
        Ok(())
    }
    fn delete(&self) -> AppResult<()> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
    fn load_oauth(&self) -> AppResult<Option<OAuthSession>> {
        Ok(self.1.lock().unwrap().clone())
    }
    fn save_oauth(&self, session: &OAuthSession) -> AppResult<()> {
        *self.1.lock().unwrap() = Some(session.clone());
        Ok(())
    }
    fn delete_oauth(&self) -> AppResult<()> {
        *self.1.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> OAuthSession {
        OAuthSession {
            client_id: "client".into(),
            token_endpoint: "https://auth.example/token".into(),
            resource: "https://mcp.example".into(),
            access_token: "ACCESS-SECRET".into(),
            refresh_token: Some("REFRESH-SECRET".into()),
            expires_at_ms: 1,
            cloud_id: "cloud".into(),
            site: "t.atlassian.net".into(),
            account_name: "Marlon".into(),
        }
    }

    fn login() -> Credentials {
        Credentials::new("t.atlassian.net", "me@x.com", "API-SECRET").unwrap()
    }

    fn store() -> (tempfile::TempDir, FileSecrets) {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("integration-secrets.json");
        (tmp, FileSecrets::new(file))
    }

    #[test]
    fn a_saved_login_comes_back_and_a_deleted_one_does_not() {
        let (_tmp, secrets) = store();
        assert_eq!(secrets.load().unwrap(), None);
        secrets.save(&login()).unwrap();
        assert_eq!(secrets.load().unwrap(), Some(login()));
        secrets.delete().unwrap();
        assert_eq!(secrets.load().unwrap(), None);
        // Deleting what is not there is not an error.
        secrets.delete().unwrap();
        secrets.delete_oauth().unwrap();
    }

    #[test]
    fn the_token_login_and_the_atlassian_sign_in_are_kept_apart() {
        let (_tmp, secrets) = store();
        secrets.save(&login()).unwrap();
        secrets.save_oauth(&session()).unwrap();
        assert_eq!(secrets.load().unwrap(), Some(login()));
        assert_eq!(secrets.load_oauth().unwrap(), Some(session()));
        secrets.delete_oauth().unwrap();
        assert_eq!(secrets.load_oauth().unwrap(), None);
        assert_eq!(
            secrets.load().unwrap(),
            Some(login()),
            "the other one stays"
        );
    }

    #[test]
    fn it_is_the_file_that_keeps_them_so_another_run_of_the_app_sees_them() {
        let (tmp, secrets) = store();
        secrets.save(&login()).unwrap();
        let again = FileSecrets::new(tmp.path().join("integration-secrets.json"));
        assert_eq!(again.load().unwrap(), Some(login()));
    }

    #[test]
    fn the_file_goes_when_nothing_is_left_in_it() {
        let (tmp, secrets) = store();
        let file = tmp.path().join("integration-secrets.json");
        secrets.save(&login()).unwrap();
        assert!(file.exists());
        secrets.delete().unwrap();
        assert!(
            !file.exists(),
            "no empty file with a name that says secrets"
        );
        // And nothing is left over from writing it.
        assert!(!file.with_extension("tmp").exists());
    }

    #[test]
    fn a_file_that_cannot_be_read_is_the_same_as_none_and_is_replaced_by_the_next_save() {
        let (tmp, secrets) = store();
        let file = tmp.path().join("integration-secrets.json");
        std::fs::write(&file, "this is not json").unwrap();
        assert_eq!(secrets.load().unwrap(), None);
        assert_eq!(secrets.load_oauth().unwrap(), None);
        secrets.save(&login()).unwrap();
        assert_eq!(secrets.load().unwrap(), Some(login()));
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_it_even_when_it_was_left_open_before() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, secrets) = store();
        let file = tmp.path().join("integration-secrets.json");
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        secrets.save(&login()).unwrap();
        assert_eq!(mode(&file), 0o600);

        // Made open by hand, it is closed again the next time it is read.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(secrets.load().unwrap(), Some(login()));
        assert_eq!(mode(&file), 0o600);

        // A file that was there, open, is not written to before it is closed: the new content never
        // sits in a readable file.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();
        secrets.save_oauth(&session()).unwrap();
        assert_eq!(mode(&file), 0o600);
    }

    #[test]
    fn nothing_prints_a_secret() {
        let text = format!("{:?} {:?}", login(), session());
        for secret in ["API-SECRET", "ACCESS-SECRET", "REFRESH-SECRET"] {
            assert!(!text.contains(secret), "{secret} was printed");
        }
    }

    #[test]
    fn several_changes_at_once_do_not_lose_each_other() {
        let (_tmp, secrets) = store();
        let secrets = std::sync::Arc::new(secrets);
        let a = secrets.clone();
        let b = secrets.clone();
        let one = std::thread::spawn(move || {
            for _ in 0..30 {
                a.save(&login()).unwrap();
            }
        });
        let two = std::thread::spawn(move || {
            for _ in 0..30 {
                b.save_oauth(&session()).unwrap();
            }
        });
        one.join().unwrap();
        two.join().unwrap();
        assert_eq!(secrets.load().unwrap(), Some(login()));
        assert_eq!(secrets.load_oauth().unwrap(), Some(session()));
    }
}
