//! Outside services Circles can talk to. Jira Cloud for now.

pub mod atlassian;
pub mod jira;
pub mod meta;
pub mod secrets;

use std::sync::{Arc, Mutex};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use jira::client::{JiraClient, JiraFailure};
use secrets::{Credentials, SecretStore};

/// What the app tells the UI about the Jira connection. Never the token.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct JiraStatus {
    pub connected: bool,
    pub site: Option<String>,
    pub email: Option<String>,
    pub account_name: Option<String>,
    /// Jira answered 401 or 403 to the saved token: it was revoked or has expired.
    pub needs_reconnect: bool,
    /// How it is connected: `oauth` (signed in with Atlassian) or `token`.
    pub method: Option<String>,
    /// After signing in with several sites to pick from: the sites. Empty otherwise.
    pub choose_site: Vec<atlassian::JiraSite>,
}

#[derive(Debug, Clone)]
struct Known {
    credentials: Credentials,
    account_name: String,
    needs_reconnect: bool,
}

/// The saved connections, and the one place that turns them into calls.
pub struct Integrations {
    store: Box<dyn SecretStore>,
    /// Where Jira is reached. Only tests set it, to a fake server.
    base_override: Option<String>,
    known: Mutex<Option<Known>>,
    /// Whether ordinary chats may use Jira. On unless switched off in Settings.
    chats: std::sync::atomic::AtomicBool,
    /// The sign-in with Atlassian, when that is how Jira is connected.
    oauth: Mutex<OAuthSlot>,
    atlassian: atlassian::Runtime,
    /// Atlassian's MCP server. Only tests set it, to a fake one.
    mcp_url_override: Option<String>,
    /// Names tried for the browser's way back to this Mac, in order.
    redirect_hosts: Vec<String>,
    /// Signed in, waiting for a site to be picked.
    pending: Mutex<Option<atlassian::Pending>>,
    /// Ends a sign-in that waits for the browser.
    cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Where the connection's non-secret details are kept.
    meta_path: Option<std::path::PathBuf>,
    /// `None` until read. `Some(None)` when nothing is connected.
    meta: Mutex<Option<Option<meta::Meta>>>,
    /// Jira turned the saved login down. Cleared by connecting again.
    rejected: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
pub(crate) struct OAuthSlot {
    pub loaded: bool,
    pub session: Option<secrets::OAuthSession>,
    pub needs_reconnect: bool,
}

fn friendly(failure: JiraFailure) -> AppError {
    AppError::Invalid(failure.message())
}

impl Integrations {
    pub fn new(store: Box<dyn SecretStore>, meta_path: std::path::PathBuf) -> Arc<Self> {
        Arc::new(Self {
            store,
            base_override: None,
            known: Mutex::new(None),
            chats: std::sync::atomic::AtomicBool::new(true),
            oauth: Mutex::new(OAuthSlot::default()),
            atlassian: atlassian::Runtime::default(),
            mcp_url_override: None,
            redirect_hosts: vec!["localhost".into(), "127.0.0.1".into()],
            pending: Mutex::new(None),
            cancel: Mutex::new(None),
            meta_path: Some(meta_path),
            meta: Mutex::new(None),
            rejected: std::sync::atomic::AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    pub fn for_test(store: Box<dyn SecretStore>, base: String) -> Arc<Self> {
        Arc::new(Self {
            store,
            base_override: Some(base),
            known: Mutex::new(None),
            chats: std::sync::atomic::AtomicBool::new(true),
            oauth: Mutex::new(OAuthSlot::default()),
            atlassian: atlassian::Runtime::default(),
            mcp_url_override: None,
            redirect_hosts: vec!["localhost".into(), "127.0.0.1".into()],
            pending: Mutex::new(None),
            cancel: Mutex::new(None),
            meta_path: None,
            meta: Mutex::new(None),
            rejected: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// For tests of the sign-in with Atlassian: a fake MCP server, and the redirect names to try.
    #[cfg(test)]
    pub fn for_test_oauth(
        store: Box<dyn SecretStore>,
        mcp_url: String,
        hosts: &[&str],
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            base_override: None,
            known: Mutex::new(None),
            chats: std::sync::atomic::AtomicBool::new(true),
            oauth: Mutex::new(OAuthSlot::default()),
            atlassian: atlassian::Runtime::default(),
            mcp_url_override: Some(mcp_url),
            redirect_hosts: hosts.iter().map(|h| h.to_string()).collect(),
            pending: Mutex::new(None),
            cancel: Mutex::new(None),
            meta_path: None,
            meta: Mutex::new(None),
            rejected: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn client(&self, credentials: &Credentials) -> AppResult<JiraClient> {
        JiraClient::new(credentials, self.base_override.as_deref()).map_err(friendly)
    }

    /// The saved credentials, read from the store once.
    fn load(&self) -> AppResult<Option<Known>> {
        let mut known = self.known.lock().unwrap();
        if known.is_none() {
            if let Some(credentials) = self.store.load()? {
                *known = Some(Known {
                    account_name: credentials.email.clone(),
                    credentials,
                    needs_reconnect: false,
                });
            }
        }
        Ok(known.clone())
    }

    /// What is connected, from the plain file. The first time there is no file, it is worked out
    /// from the Keychain once, which is how connections made before this existed are found. That
    /// is the only time the Keychain is asked about the state.
    pub(crate) fn current_meta(&self) -> Option<meta::Meta> {
        let mut slot = self.meta.lock().unwrap();
        if slot.is_none() {
            let from_file = self.meta_path.as_deref().and_then(meta::read);
            let found = match from_file {
                Some(m) => Some(m).filter(meta::Meta::connected),
                None => {
                    let derived = if let Ok(Some(s)) = self.store.load_oauth() {
                        Some(meta::Meta {
                            method: Some("oauth".into()),
                            site: Some(s.site),
                            email: None,
                            account_name: Some(s.account_name),
                        })
                    } else if let Ok(Some(c)) = self.store.load() {
                        Some(meta::Meta {
                            method: Some("token".into()),
                            site: Some(c.site),
                            email: Some(c.email.clone()),
                            account_name: Some(c.email),
                        })
                    } else {
                        None
                    };
                    if let Some(path) = &self.meta_path {
                        meta::write(path, &derived.clone().unwrap_or_default());
                    }
                    derived
                }
            };
            *slot = Some(found);
        }
        slot.clone().flatten()
    }

    pub(crate) fn set_meta(&self, value: Option<meta::Meta>) {
        if let Some(path) = &self.meta_path {
            meta::write(path, &value.clone().unwrap_or_default());
        }
        *self.meta.lock().unwrap() = Some(value);
        self.rejected
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn is_rejected(&self) -> bool {
        self.rejected.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn jira_status(&self) -> AppResult<JiraStatus> {
        let choose_site = self.pending_sites();
        Ok(match self.current_meta() {
            None => JiraStatus {
                connected: false,
                site: None,
                email: None,
                account_name: None,
                needs_reconnect: false,
                method: None,
                choose_site,
            },
            Some(m) => JiraStatus {
                connected: true,
                site: m.site,
                email: m.email,
                account_name: m.account_name,
                needs_reconnect: self.is_rejected(),
                method: m.method,
                choose_site,
            },
        })
    }

    pub fn set_chats(&self, allowed: bool) {
        self.chats
            .store(allowed, std::sync::atomic::Ordering::SeqCst);
    }

    /// Jira is connected and chats may use it.
    pub fn jira_for_chats(&self) -> bool {
        self.chats.load(std::sync::atomic::Ordering::SeqCst) && self.is_jira_connected()
    }

    pub fn is_jira_connected(&self) -> bool {
        self.current_meta().is_some() && !self.is_rejected()
    }

    /// Checks the token against Jira, and only then saves it.
    pub async fn jira_connect(
        &self,
        site: &str,
        email: &str,
        token: &str,
    ) -> AppResult<JiraStatus> {
        let credentials = Credentials::new(site, email, token).map_err(friendly)?;
        let me = self
            .client(&credentials)?
            .myself()
            .await
            .map_err(friendly)?;
        self.store.save(&credentials)?;
        // One way of being connected at a time.
        self.clear_oauth()?;
        self.set_meta(Some(meta::Meta {
            method: Some("token".into()),
            site: Some(credentials.site.clone()),
            email: Some(credentials.email.clone()),
            account_name: Some(me.display_name.clone()),
        }));
        *self.known.lock().unwrap() = Some(Known {
            credentials,
            account_name: me.display_name,
            needs_reconnect: false,
        });
        self.jira_status()
    }

    /// Forgets the connection and deletes the saved token.
    pub fn jira_disconnect(&self) -> AppResult<JiraStatus> {
        self.store.delete()?;
        self.clear_oauth()?;
        self.set_meta(None);
        *self.known.lock().unwrap() = None;
        self.jira_status()
    }

    /// A client for the saved connection. A later 401 or 403 marks it as needing a reconnect.
    pub fn jira_client(&self) -> AppResult<JiraClient> {
        if self.is_oauth() {
            return Err(AppError::Invalid(
                "Jira is connected by signing in with Atlassian, not with a token.".into(),
            ));
        }
        let known = self.load()?.ok_or_else(|| {
            AppError::Invalid("Jira is not connected. Connect it in Settings, Integrations.".into())
        })?;
        if known.needs_reconnect || self.is_rejected() {
            return Err(AppError::Invalid(
                "Jira needs to be reconnected in Settings, Integrations.".into(),
            ));
        }
        self.client(&known.credentials)
    }

    /// Called when Jira turned the saved token down.
    pub fn jira_rejected(&self) {
        self.rejected
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if self.is_oauth() {
            self.mark_oauth_rejected();
        }
        if let Some(known) = self.known.lock().unwrap().as_mut() {
            known.needs_reconnect = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::fake_jira as fake;
    use super::*;
    use secrets::MemorySecrets;

    const OK: &str = r#"{"accountId":"a1","displayName":"Marlon"}"#;

    fn memory() -> Box<MemorySecrets> {
        Box::default()
    }

    #[tokio::test]
    async fn connecting_checks_the_token_then_saves_it() {
        let base = fake("200 OK", "", OK).await.0;
        let hub = Integrations::for_test(memory(), base);
        let status = hub
            .jira_connect("Team.Atlassian.net", "me@x.com", "tok")
            .await
            .unwrap();
        assert!(status.connected);
        assert_eq!(status.site.as_deref(), Some("team.atlassian.net"));
        assert_eq!(status.account_name.as_deref(), Some("Marlon"));
        assert!(hub.is_jira_connected());
    }

    #[tokio::test]
    async fn a_refused_token_is_not_saved() {
        let base = fake("401 Unauthorized", "", "{}").await.0;
        let hub = Integrations::for_test(memory(), base);
        let err = hub
            .jira_connect("team.atlassian.net", "me@x.com", "bad")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("did not accept"));
        assert!(!hub.jira_status().unwrap().connected);
    }

    #[tokio::test]
    async fn rate_limits_say_how_long_to_wait() {
        let base = fake("429 Too Many Requests", "retry-after: 7\r\n", "{}")
            .await
            .0;
        let hub = Integrations::for_test(memory(), base);
        let err = hub
            .jira_connect("team.atlassian.net", "me@x.com", "t")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("7 seconds"));
    }

    #[tokio::test]
    async fn only_atlassian_cloud_sites_are_accepted() {
        let hub = Integrations::for_test(memory(), "http://127.0.0.1:1".into());
        for site in [
            "example.com",
            "evil.com/team.atlassian.net",
            "-x.atlassian.net",
            ".atlassian.net",
            "",
        ] {
            let err = hub.jira_connect(site, "me@x.com", "t").await.unwrap_err();
            assert!(err.to_string().contains("Jira Cloud"), "{site}");
        }
    }

    #[tokio::test]
    async fn disconnecting_forgets_everything() {
        let base = fake("200 OK", "", OK).await.0;
        let hub = Integrations::for_test(memory(), base);
        hub.jira_connect("team.atlassian.net", "me@x.com", "tok")
            .await
            .unwrap();
        let status = hub.jira_disconnect().unwrap();
        assert!(!status.connected);
        assert!(hub.jira_client().is_err());
    }

    #[tokio::test]
    async fn a_rejection_later_asks_for_a_reconnect() {
        let base = fake("200 OK", "", OK).await.0;
        let hub = Integrations::for_test(memory(), base);
        hub.jira_connect("team.atlassian.net", "me@x.com", "tok")
            .await
            .unwrap();
        hub.jira_rejected();
        assert!(hub.jira_status().unwrap().needs_reconnect);
        assert!(!hub.is_jira_connected());
        assert!(hub
            .jira_client()
            .err()
            .unwrap()
            .to_string()
            .contains("reconnected"));
    }

    /// A store that counts how often the secrets are read.
    struct Counting {
        inner: MemorySecrets,
        reads: std::sync::atomic::AtomicUsize,
    }
    impl SecretStore for std::sync::Arc<Counting> {
        fn load(&self) -> AppResult<Option<Credentials>> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.load()
        }
        fn save(&self, c: &Credentials) -> AppResult<()> {
            self.inner.save(c)
        }
        fn delete(&self) -> AppResult<()> {
            self.inner.delete()
        }
        fn load_oauth(&self) -> AppResult<Option<secrets::OAuthSession>> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.load_oauth()
        }
        fn save_oauth(&self, s: &secrets::OAuthSession) -> AppResult<()> {
            self.inner.save_oauth(s)
        }
        fn delete_oauth(&self) -> AppResult<()> {
            self.inner.delete_oauth()
        }
    }

    #[test]
    fn the_state_is_shown_without_asking_the_keychain() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("integrations.json");
        std::fs::write(
            &file,
            r#"{"method":"oauth","site":"t.atlassian.net","email":null,"account_name":"Marlon"}"#,
        )
        .unwrap();
        let store = std::sync::Arc::new(Counting {
            inner: MemorySecrets::default(),
            reads: std::sync::atomic::AtomicUsize::new(0),
        });
        let hub = Integrations::new(Box::new(store.clone()), file);
        for _ in 0..5 {
            let status = hub.jira_status().unwrap();
            assert!(status.connected);
            assert_eq!(status.site.as_deref(), Some("t.atlassian.net"));
            assert_eq!(status.method.as_deref(), Some("oauth"));
            assert!(hub.is_jira_connected());
        }
        assert_eq!(
            store.reads.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "no password prompt just for showing the state"
        );
    }

    #[test]
    fn a_connection_from_before_is_found_once_and_then_remembered_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("integrations.json");
        let inner = MemorySecrets::default();
        inner
            .save(&Credentials::new("t.atlassian.net", "me@x.com", "tok").unwrap())
            .unwrap();
        let store = std::sync::Arc::new(Counting {
            inner,
            reads: std::sync::atomic::AtomicUsize::new(0),
        });
        let hub = Integrations::new(Box::new(store.clone()), file.clone());
        let status = hub.jira_status().unwrap();
        assert_eq!(status.method.as_deref(), Some("token"));
        assert!(file.exists(), "written for next time");
        let reads = store.reads.load(std::sync::atomic::Ordering::SeqCst);
        assert!(reads > 0);

        // The next start does not read the secrets at all.
        let again = Integrations::new(Box::new(store.clone()), file);
        assert!(again.jira_status().unwrap().connected);
        assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), reads);
    }

    #[test]
    fn nothing_connected_is_remembered_too() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("integrations.json");
        let store = std::sync::Arc::new(Counting {
            inner: MemorySecrets::default(),
            reads: std::sync::atomic::AtomicUsize::new(0),
        });
        let hub = Integrations::new(Box::new(store.clone()), file.clone());
        assert!(!hub.jira_status().unwrap().connected);
        let reads = store.reads.load(std::sync::atomic::Ordering::SeqCst);
        let again = Integrations::new(Box::new(store.clone()), file);
        assert!(!again.jira_status().unwrap().connected);
        assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), reads);
    }

