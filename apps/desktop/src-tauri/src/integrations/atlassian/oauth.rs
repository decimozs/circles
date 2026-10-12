//! Signing in through the browser, the way the MCP specification describes it: find the
//! authorization server from the MCP server, register a public client, then the authorization code
//! flow with PKCE. The browser comes back to a one-shot listener on this Mac only.

use std::time::Duration;

use base64::Engine;
use reqwest::Url;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::ipc::now_ms;

/// How long the person has to finish in the browser.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq)]
pub enum OAuthError {
    Network(String),
    /// Atlassian's answers were not what the specification says.
    Metadata(String),
    Registration(String),
    /// The person said no, or the server refused: its own words.
    Denied(String),
    Timeout,
    Cancelled,
    Token(String),
}

impl OAuthError {
    pub fn message(&self) -> String {
        match self {
            Self::Network(e) => format!("Could not reach Atlassian: {e}"),
            Self::Metadata(e) => format!("Atlassian's sign-in is not set up as expected: {e}"),
            Self::Registration(e) => format!(
                "Atlassian did not accept Circles as an app ({e}). Your organisation may block it. You can use an API token instead."
            ),
            Self::Denied(e) => format!("Sign-in was refused: {e}"),
            Self::Timeout => "Sign-in took too long. Try again.".into(),
            Self::Cancelled => "Sign-in was cancelled.".into(),
            Self::Token(e) => format!("Atlassian did not give an access token: {e}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Discovery {
    /// The MCP server the tokens are for.
    pub resource: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_ms: i64,
    pub scope: Option<String>,
}

fn net(e: reqwest::Error) -> OAuthError {
    OAuthError::Network(e.without_url().to_string())
}

pub fn client() -> Result<reqwest::Client, OAuthError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("circles")
        .build()
        .map_err(net)
}

async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value, OAuthError> {
    let response = http
        .get(url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(net)?;
    if !response.status().is_success() {
        return Err(OAuthError::Metadata(format!(
            "{url} answered {}",
            response.status().as_u16()
        )));
    }
    response
        .json()
        .await
        .map_err(|_| OAuthError::Metadata(format!("{url} did not send JSON")))
}

fn text(value: &Value, key: &str) -> Option<String> {
    value[key].as_str().map(str::to_string)
}

/// The authorization server of an MCP server, and where to register, authorize and get tokens.
pub async fn discover(http: &reqwest::Client, mcp_url: &str) -> Result<Discovery, OAuthError> {
    let url = Url::parse(mcp_url).map_err(|_| OAuthError::Metadata("bad server address".into()))?;
    let origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/');
    let resource_meta = get_json(
        http,
        &format!("{origin}/.well-known/oauth-protected-resource{path}"),
    )
    .await?;
    let issuer = resource_meta["authorization_servers"][0]
        .as_str()
        .ok_or_else(|| OAuthError::Metadata("no authorization server is named".into()))?
        .trim_end_matches('/')
        .to_string();
    let issuer_url =
        Url::parse(&issuer).map_err(|_| OAuthError::Metadata("bad authorization server".into()))?;
    let issuer_origin = issuer_url.origin().ascii_serialization();
    let issuer_path = issuer_url.path().trim_end_matches('/');
    // The specification puts the well-known part between host and path. Many servers also answer
    // with it appended, so both are tried.
    let mut found = None;
    for candidate in [
        format!("{issuer_origin}/.well-known/oauth-authorization-server{issuer_path}"),
        format!("{issuer}/.well-known/oauth-authorization-server"),
        format!("{issuer}/.well-known/openid-configuration"),
    ] {
        if let Ok(meta) = get_json(http, &candidate).await {
            found = Some(meta);
            break;
        }
    }
    let meta =
        found.ok_or_else(|| OAuthError::Metadata("no authorization server metadata".into()))?;
    let challenge_ok = meta["code_challenge_methods_supported"]
        .as_array()
        .is_none_or(|m| m.iter().any(|v| v == "S256"));
    if !challenge_ok {
        return Err(OAuthError::Metadata(
            "the server does not support PKCE (S256)".into(),
        ));
    }
    let endpoint =
        |key: &str| text(&meta, key).ok_or_else(|| OAuthError::Metadata(format!("no {key}")));
    // Only secure addresses, so a tampered answer cannot send the person to plain HTTP.
    let authorization_endpoint = endpoint("authorization_endpoint")?;
    let token_endpoint = endpoint("token_endpoint")?;
    for e in [&authorization_endpoint, &token_endpoint] {
        let loopback = e.starts_with("http://127.0.0.1") || e.starts_with("http://localhost");
        if !e.starts_with("https://") && !loopback {
            return Err(OAuthError::Metadata(
                "the server named an address that is not HTTPS".into(),
            ));
        }
    }
    let scopes = |v: &Value| -> Vec<String> {
        match &v["scopes_supported"] {
            Value::Array(items) => items
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect(),
            Value::String(s) => s.split_whitespace().map(str::to_string).collect(),
            _ => Vec::new(),
        }
    };
    let mut scopes_supported = scopes(&resource_meta);
    if scopes_supported.is_empty() {
        scopes_supported = scopes(&meta);
    }
    Ok(Discovery {
        resource: text(&resource_meta, "resource").unwrap_or_else(|| mcp_url.to_string()),
        authorization_endpoint,
        token_endpoint,
        registration_endpoint: text(&meta, "registration_endpoint"),
        scopes_supported,
    })
}

/// The scopes to ask for: Jira reading, writing and searching, and the offline one that lets the
/// sign-in be renewed. Never delete or manage, and nothing outside Jira.
pub fn pick_scopes(supported: &[String]) -> String {
    let mut chosen: Vec<&str> = supported
        .iter()
        .map(String::as_str)
        .filter(|s| {
            let lower = s.to_lowercase();
            (lower.contains("jira"))
                && !lower.contains("delete")
                && !lower.contains("manage")
                && !lower.contains("admin")
                && !lower.contains("align")
                && ["read", "write", "search"]
                    .iter()
                    .any(|p| lower.starts_with(p))
        })
        .collect();
    // Atlassian's authorization step rejects a request that does not also ask who the person is
    // (other clients that work, such as mcp-remote, ask for these). Reading the account is all they
    // allow.
    for identity in ["read:me", "read:account", "email", "offline_access"] {
        if supported.iter().any(|s| s == identity) {
            chosen.push(identity);
        }
    }
    chosen.join(" ")
}

pub fn pkce() -> (String, String) {
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Registers Circles as a public client for this redirect address.
pub async fn register(
    http: &reqwest::Client,
    endpoint: &str,
    redirect_uri: &str,
) -> Result<String, OAuthError> {
    let response = http
        .post(endpoint)
        .json(&json!({
            "client_name": "Circles",
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        }))
        .send()
        .await
        .map_err(net)?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let why = body["error_description"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("it was refused");
        return Err(OAuthError::Registration(format!(
            "{}: {why}",
            status.as_u16()
        )));
    }
    text(&body, "client_id")
        .ok_or_else(|| OAuthError::Registration("no client id came back".into()))
}

pub fn authorize_url(
    d: &Discovery,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
    scope: &str,
) -> Result<String, OAuthError> {
    let mut url = Url::parse(&d.authorization_endpoint)
        .map_err(|_| OAuthError::Metadata("bad authorization address".into()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("resource", &d.resource)
            // Atlassian rejects the request after sign-in without this.
            .append_pair("prompt", "consent");
        if !scope.is_empty() {
            q.append_pair("scope", scope);
        }
    }
    Ok(url.to_string())
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Circles</title><body style=\"font:16px system-ui;margin:3rem\"><h2>You are signed in</h2><p>You can close this tab and go back to Circles.</p>";
const FAILED_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Circles</title><body style=\"font:16px system-ui;margin:3rem\"><h2>Sign-in did not finish</h2><p>Go back to Circles to see why.</p>";

async fn respond(socket: &mut tokio::net::TcpStream, status: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = socket.write_all(reply.as_bytes()).await;
    let _ = socket.shutdown().await;
}

/// Waits for the browser to come back with the code. Anything that is not the callback with the
/// right `state` is turned away and ignored, so a stray request cannot end or hijack the sign-in.
pub async fn wait_for_callback(
    listener: TcpListener,
    state: &str,
    timeout: Duration,
    cancel: oneshot::Receiver<()>,
) -> Result<String, OAuthError> {
    let wait = async {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                continue;
            };
            let mut buf = vec![0u8; 8192];
            let n = match tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buf)).await
            {
                Ok(Ok(n)) => n,
                _ => continue,
            };
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let target = request
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("GET "))
                .and_then(|l| l.split(' ').next())
                .unwrap_or("")
                .to_string();
            let Ok(url) = Url::parse(&format!("http://localhost{target}")) else {
                respond(&mut socket, "400 Bad Request", FAILED_PAGE).await;
                continue;
            };
            if url.path() != "/callback" {
                respond(&mut socket, "404 Not Found", FAILED_PAGE).await;
                continue;
            }
            let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            if pairs.get("state").map(String::as_str) != Some(state) {
                respond(&mut socket, "400 Bad Request", FAILED_PAGE).await;
                continue;
            }
            if let Some(error) = pairs.get("error") {
                respond(&mut socket, "200 OK", FAILED_PAGE).await;
                let why = pairs.get("error_description").unwrap_or(error);
                return Err(OAuthError::Denied(why.clone()));
            }
            let Some(code) = pairs.get("code") else {
                respond(&mut socket, "400 Bad Request", FAILED_PAGE).await;
                continue;
            };
            respond(&mut socket, "200 OK", DONE_PAGE).await;
            return Ok(code.clone());
        }
    };
    tokio::select! {
        result = wait => result,
        _ = tokio::time::sleep(timeout) => Err(OAuthError::Timeout),
        _ = cancel => Err(OAuthError::Cancelled),
    }
}

fn tokens(body: &Value, previous_refresh: Option<&str>) -> Result<TokenSet, OAuthError> {
    let access = text(body, "access_token").ok_or_else(|| {
        OAuthError::Token(
            body["error_description"]
                .as_str()
                .or_else(|| body["error"].as_str())
                .unwrap_or("no access token")
                .to_string(),
        )
    })?;
    let seconds = body["expires_in"]
        .as_i64()
        .unwrap_or(3600)
        .clamp(60, 86_400);
    Ok(TokenSet {
        access_token: access,
        // A renewal may not send a new refresh token. The old one stays good then.
        refresh_token: text(body, "refresh_token").or_else(|| previous_refresh.map(str::to_string)),
        expires_at_ms: now_ms() + seconds * 1000,
        scope: text(body, "scope"),
    })
}

async fn post_form(
    http: &reqwest::Client,
    endpoint: &str,
    form: &[(&str, &str)],
) -> Result<(u16, Value), OAuthError> {
    let response = http
        .post(endpoint)
        .header("accept", "application/json")
        .form(form)
        .send()
        .await
        .map_err(net)?;
    let status = response.status().as_u16();
    Ok((status, response.json().await.unwrap_or(Value::Null)))
}

pub async fn exchange(
    http: &reqwest::Client,
    d: &Discovery,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> Result<TokenSet, OAuthError> {
    let (_, body) = post_form(
        http,
        &d.token_endpoint,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("code_verifier", verifier),
            ("resource", &d.resource),
        ],
    )
    .await?;
    tokens(&body, None)
}

/// A new access token from the refresh token. `Err(Denied)` means the sign-in is over and the
/// person has to sign in again.
pub async fn refresh(
    http: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &str,
    resource: &str,
) -> Result<TokenSet, OAuthError> {
    let (status, body) = post_form(
        http,
        token_endpoint,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("resource", resource),
        ],
    )
    .await?;
    if (400..500).contains(&status) {
        return Err(OAuthError::Denied(
            body["error_description"]
                .as_str()
                .or_else(|| body["error"].as_str())
                .unwrap_or("the sign-in has ended")
                .to_string(),
        ));
    }
    tokens(&body, Some(refresh_token))
}

