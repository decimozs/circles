//! The Tauri side of terminals: a channel that carries output to the webview, and the event that
//! tells the UI a process started or ended.

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};

use super::{TerminalChange, TerminalNotifier};
use crate::pty::{ExitInfo, PtySink};

pub const CHANGED_EVENT: &str = "terminal:changed";

/// Sends terminal output over a Tauri channel. Output is raw bytes, which the webview receives
/// as an `ArrayBuffer`. Control messages are JSON: `{"type":"exit",...}` and `{"type":"reset"}`.
pub struct ChannelSink(pub Channel<InvokeResponseBody>);

impl PtySink for ChannelSink {
    fn data(&self, bytes: Vec<u8>) {
        let _ = self.0.send(InvokeResponseBody::Raw(bytes));
    }

    fn exit(&self, info: &ExitInfo) {
        let message = serde_json::json!({
            "type": "exit",
            "code": info.code,
            "signaled": info.signaled,
        });
        let _ = self.0.send(InvokeResponseBody::Json(message.to_string()));
    }

    fn reset(&self) {
        let _ = self
            .0
            .send(InvokeResponseBody::Json(r#"{"type":"reset"}"#.to_string()));
    }
}

pub struct TauriNotifier(pub AppHandle);

impl TerminalNotifier for TauriNotifier {
    fn changed(&self, change: &TerminalChange) {
        if let Err(e) = self.0.emit(CHANGED_EVENT, change) {
            tracing::warn!(error = %e, "cannot emit {CHANGED_EVENT}");
        }
    }
}
