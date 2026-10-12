//! A small client for the Jira Cloud REST API (v3). The address is fixed when the connection is
//! made, so no tool input can send the token anywhere else.

use std::time::Duration;

use base64::Engine;
use reqwest::{header, Method, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::integrations::secrets::{clean_site, Credentials};

/// Biggest answer read, so one huge ticket cannot fill memory.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum JiraFailure {
    BadSite,
    BadLogin,
    /// 401 or 403: the token was turned down.
    Rejected,
    NotFound,
    /// 429, with the seconds Jira asked to wait.
    Busy(Option<u64>),
    Network(String),
    Other(u16, String),
}

impl JiraFailure {
    pub fn message(&self) -> String {
        match self {
            Self::BadSite => "That is not a Jira Cloud site. Use the address like your-team.atlassian.net.".into(),
            Self::BadLogin => "Enter the email of the Atlassian account and its API token.".into(),
            Self::Rejected => "Jira did not accept the email and token. Check both, and that the token has not been revoked.".into(),
            Self::NotFound => "Jira says that does not exist, or this account cannot see it.".into(),
            Self::Busy(Some(s)) => format!("Jira is rate limiting this account. Try again in {s} seconds."),
            Self::Busy(None) => "Jira is rate limiting this account. Try again in a minute.".into(),
            Self::Network(e) => format!("Could not reach Jira: {e}"),
            Self::Other(code, text) => format!("Jira answered {code}: {text}"),
        }
    }

    pub fn is_rejection(&self) -> bool {
        matches!(self, Self::Rejected)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Myself {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

pub struct JiraClient {
    http: reqwest::Client,
    base: String,
    auth: String,
}

impl JiraClient {
    /// `base` is only given by tests.
    pub fn new(credentials: &Credentials, base: Option<&str>) -> Result<Self, JiraFailure> {
        let site = clean_site(&credentials.site)?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("circles")
            .build()
            .map_err(|e| JiraFailure::Network(e.to_string()))?;
        let pair = format!("{}:{}", credentials.email, credentials.token);
        Ok(Self {
            http,
            base: base.map_or_else(|| format!("https://{site}"), str::to_string),
            auth: format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(pair)
            ),
        })
    }

    /// One call. `path` starts with `/rest/api/3/`.
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, JiraFailure> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header(header::AUTHORIZATION, &self.auth)
            .header(header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| JiraFailure::Network(e.without_url().to_string()))?;
        let status = response.status();
        let retry = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| JiraFailure::Network(e.without_url().to_string()))?;
        if bytes.len() > BODY_LIMIT {
            return Err(JiraFailure::Other(
                status.as_u16(),
                "the answer was too large".into(),
            ));
        }
        match status {
            s if s.is_success() => {
                if bytes.is_empty() {
                    Ok(Value::Null)
                } else {
                    serde_json::from_slice(&bytes).map_err(|_| {
                        JiraFailure::Other(s.as_u16(), "the answer was not JSON".into())
                    })
                }
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(JiraFailure::Rejected),
            StatusCode::NOT_FOUND => Err(JiraFailure::NotFound),
            StatusCode::TOO_MANY_REQUESTS => Err(JiraFailure::Busy(retry)),
            s => {
                let text = String::from_utf8_lossy(&bytes);
                Err(JiraFailure::Other(
                    s.as_u16(),
                    text.chars().take(300).collect(),
                ))
            }
        }
    }

    /// Whose token this is. Also the check that a new token works.
    pub async fn myself(&self) -> Result<Myself, JiraFailure> {
        let value = self
            .request(Method::GET, "/rest/api/3/myself", None)
            .await?;
        serde_json::from_value(value).map_err(|_| {
            JiraFailure::Other(200, "the answer was not what Jira usually sends".into())
        })
    }
}
