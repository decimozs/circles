//! Signing in with Atlassian, and Jira through Atlassian's remote MCP server. The Circles Jira
//! tools stay the same. Only where their calls go changes. See `tools.rs` for the mapping.

pub mod mcp;
pub mod oauth;
pub mod tools;

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use ts_rs::TS;

use super::secrets::OAuthSession;
use super::Integrations;
use crate::error::{AppError, AppResult};
use crate::ipc::now_ms;
use mcp::{McpClient, McpError};

/// Atlassian's MCP server. `?tools=all` lists every tool, not only the main ones.
pub const MCP_URL: &str = "https://mcp.atlassian.com/v2/mcp?tools=all";

/// A Jira site the signed-in account can reach.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct JiraSite {
    pub cloud_id: String,
    pub name: String,
    pub url: String,
}

/// A started MCP session and the tools it offers, with their input schemas.
pub struct Ready {
    client: McpClient,
    schemas: HashMap<String, Value>,
}

/// Signed in, and waiting for the person to pick a site.
pub struct Pending {
    session: OAuthSession,
    sites: Vec<JiraSite>,
}

#[derive(Default)]
pub struct Runtime {
    ready: tokio::sync::Mutex<Option<Arc<Ready>>>,
    /// Renewing the access token happens one at a time.
    refreshing: tokio::sync::Mutex<()>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// Reads what a tool said as JSON when it is JSON, and as text when it is not.
pub fn parse_text(text: &str) -> Value {
    serde_json::from_str(text.trim()).unwrap_or_else(|_| Value::String(text.to_string()))
}

/// Atlassian wraps most answers as `{"data": ...}`. The inside is what matters.
pub fn unwrap_data(value: Value) -> Value {
    match value {
        Value::Object(mut map)
            if matches!(map.get("data"), Some(Value::Object(_) | Value::Array(_))) =>
        {
            map.remove("data").unwrap_or(Value::Null)
        }
        other => other,
    }
}

/// The Jira sites in what `getAccessibleAtlassianResources` said. It may be a list, an object that
/// holds one, or text around either. A site is anything with an id; the sign-in only asked for
/// Jira, so nothing is filtered by the scopes it lists.
pub fn sites_from(text: &str) -> Vec<JiraSite> {
    let mut value = parse_text(text);
    if let Value::String(raw) = &value {
        // JSON inside prose: take the widest list or object in it.
        for (open, close) in [('[', ']'), ('{', '}')] {
            if let (Some(start), Some(end)) = (raw.find(open), raw.rfind(close)) {
                if let Ok(found) = serde_json::from_str::<Value>(&raw[start..=end]) {
                    value = found;
                    break;
                }
            }
        }
    }
    let value = unwrap_data(value);
    let list = value
        .as_array()
        .or_else(|| value["resources"].as_array())
        .or_else(|| value["sites"].as_array())
        .or_else(|| value["accessibleResources"].as_array())
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|r| {
            let cloud_id = r["id"].as_str().or_else(|| r["cloudId"].as_str())?;
            // When Atlassian says which products a site has, it has to include Jira.
            if let Some(products) = r["products"].as_array() {
                let has_jira = products.iter().any(|p| {
                    p["id"]
                        .as_str()
                        .is_some_and(|id| id.to_lowercase().contains("jira"))
                });
                if !has_jira {
                    return None;
                }
            }
            let url = r["url"].as_str().unwrap_or_default();
            let name = r["name"]
                .as_str()
                .filter(|n| !n.is_empty())
                .map_or_else(|| host_of(url), str::to_string);
            Some(JiraSite {
                cloud_id: cloud_id.to_string(),
                name,
                url: url.to_string(),
            })
        })
        .collect()
}

fn host_of(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string()
}

impl Integrations {
    fn mcp_url(&self) -> String {
        self.mcp_url_override
            .clone()
            .unwrap_or_else(|| MCP_URL.to_string())
    }

    /// The saved sign-in, read from the store once.
    pub(crate) fn load_oauth(&self) -> AppResult<Option<OAuthSession>> {
        let mut slot = self.oauth.lock().unwrap();
        if !slot.loaded {
            slot.session = self.store.load_oauth()?;
            slot.loaded = true;
        }
        Ok(slot.session.clone())
    }

    /// Signed in with Atlassian, known without asking the Keychain.
    pub(crate) fn is_oauth(&self) -> bool {
        self.current_meta()
            .is_some_and(|m| m.method.as_deref() == Some("oauth"))
    }

