//! IPC conventions used by every command.
//!
//! - Request and response: `#[tauri::command]` returning `IpcResult<T>`. Errors serialise to
//!   `{ code, message, details }` (see `error::ErrorPayload`).
//! - High-volume streams (PTY bytes, agent events): take a `tauri::ipc::Channel<T>` argument and
//!   send batched messages through it. Do not use `emit` for streams.
//! - Low-volume notifications (state changes): `app.emit("<domain>:<event>", payload)`.
//! - Every type crossing the boundary derives `ts_rs::TS` with `#[ts(export)]`, and the frontend
//!   imports the generated file from `packages/bindings/src`.

use crate::error::AppError;

pub type IpcResult<T> = Result<T, AppError>;

/// Current time as unix milliseconds (UTC). All stored timestamps use this.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// New sortable id (ULID).
pub fn new_id() -> String {
    ulid::Ulid::generate().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_26_chars() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.len(), 26);
        assert_ne!(a, b);
    }

    #[test]
    fn ids_sort_by_creation_time() {
        let first = new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = new_id();
        assert!(first < second);
    }

    #[test]
    fn now_ms_is_recent() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}
