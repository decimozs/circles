//! What is known about a connection without any secret in it: how it is connected, to which site
//! and as whom. It lives in a plain file so the app can show the state without asking the
//! Keychain, which would make macOS ask for a password every time the app starts.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    /// `oauth` or `token`. Nothing when nothing is connected.
    pub method: Option<String>,
    pub site: Option<String>,
    pub email: Option<String>,
    pub account_name: Option<String>,
}

impl Meta {
    pub fn connected(&self) -> bool {
        self.method.is_some()
    }
}

/// `None` when there is no file, or it cannot be read.
pub fn read(path: &Path) -> Option<Meta> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write(path: &Path, meta: &Meta) {
    if let Ok(text) = serde_json::to_string_pretty(meta) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    }
}