    pub(crate) fn oauth_needs_reconnect(&self) -> bool {
        self.oauth.lock().unwrap().needs_reconnect || self.is_rejected()
    }

    pub(crate) fn mark_oauth_rejected(&self) {
        self.oauth.lock().unwrap().needs_reconnect = true;
        self.rejected
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// An access token that is good for at least a minute, renewed when it is not.
    async fn oauth_token(&self, force: bool) -> AppResult<String> {
        let session = self.load_oauth()?.ok_or_else(|| {
            invalid("Jira is not connected. Connect it in Settings, Integrations.")
        })?;
        if self.oauth_needs_reconnect() {
            return Err(invalid(
                "Jira needs to be reconnected in Settings, Integrations.",
            ));
        }
        if !force && session.expires_at_ms - now_ms() > 60_000 {
            return Ok(session.access_token);
        }
        let _one_at_a_time = self.atlassian.refreshing.lock().await;
        // Someone else may have renewed it while this waited.
        let session = self.load_oauth()?.unwrap_or(session);
        if !force && session.expires_at_ms - now_ms() > 60_000 {
            return Ok(session.access_token);
        }
        let Some(refresh) = session.refresh_token.clone() else {
            self.mark_oauth_rejected();
            return Err(invalid(
                "The Jira sign-in ended. Reconnect Jira in Settings, Integrations.",
            ));
        };
        let http = oauth::client().map_err(|e| invalid(e.message()))?;
        match oauth::refresh(
            &http,
            &session.token_endpoint,
            &session.client_id,
            &refresh,
            &session.resource,
        )
        .await
        {
            Ok(tokens) => {
                let renewed = OAuthSession {
                    access_token: tokens.access_token.clone(),
                    refresh_token: tokens.refresh_token,
                    expires_at_ms: tokens.expires_at_ms,
                    ..session
                };
                self.store.save_oauth(&renewed)?;
                self.oauth.lock().unwrap().session = Some(renewed);
                Ok(tokens.access_token)
            }
            Err(oauth::OAuthError::Denied(_)) => {
                self.mark_oauth_rejected();
                Err(invalid(
                    "The Jira sign-in ended. Reconnect Jira in Settings, Integrations.",
                ))
            }
            Err(e) => Err(invalid(e.message())),
        }
    }

    async fn start_ready(&self, token: &str) -> Result<Arc<Ready>, McpError> {
        let client = McpClient::new(&self.mcp_url())?;
        client.initialize(token).await?;
        let tools = client.list_tools(token).await?;
        let schemas = tools
            .iter()
            .filter_map(|t| Some((t["name"].as_str()?.to_string(), t["inputSchema"].clone())))
            .collect();
        Ok(Arc::new(Ready { client, schemas }))
    }

    /// One call to a tool of Atlassian's server. The arguments are checked against the schema the
    /// server gave, so a tool that changed fails with a plain message instead of a wrong call.
    pub async fn call_atlassian(&self, name: &str, args: Value) -> AppResult<Value> {
        let mut retried = false;
        loop {
            let token = self.oauth_token(retried).await?;
            let ready = {
                let mut slot = self.atlassian.ready.lock().await;
                if slot.is_none() {
                    match self.start_ready(&token).await {
                        Ok(ready) => *slot = Some(ready),
                        Err(McpError::Unauthorized) if !retried => {
                            retried = true;
                            continue;
                        }
                        Err(McpError::Unauthorized) => {
                            self.mark_oauth_rejected();
                            return Err(invalid("Atlassian turned the sign-in down. Reconnect Jira in Settings, Integrations."));
                        }
                        Err(e) => return Err(invalid(e.message())),
                    }
                }
                slot.clone().expect("just set")
            };
            tools::check_args(&ready.schemas, name, &args)?;
            match ready.client.call_tool(&token, name, &args).await {
                Ok(result) if result.is_error => return Err(invalid(result.text)),
                Ok(result) => return Ok(unwrap_data(parse_text(&result.text))),
                Err(McpError::SessionLost) if !retried => {
                    retried = true;
                    *self.atlassian.ready.lock().await = None;
                }
                Err(McpError::Unauthorized) if !retried => {
                    retried = true;
                    *self.atlassian.ready.lock().await = None;
                }
                Err(McpError::Unauthorized) => {
                    self.mark_oauth_rejected();
                    return Err(invalid(
                        "Atlassian turned the sign-in down. Reconnect Jira in Settings, Integrations.",
                    ));
                }
                Err(e) => return Err(invalid(e.message())),
            }
        }
    }

    /// Browser sign-in. `open` shows a page in the browser. Returns the status: connected, or
    /// waiting for a site to be chosen when the account reaches several.
    pub async fn jira_sign_in(
        &self,
        open: &(dyn Fn(&str) -> Result<(), String> + Send + Sync),
    ) -> AppResult<super::JiraStatus> {
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        *self.cancel.lock().unwrap() = Some(cancel_tx);
        let http = oauth::client().map_err(|e| invalid(e.message()))?;
        let hosts: Vec<&str> = self.redirect_hosts.iter().map(String::as_str).collect();
        let result = oauth::sign_in(
            &http,
            &self.mcp_url(),
            &hosts,
            open,
            oauth::SIGN_IN_TIMEOUT,
            cancel_rx,
        )
        .await;
        *self.cancel.lock().unwrap() = None;
        let signed = result.map_err(|e| invalid(e.message()))?;

        // Ask the server who this is and which sites it reaches, with a session of its own.
        let token = signed.tokens.access_token.clone();
        let client = McpClient::new(&self.mcp_url()).map_err(|e| invalid(e.message()))?;
        client
            .initialize(&token)
            .await
            .map_err(|e| invalid(e.message()))?;
        let resources = client
            .call_tool(&token, "getAccessibleAtlassianResources", &json!({}))
            .await
            .map_err(|e| invalid(e.message()))?;
        if resources.is_error {
            return Err(invalid(resources.text));
        }
        let sites = sites_from(&resources.text);
        let account_name = match client
            .call_tool(&token, "atlassianUserInfo", &json!({}))
            .await
        {
            Ok(info) if !info.is_error => {
                let v = unwrap_data(parse_text(&info.text));
                v["name"]
                    .as_str()
                    .or_else(|| v["displayName"].as_str())
                    .or_else(|| v["user"]["name"].as_str())
                    .or_else(|| v["email"].as_str())
                    .or_else(|| v["user"]["email"].as_str())
                    .unwrap_or("your account")
                    .to_string()
            }
            _ => "your account".to_string(),
        };
        let base = OAuthSession {
            client_id: signed.client_id,
            token_endpoint: signed.discovery.token_endpoint,
            resource: signed.discovery.resource,
            access_token: signed.tokens.access_token,
            refresh_token: signed.tokens.refresh_token,
            expires_at_ms: signed.tokens.expires_at_ms,
            cloud_id: String::new(),
            site: String::new(),
            account_name,
        };
        match sites.len() {
            0 => {
                // What Atlassian said about the sites holds no secret, and it is what is needed
                // to see why none was found.
                let said: String = resources.text.chars().take(400).collect();
                tracing::warn!(answer = %said, "the sign-in found no Jira site");
                Err(invalid(format!(
                    "Atlassian did not list a Jira site for this account. Atlassian said: {said}"
                )))
            }
            1 => self.finish_sign_in(base, &sites[0]),
            _ => {
                *self.pending.lock().unwrap() = Some(Pending {
                    session: base,
                    sites,
                });
                self.jira_status()
            }
        }
    }

    /// Stops a sign-in that is waiting for the browser.
    pub fn jira_cancel_sign_in(&self) {
        if let Some(cancel) = self.cancel.lock().unwrap().take() {
            let _ = cancel.send(());
        }
    }

    /// The person picked a site after signing in.
    pub fn jira_choose_site(&self, cloud_id: &str) -> AppResult<super::JiraStatus> {
        // A wrong choice leaves the sign-in waiting, so the person can choose again.
        let (session, site) = {
            let pending = self.pending.lock().unwrap();
            let pending = pending
                .as_ref()
                .ok_or_else(|| invalid("There is no sign-in waiting. Sign in again."))?;
            let site = pending
                .sites
                .iter()
                .find(|s| s.cloud_id == cloud_id)
                .cloned()
                .ok_or_else(|| invalid("That is not one of the sites."))?;
            (pending.session.clone(), site)
        };
        self.finish_sign_in(session, &site)
    }

    fn finish_sign_in(&self, base: OAuthSession, site: &JiraSite) -> AppResult<super::JiraStatus> {
        let session = OAuthSession {
            cloud_id: site.cloud_id.clone(),
            site: host_of(&site.url),
            ..base
        };
        self.store.save_oauth(&session)?;
        // One way of being connected at a time.
        self.store.delete()?;
        *self.known.lock().unwrap() = None;
        self.set_meta(Some(super::meta::Meta {
            method: Some("oauth".into()),
            site: Some(session.site.clone()),
            email: None,
            account_name: Some(session.account_name.clone()),
        }));
        *self.pending.lock().unwrap() = None;
        *self.oauth.lock().unwrap() = super::OAuthSlot {
            loaded: true,
            session: Some(session),
            needs_reconnect: false,
        };
        self.atlassian.ready.try_lock().map(|mut r| *r = None).ok();
        self.jira_status()
    }

    pub(crate) fn pending_sites(&self) -> Vec<JiraSite> {
        self.pending
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| p.sites.clone())
            .unwrap_or_default()
    }