/// Everything the sign-in gives back.
pub struct SignedIn {
    pub tokens: TokenSet,
    pub client_id: String,
    pub discovery: Discovery,
}

/// The whole sign-in. `hosts` are the names tried for the redirect address, in order: registration
/// is repeated with the next one when a server will not take the first. `open` shows the page in
/// the browser.
pub async fn sign_in(
    http: &reqwest::Client,
    mcp_url: &str,
    hosts: &[&str],
    open: &(dyn Fn(&str) -> Result<(), String> + Send + Sync),
    timeout: Duration,
    cancel: oneshot::Receiver<()>,
) -> Result<SignedIn, OAuthError> {
    let discovery = discover(http, mcp_url).await?;
    let registration = discovery
        .registration_endpoint
        .clone()
        .ok_or_else(|| OAuthError::Registration("the server cannot register apps".into()))?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| OAuthError::Network(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| OAuthError::Network(e.to_string()))?
        .port();
    let mut registered = None;
    let mut last = OAuthError::Registration("no address to try".into());
    for host in hosts {
        let redirect = format!("http://{host}:{port}/callback");
        match register(http, &registration, &redirect).await {
            Ok(id) => {
                registered = Some((id, redirect));
                break;
            }
            Err(e @ OAuthError::Registration(_)) => last = e,
            Err(e) => return Err(e),
        }
    }
    let (client_id, redirect) = registered.ok_or(last)?;
    let (verifier, challenge) = pkce();
    let state = format!("{}", uuid::Uuid::new_v4().simple());
    let scope = pick_scopes(&discovery.scopes_supported);
    let url = authorize_url(
        &discovery, &client_id, &redirect, &challenge, &state, &scope,
    )?;
    open(&url).map_err(|e| OAuthError::Network(format!("cannot open the browser: {e}")))?;
    let code = wait_for_callback(listener, &state, timeout, cancel).await?;
    let tokens = exchange(http, &discovery, &client_id, &redirect, &code, &verifier).await?;
    Ok(SignedIn {
        tokens,
        client_id,
        discovery,
    })
}
