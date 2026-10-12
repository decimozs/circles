//! Where the Jira token is kept: the macOS Keychain. Hashing is not an option, since every call to
//! Jira needs the real token. Nothing here logs or prints it.

use std::fmt;
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

fn keychain(error: keyring::Error) -> AppError {
    // The text of a keychain error never holds the secret.
    AppError::Env(format!("the Keychain said: {error}"))
}

pub struct KeychainSecrets {
    service: String,
}

fn read_entry(entry: &keyring::Entry) -> AppResult<Option<String>> {
    match entry.get_password() {
        Ok(text) => Ok(Some(text)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain(e)),
    }
}

fn remove_entry(entry: &keyring::Entry) -> AppResult<()> {
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keychain(e)),
    }
}

impl KeychainSecrets {
    pub fn new() -> Self {
        Self {
            service: "circles".into(),
        }
    }

    fn entry(&self) -> AppResult<keyring::Entry> {
        keyring::Entry::new(&self.service, "jira").map_err(keychain)
    }

    fn oauth_entry(&self) -> AppResult<keyring::Entry> {
        keyring::Entry::new(&self.service, "jira-oauth").map_err(keychain)
    }
}

impl SecretStore for KeychainSecrets {
    fn load(&self) -> AppResult<Option<Credentials>> {
        match self.entry()?.get_password() {
            Ok(text) => Ok(serde_json::from_str(&text).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain(e)),
        }
    }

    fn save(&self, credentials: &Credentials) -> AppResult<()> {
        let text =
            serde_json::to_string(credentials).map_err(|e| AppError::Internal(e.to_string()))?;
        self.entry()?.set_password(&text).map_err(keychain)
    }

    fn delete(&self) -> AppResult<()> {
        remove_entry(&self.entry()?)
    }

    fn load_oauth(&self) -> AppResult<Option<OAuthSession>> {
        self.oauth_load()
    }

    fn save_oauth(&self, session: &OAuthSession) -> AppResult<()> {
        let text = serde_json::to_string(session).map_err(|e| AppError::Internal(e.to_string()))?;
        self.oauth_entry()?.set_password(&text).map_err(keychain)
    }

    fn delete_oauth(&self) -> AppResult<()> {
        remove_entry(&self.oauth_entry()?)
    }
}

impl KeychainSecrets {
    fn oauth_load(&self) -> AppResult<Option<OAuthSession>> {
        Ok(read_entry(&self.oauth_entry()?)?.and_then(|t| serde_json::from_str(&t).ok()))
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