    pub(crate) fn clear_oauth(&self) -> AppResult<()> {
        self.store.delete_oauth()?;
        *self.oauth.lock().unwrap() = super::OAuthSlot {
            loaded: true,
            session: None,
            needs_reconnect: false,
        };
        *self.pending.lock().unwrap() = None;
        self.atlassian.ready.try_lock().map(|mut r| *r = None).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::secrets::{MemorySecrets, SecretStore};
    use crate::integrations::testing::{fake_http, Req, Resp};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    /// A pretend Atlassian: metadata, registration, tokens and an MCP server in one.
    struct Atlassian {
        base: OnceLock<String>,
        sites: usize,
        /// The access token the MCP server accepts right now.
        valid: Mutex<String>,
        refresh_works: bool,
        refreshes: AtomicUsize,
        /// The registration is refused for redirect addresses that contain this.
        refuse_host: Option<&'static str>,
        tool_properties: Value,
        stream: bool,
    }

    impl Atlassian {
        fn new() -> Self {
            Self {
                base: OnceLock::new(),
                sites: 1,
                valid: Mutex::new("at1".into()),
                refresh_works: true,
                refreshes: AtomicUsize::new(0),
                refuse_host: None,
                tool_properties: json!({
                    "cloudId": {}, "jql": {}, "maxResults": {}, "fields": {}, "nextPageToken": {}
                }),
                stream: false,
            }
        }

        fn handle(&self, req: &Req) -> Resp {
            let base = self.base.get().cloned().unwrap_or_default();
            let path = req.path.split('?').next().unwrap_or("");
            match (req.method.as_str(), path) {
                ("GET", "/.well-known/oauth-protected-resource/v2/mcp") => Resp::json(json!({
                    "resource": format!("{base}/v2/mcp"),
                    "authorization_servers": [format!("{base}/as")],
                    "scopes_supported": ["read:jira-work", "write:jira-work", "search:jira-work",
                        "delete:jira-work", "manage:jira-project", "read:confluence-content.all", "offline_access"],
                })),
                ("GET", "/.well-known/oauth-authorization-server/as") => Resp::json(json!({
                    "authorization_endpoint": format!("{base}/authorize"),
                    "token_endpoint": format!("{base}/token"),
                    "registration_endpoint": format!("{base}/register"),
                    "code_challenge_methods_supported": ["S256"],
                })),
                ("POST", "/register") => {
                    if self.refuse_host.is_some_and(|h| req.body.contains(h)) {
                        return Resp::status(400, json!({"error": "invalid_redirect_uri"}));
                    }
                    Resp::json(json!({ "client_id": "cid" }))
                }
                ("POST", "/token") => {
                    if req.body.contains("grant_type=refresh_token") {
                        self.refreshes.fetch_add(1, Ordering::SeqCst);
                        if !self.refresh_works {
                            return Resp::status(400, json!({"error": "invalid_grant"}));
                        }
                        *self.valid.lock().unwrap() = "at2".into();
                        return Resp::json(
                            json!({"access_token": "at2", "refresh_token": "rt2", "expires_in": 3600}),
                        );
                    }
                    Resp::json(
                        json!({"access_token": "at1", "refresh_token": "rt1", "expires_in": 3600}),
                    )
                }
                ("POST", "/v2/mcp") => self.mcp(req),
                _ => Resp::status(404, json!({})),
            }
        }

        fn mcp(&self, req: &Req) -> Resp {
            let token = req
                .headers
                .get("authorization")
                .cloned()
                .unwrap_or_default();
            if token != format!("Bearer {}", self.valid.lock().unwrap()) {
                return Resp::status(401, json!({}));
            }
            let body: Value = serde_json::from_str(&req.body).unwrap_or(Value::Null);
            let id = body["id"].clone();
            let reply = |result: Value| {
                let message = json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
                if self.stream {
                    Resp {
                        status: 200,
                        content_type: "text/event-stream",
                        headers: vec![("mcp-session-id", "s1".into())],
                        body: format!("event: message\ndata: {message}\n\n"),
                    }
                } else {
                    Resp {
                        status: 200,
                        content_type: "application/json",
                        headers: vec![("mcp-session-id", "s1".into())],
                        body: message,
                    }
                }
            };
            let text = |v: Value| json!({ "content": [{"type": "text", "text": v.to_string()}] });
            match body["method"].as_str() {
                Some("initialize") => {
                    reply(json!({"protocolVersion": "2025-06-18", "capabilities": {}}))
                }
                Some("notifications/initialized") => Resp {
                    status: 202,
                    content_type: "application/json",
                    headers: vec![],
                    body: String::new(),
                },
                Some("tools/list") => reply(json!({ "tools": [
                    {"name": "getAccessibleAtlassianResources", "inputSchema": {"type": "object", "properties": {}}},
                    {"name": "atlassianUserInfo", "inputSchema": {"type": "object", "properties": {}}},
                    {"name": "searchJiraIssuesUsingJql", "inputSchema": {
                        "type": "object", "properties": self.tool_properties, "required": ["cloudId", "jql"]}},
                ]})),
                Some("tools/call") => {
                    let args = &body["params"]["arguments"];
                    match body["params"]["name"].as_str() {
                        Some("getAccessibleAtlassianResources") => {
                            let sites: Vec<Value> = (0..self.sites)
                                .map(|i| json!({"id": format!("cloud{i}"), "url": format!("https://team{i}.atlassian.net"), "name": format!("Team {i}"), "scopes": ["read:jira-work"]}))
                                .collect();
                            reply(text(json!(sites)))
                        }
                        Some("atlassianUserInfo") => {
                            reply(text(json!({"name": "Marlon", "account_id": "acc1"})))
                        }
                        Some("searchJiraIssuesUsingJql") => {
                            assert_eq!(args["cloudId"], "cloud0");
                            reply(text(json!({"issues": [{"key": "PROJ-1", "fields": {
                                "summary": "Fix it", "status": {"name": "To Do"}, "updated": "2026-01-01"}}]})))
                        }
                        _ => reply(
                            json!({"content": [{"type": "text", "text": "no such tool"}], "isError": true}),
                        ),
                    }
                }
                _ => Resp::status(400, json!({})),
            }
        }
    }

    async fn serve(fake: Atlassian) -> (String, Arc<Atlassian>, Arc<Mutex<Vec<Req>>>) {
        let fake = Arc::new(fake);
        let handler = fake.clone();
        let (base, seen) = fake_http(move |req| handler.handle(req)).await;
        fake.base.set(base.clone()).unwrap();
        (format!("{base}/v2/mcp?tools=all"), fake, seen)
    }

    /// The browser: it opens the address it is given and goes to where it is sent back.
    fn browser(
        stray_first: bool,
        denied: bool,
    ) -> impl Fn(&str) -> Result<(), String> + Send + Sync {
        move |url: &str| {
            let url = reqwest::Url::parse(url).unwrap();
            let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(q["code_challenge_method"], "S256");
            assert_eq!(q["client_id"], "cid");
            assert_eq!(q["prompt"], "consent");
            assert_eq!(q["resource"].split("/v2/mcp").count(), 2);
            // Delete and manage are never asked for.
            let scope = q["scope"].clone();
            assert!(
                scope.contains("read:jira-work") && scope.contains("offline_access"),
                "{scope}"
            );
            assert!(
                !scope.contains("delete")
                    && !scope.contains("manage")
                    && !scope.contains("confluence"),
                "{scope}"
            );
            let redirect = q["redirect_uri"].clone();
            let state = q["state"].clone();
            tokio::spawn(async move {
                let http = reqwest::Client::new();
                if stray_first {
                    let _ = http
                        .get(format!("{redirect}?code=evil&state=wrong"))
                        .send()
                        .await;
                    let _ = http
                        .get(redirect.replace("/callback", "/other"))
                        .send()
                        .await;
                }
                let tail = if denied {
                    format!("error=access_denied&error_description=nope&state={state}")
                } else {
                    format!("code=abc&state={state}")
                };
                let _ = http.get(format!("{redirect}?{tail}")).send().await;
            });
            Ok(())
        }
    }

    fn hub(url: String) -> (Arc<Integrations>, Arc<MemorySecrets>) {
        let secrets = Arc::new(MemorySecrets::default());
        struct Shared(Arc<MemorySecrets>);
        impl SecretStore for Shared {
            fn load(&self) -> AppResult<Option<crate::integrations::secrets::Credentials>> {
                self.0.load()
            }
            fn save(&self, c: &crate::integrations::secrets::Credentials) -> AppResult<()> {
                self.0.save(c)
            }
            fn delete(&self) -> AppResult<()> {
                self.0.delete()
            }
            fn load_oauth(&self) -> AppResult<Option<OAuthSession>> {
                self.0.load_oauth()
            }
            fn save_oauth(&self, s: &OAuthSession) -> AppResult<()> {
                self.0.save_oauth(s)
            }
            fn delete_oauth(&self) -> AppResult<()> {
                self.0.delete_oauth()
            }
        }
        (
            Integrations::for_test_oauth(Box::new(Shared(secrets.clone())), url, &["127.0.0.1"]),
            secrets,
        )
    }

    #[tokio::test]
    async fn signing_in_registers_asks_for_jira_only_and_saves_the_session() {
        let (url, _fake, seen) = serve(Atlassian::new()).await;
        let (hub, secrets) = hub(url);
        let status = hub.jira_sign_in(&browser(false, false)).await.unwrap();
        assert!(status.connected);
        assert_eq!(status.method.as_deref(), Some("oauth"));
        assert_eq!(status.site.as_deref(), Some("team0.atlassian.net"));
        assert_eq!(status.account_name.as_deref(), Some("Marlon"));
        let saved = secrets.load_oauth().unwrap().unwrap();
        assert_eq!(saved.cloud_id, "cloud0");
        assert_eq!(saved.refresh_token.as_deref(), Some("rt1"));
        assert!(
            secrets.load().unwrap().is_none(),
            "no token form is kept alongside"
        );
        let seen = seen.lock().unwrap();
        let register = seen.iter().find(|r| r.path == "/register").unwrap();
        assert!(register
            .body
            .contains(r#""token_endpoint_auth_method":"none""#));
        assert!(register.body.contains("http://127.0.0.1:"));
        let token = seen.iter().find(|r| r.path == "/token").unwrap();
        assert!(token.body.contains("code_verifier=") && token.body.contains("code=abc"));
        assert!(!format!("{saved:?}").contains("at1"));
    }

    #[tokio::test]
    async fn a_stray_or_forged_request_does_not_end_the_sign_in() {
        let (url, _fake, _seen) = serve(Atlassian::new()).await;
        let (hub, _secrets) = hub(url);
        let status = hub.jira_sign_in(&browser(true, false)).await.unwrap();
        assert!(status.connected);
    }

    #[test]
    fn sites_are_found_in_any_shape_atlassian_may_use() {
        let one =
            r#"[{"id":"c1","url":"https://a.atlassian.net","name":"A","scopes":["read:me"]}]"#;
        assert_eq!(
            sites_from(one)[0].cloud_id,
            "c1",
            "scopes do not hide a site"
        );
        let wrapped = r#"{"resources":[{"cloudId":"c2","url":"https://b.atlassian.net"}]}"#;
        let sites = sites_from(wrapped);
        assert_eq!(
            (sites[0].cloud_id.as_str(), sites[0].name.as_str()),
            ("c2", "b.atlassian.net")
        );
        let prose = format!("Here are your sites:\n{one}\nLet me know.");
        assert_eq!(sites_from(&prose).len(), 1);
        // What Atlassian really sends: wrapped in data, no name, products instead of scopes.
        let real = r#"{"data":{"resources":[{"cloudId":"fed","url":"https://rosterbots.atlassian.net","products":[{"id":"jira","access":"read-write"}]}]}}"#;
        let sites = sites_from(real);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].cloud_id, "fed");
        assert_eq!(sites[0].name, "rosterbots.atlassian.net");
        // A site with only other products is not a Jira site.
        let other = r#"{"data":{"resources":[{"cloudId":"x","url":"https://x.atlassian.net","products":[{"id":"confluence"}]}]}}"#;
        assert!(sites_from(other).is_empty());
        assert!(sites_from("nothing here").is_empty());
        assert!(sites_from("[]").is_empty());
    }

    #[tokio::test]
    async fn a_refusal_in_the_browser_says_so_and_saves_nothing() {
        let (url, _fake, _seen) = serve(Atlassian::new()).await;
        let (hub, secrets) = hub(url);
        let err = hub.jira_sign_in(&browser(false, true)).await.unwrap_err();
        assert!(err.to_string().contains("refused"), "{err}");
        assert!(secrets.load_oauth().unwrap().is_none());
        assert!(!hub.jira_status().unwrap().connected);
    }

    #[tokio::test]
    async fn waiting_ends_on_cancel_and_on_time() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (_keep, rx) = tokio::sync::oneshot::channel::<()>();
        let err = oauth::wait_for_callback(listener, "s", Duration::from_millis(50), rx)
            .await
            .unwrap_err();
        assert_eq!(err, oauth::OAuthError::Timeout);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tx.send(()).unwrap();
        let err = oauth::wait_for_callback(listener, "s", Duration::from_secs(5), rx)
            .await
            .unwrap_err();
        assert_eq!(err, oauth::OAuthError::Cancelled);
    }

