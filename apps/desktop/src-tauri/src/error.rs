use serde::{Serialize, Serializer};
use ts_rs::TS;

use crate::git::GitError;
use crate::github::GhError;

/// Stable error shape sent to the frontend: `{ code, message, details }`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[ts(type = "unknown | null")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("settings error: {0}")]
    Settings(String),
    #[error("environment error: {0}")]
    Env(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    AlreadyExists(String),
    /// The `claude` command is not installed or not on the PATH.
    #[error("{0}")]
    ClaudeMissing(String),
    #[error("{0}")]
    Git(GitError),
    #[error("{0}")]
    Gh(GhError),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Machine readable code. Part of the IPC contract, do not rename lightly.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Db(_) => "db",
            AppError::Settings(_) => "settings",
            AppError::Env(_) => "env",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::AlreadyExists(_) => "already_exists",
            AppError::ClaudeMissing(_) => "claude_missing",
            AppError::Git(_) => "git",
            AppError::Gh(_) => "gh",
            AppError::Internal(_) => "internal",
        }
    }

    pub fn payload(&self) -> ErrorPayload {
        let details = match self {
            AppError::Git(e) => Some(e.details()),
            AppError::Gh(e) => Some(e.details()),
            _ => None,
        };
        ErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
            details,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

impl From<GitError> for AppError {
    fn from(e: GitError) -> Self {
        AppError::Git(e)
    }
}

impl From<GhError> for AppError {
    fn from(e: GhError) -> Self {
        AppError::Gh(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_stable_shape() {
        let err = AppError::NotFound("project p1".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "not_found");
        assert_eq!(json["message"], "not found: project p1");
        assert!(json["details"].is_null());
    }

    #[test]
    fn git_and_gh_errors_carry_their_kind() {
        use crate::git::{GitError, GitErrorKind};
        let git = AppError::Git(GitError::new(GitErrorKind::DirtyTree, "dirty"));
        let json = serde_json::to_value(&git).unwrap();
        assert_eq!(json["code"], "git");
        assert_eq!(json["details"]["kind"], "dirty_tree");

        let gh = AppError::Gh(crate::github::classify("gh auth login"));
        let json = serde_json::to_value(&gh).unwrap();
        assert_eq!(json["code"], "gh");
        assert_eq!(json["details"]["kind"], "not_authenticated");
        assert_eq!(json["details"]["fix"], "gh auth login");
    }

    #[test]
    fn codes_are_distinct() {
        let codes = [
            AppError::Settings(String::new()).code(),
            AppError::Env(String::new()).code(),
            AppError::NotFound(String::new()).code(),
            AppError::Invalid(String::new()).code(),
            AppError::AlreadyExists(String::new()).code(),
            AppError::ClaudeMissing(String::new()).code(),
            AppError::Internal(String::new()).code(),
        ];
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len());
    }
}