    #[test]
    fn the_token_is_never_printed() {
        let credentials = Credentials::new("t.atlassian.net", "me@x.com", "SECRET-TOKEN").unwrap();
        assert!(!format!("{credentials:?}").contains("SECRET-TOKEN"));
        let status = JiraStatus {
            connected: true,
            site: Some(credentials.site.clone()),
            email: Some(credentials.email.clone()),
            account_name: None,
            needs_reconnect: false,
            method: None,
            choose_site: vec![],
        };
        assert!(!format!("{status:?}").contains("SECRET-TOKEN"));
    }
}

/// A fake Jira for tests.
#[cfg(test)]
pub(crate) mod testing {
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A Jira that answers every request with the given status and body. Gives its address and the
    /// requests it received, as text.
    pub async fn fake_jira(
        status: &'static str,
        extra: &'static str,
        body: &'static str,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 16384];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..n]).into_owned());
                let reply = format!(
                    "HTTP/1.1 {status}\r\n{extra}content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    /// A Jira with an answer per address: the first route whose text is in the request line wins,
    /// and anything else is a 404.
    pub async fn fake_jira_routes(
        routes: Vec<(&'static str, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 16384];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let line = request.lines().next().unwrap_or("").to_string();
                log.lock().unwrap().push(request);
                let (status, body) = routes
                    .iter()
                    .find(|(path, _)| line.contains(path))
                    .map_or(("404 Not Found", "{}"), |(_, body)| ("200 OK", *body));
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    /// One request a fake server saw.
    #[derive(Debug, Clone)]
    pub struct Req {
        pub method: String,
        /// With the query.
        pub path: String,
        pub headers: std::collections::HashMap<String, String>,
        pub body: String,
    }

    pub struct Resp {
        pub status: u16,
        pub content_type: &'static str,
        pub headers: Vec<(&'static str, String)>,
        pub body: String,
    }

    impl Resp {
        pub fn json(body: serde_json::Value) -> Self {
            Self {
                status: 200,
                content_type: "application/json",
                headers: vec![],
                body: body.to_string(),
            }
        }
        pub fn status(status: u16, body: serde_json::Value) -> Self {
            Self {
                status,
                ..Self::json(body)
            }
        }
    }

    /// A fake HTTP server driven by a function. Gives its address and every request it saw.
    pub async fn fake_http(
        handler: impl Fn(&Req) -> Resp + Send + Sync + 'static,
    ) -> (String, Arc<Mutex<Vec<Req>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let handler = Arc::new(handler);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let log = log.clone();
                let handler = handler.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buf = vec![0u8; 8192];
                    let (head_end, length) = loop {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buf[..n]);
                        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&data[..pos]).to_lowercase();
                            let length = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            break (pos + 4, length);
                        }
                    };
                    while data.len() < head_end + length {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        data.extend_from_slice(&buf[..n]);
                    }
                    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
                    let mut lines = head.lines();
                    let first = lines.next().unwrap_or("").to_string();
                    let mut parts = first.split(' ');
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();
                    let headers = lines
                        .filter_map(|l| l.split_once(':'))
                        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                        .collect();
                    let req = Req {
                        method,
                        path,
                        headers,
                        body: String::from_utf8_lossy(&data[head_end..]).into_owned(),
                    };
                    let resp = handler(&req);
                    log.lock().unwrap().push(req);
                    let mut reply = format!(
                        "HTTP/1.1 {} X\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n",
                        resp.status,
                        resp.content_type,
                        resp.body.len()
                    );
                    for (k, v) in &resp.headers {
                        reply.push_str(&format!("{k}: {v}\r\n"));
                    }
                    reply.push_str("\r\n");
                    reply.push_str(&resp.body);
                    let _ = socket.write_all(reply.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (format!("http://{addr}"), seen)
    }
}