    #[tokio::test]
    async fn a_refused_redirect_name_is_followed_by_the_next_one() {
        let (url, _fake, seen) = serve(Atlassian {
            refuse_host: Some("localhost"),
            ..Atlassian::new()
        })
        .await;
        let secrets = MemorySecrets::default();
        let hub = Integrations::for_test_oauth(Box::new(secrets), url, &["localhost", "127.0.0.1"]);
        let status = hub.jira_sign_in(&browser(false, false)).await.unwrap();
        assert!(status.connected);
        let registrations = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.path == "/register")
            .count();
        assert_eq!(registrations, 2);
    }

    #[tokio::test]
    async fn several_sites_wait_for_a_choice() {
        let (url, _fake, _seen) = serve(Atlassian {
            sites: 2,
            ..Atlassian::new()
        })
        .await;
        let (hub, secrets) = hub(url);
        let status = hub.jira_sign_in(&browser(false, false)).await.unwrap();
        assert!(!status.connected);
        assert_eq!(status.choose_site.len(), 2);
        assert!(
            secrets.load_oauth().unwrap().is_none(),
            "nothing is saved before the choice"
        );
        assert!(hub.jira_choose_site("nope").is_err());
        let status = hub.jira_choose_site("cloud1").unwrap();
        assert!(status.connected);
        assert_eq!(status.site.as_deref(), Some("team1.atlassian.net"));
    }

    async fn signed_in(
        fake: Atlassian,
    ) -> (
        Arc<Integrations>,
        Arc<Atlassian>,
        Arc<Mutex<Vec<Req>>>,
        Arc<MemorySecrets>,
    ) {
        let (url, fake, seen) = serve(fake).await;
        let (hub, secrets) = hub(url);
        hub.jira_sign_in(&browser(false, false)).await.unwrap();
        (hub, fake, seen, secrets)
    }

    #[tokio::test]
    async fn a_circles_tool_runs_as_the_atlassian_tool_and_is_shaped_the_same() {
        let (hub, _fake, seen, _s) = signed_in(Atlassian::new()).await;
        let out = crate::integrations::jira::tools::run(
            &hub,
            "jira_search",
            &json!({"jql": "project = PROJ"}),
        )
        .await
        .unwrap();
        assert_eq!(out["issues"][0]["key"], "PROJ-1");
        assert_eq!(out["issues"][0]["status"], "To Do");
        assert_eq!(out["issues"][0]["summary"], "Fix it");
        let seen = seen.lock().unwrap();
        let call = seen
            .iter()
            .rev()
            .find(|r| r.body.contains("searchJiraIssuesUsingJql"))
            .unwrap();
        assert_eq!(call.headers["authorization"], "Bearer at1");
        assert_eq!(call.headers["mcp-session-id"], "s1");
        assert!(call.body.contains(r#""cloudId":"cloud0""#));
    }

    #[tokio::test]
    async fn answers_sent_as_an_event_stream_are_read() {
        let (hub, _f, _s, _x) = signed_in(Atlassian {
            stream: true,
            ..Atlassian::new()
        })
        .await;
        let out = crate::integrations::jira::tools::run(&hub, "jira_search", &json!({"jql": "x"}))
            .await
            .unwrap();
        assert_eq!(out["issues"][0]["key"], "PROJ-1");
    }

    #[tokio::test]
    async fn a_tool_that_atlassian_changed_fails_plainly_and_sends_nothing() {
        let (hub, _f, seen, _s) = signed_in(Atlassian {
            tool_properties: json!({ "cloudId": {}, "query": {} }),
            ..Atlassian::new()
        })
        .await;
        let err = crate::integrations::jira::tools::run(&hub, "jira_search", &json!({"jql": "x"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Atlassian changed"), "{err}");
        assert!(!seen
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.body.contains(r#""name":"searchJiraIssuesUsingJql""#)));
        // A tool Circles has no mapping for is simply unreachable.
        let err = crate::integrations::jira::tools::run(&hub, "deleteJiraIssue", &json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no Jira tool"));
    }

    #[tokio::test]
    async fn an_expired_token_is_renewed_first_and_the_new_one_is_kept() {
        let (hub, fake, _seen, secrets) = signed_in(Atlassian::new()).await;
        // Make the saved token look old.
        let mut session = secrets.load_oauth().unwrap().unwrap();
        session.expires_at_ms = 1;
        secrets.save_oauth(&session).unwrap();
        hub.oauth.lock().unwrap().session = Some(session);
        crate::integrations::jira::tools::run(&hub, "jira_search", &json!({"jql": "x"}))
            .await
            .unwrap();
        assert_eq!(fake.refreshes.load(Ordering::SeqCst), 1);
        let kept = secrets.load_oauth().unwrap().unwrap();
        assert_eq!(kept.access_token, "at2");
        assert_eq!(kept.refresh_token.as_deref(), Some("rt2"));
    }

    #[tokio::test]
    async fn a_token_the_server_turns_down_is_renewed_once_and_the_call_retried() {
        let (hub, fake, _seen, _s) = signed_in(Atlassian::new()).await;
        *fake.valid.lock().unwrap() = "somethingelse".into();
        // The refresh makes at2 the valid one.
        crate::integrations::jira::tools::run(&hub, "jira_search", &json!({"jql": "x"}))
            .await
            .unwrap();
        assert_eq!(fake.refreshes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn when_the_sign_in_cannot_be_renewed_jira_asks_to_be_reconnected() {
        let (hub, _fake, _seen, secrets) = signed_in(Atlassian {
            refresh_works: false,
            ..Atlassian::new()
        })
        .await;
        let mut session = secrets.load_oauth().unwrap().unwrap();
        session.expires_at_ms = 1;
        hub.oauth.lock().unwrap().session = Some(session);
        let err = crate::integrations::jira::tools::run(&hub, "jira_search", &json!({"jql": "x"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Reconnect"), "{err}");
        assert!(hub.jira_status().unwrap().needs_reconnect);
        assert!(!hub.is_jira_connected());
    }

    #[tokio::test]
    async fn disconnecting_removes_the_sign_in_and_connecting_with_a_token_replaces_it() {
        let (hub, _f, _s, secrets) = signed_in(Atlassian::new()).await;
        let status = hub.jira_disconnect().unwrap();
        assert!(!status.connected);
        assert!(secrets.load_oauth().unwrap().is_none());
    }

    /// J7.0, the spike. Talks to the real Atlassian and opens the browser for a real sign-in, so it is
    /// not run with the other tests. Run it with
    /// `SPIKE_HOST=localhost SPIKE_OUT=/tmp/atlassian-tools.json cargo test spike_real_atlassian -- --ignored --nocapture`
    /// and try `SPIKE_HOST=127.0.0.1` as well.
    #[tokio::test]
    #[ignore]
    async fn spike_real_atlassian() {
        let host = std::env::var("SPIKE_HOST").unwrap_or_else(|_| "localhost".into());
        let http = oauth::client().unwrap();
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        let open = |url: &str| {
            println!("OPENING: {url}");
            std::process::Command::new("open")
                .arg(url)
                .status()
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        let discovery = oauth::discover(&http, MCP_URL).await.unwrap();
        println!("DISCOVERY: {discovery:?}");
        println!(
            "SCOPES CHOSEN: {}",
            oauth::pick_scopes(&discovery.scopes_supported)
        );
        let signed = match oauth::sign_in(
            &http,
            MCP_URL,
            &[host.as_str()],
            &open,
            oauth::SIGN_IN_TIMEOUT,
            rx,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => panic!("SIGN-IN FAILED: {e:?} / {}", e.message()),
        };
        println!(
            "SIGNED IN. scope granted: {:?}, refresh token: {}",
            signed.tokens.scope,
            signed.tokens.refresh_token.is_some()
        );
        let token = signed.tokens.access_token;
        let client = McpClient::new(MCP_URL).unwrap();
        client.initialize(&token).await.unwrap();
        let tools = client.list_tools(&token).await.unwrap();
        println!("TOOLS: {}", tools.len());
        if let Ok(out) = std::env::var("SPIKE_OUT") {
            std::fs::write(out, serde_json::to_string_pretty(&tools).unwrap()).unwrap();
        }
        let sites = client
            .call_tool(&token, "getAccessibleAtlassianResources", &json!({}))
            .await
            .unwrap();
        println!(
            "SITES: {}",
            sites.text.chars().take(600).collect::<String>()
        );
    }
}
