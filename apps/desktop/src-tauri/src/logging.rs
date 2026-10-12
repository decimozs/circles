//! File logging with secret redaction.
//!
//! Logs go to a daily rolling file under the app data dir. Environment values are never logged
//! on purpose, and as a second line of defence every line is scrubbed of known secret values.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, RwLock};

use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::error::{AppError, AppResult};

const MAX_LOG_FILES: usize = 14;
const REDACTED: &str = "[redacted]";

#[derive(Clone, Default)]
pub struct Redactor {
    values: Arc<RwLock<Vec<String>>>,
}

impl Redactor {
    /// Replaces the set of values to scrub. Longest first so a secret that contains another
    /// one is removed whole.
    pub fn set_values(&self, mut values: Vec<String>) {
        values.retain(|v| !v.is_empty());
        values.sort_by_key(|v| std::cmp::Reverse(v.len()));
        values.dedup();
        if let Ok(mut guard) = self.values.write() {
            *guard = values;
        }
    }

    pub fn redact(&self, text: &str) -> String {
        let Ok(values) = self.values.read() else {
            return text.to_string();
        };
        let mut out = text.to_string();
        for value in values.iter() {
            if out.contains(value.as_str()) {
                out = out.replace(value.as_str(), REDACTED);
            }
        }
        out
    }
}

pub struct RedactingWriter<W: Write> {
    inner: W,
    redactor: Redactor,
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let scrubbed = self.redactor.redact(&String::from_utf8_lossy(buf));
        self.inner.write_all(scrubbed.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[derive(Clone)]
struct RedactingMakeWriter {
    inner: NonBlocking,
    redactor: Redactor,
}

impl<'a> MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingWriter<NonBlocking>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.inner.clone(),
            redactor: self.redactor.clone(),
        }
    }
}

/// How much to log, from the user's settings file. Read before anything else starts, so a problem
/// with the file never stops the app: it means the usual level.
pub fn level_from_settings(file: &Path) -> String {
    const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| {
            table
                .get("advanced")?
                .get("log_level")?
                .as_str()
                .map(str::to_string)
        })
        .filter(|level| LEVELS.contains(&level.as_str()))
        .unwrap_or_else(|| "info".to_string())
}

/// Starts logging. Keep the returned guard alive for the life of the app, or buffered lines
/// are lost on exit.
pub fn init(logs_dir: &Path, redactor: Redactor, level: &str) -> AppResult<WorkerGuard> {
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("circles")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(logs_dir)
        .map_err(|e| AppError::Internal(format!("cannot open the log file: {e}")))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(RedactingMakeWriter {
            inner: writer,
            redactor,
        });
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(io::stderr);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .map_err(|e| AppError::Internal(format!("logging already initialised: {e}")))?;
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_known_values() {
        let r = Redactor::default();
        r.set_values(vec!["ghp_abcdef123456".into(), "".into()]);
        assert_eq!(
            r.redact("token=ghp_abcdef123456 and again ghp_abcdef123456"),
            "token=[redacted] and again [redacted]"
        );
        assert_eq!(r.redact("nothing to hide"), "nothing to hide");
    }

    #[test]
    fn longest_secret_is_removed_whole() {
        let r = Redactor::default();
        r.set_values(vec!["secret1234".into(), "secret1234-extended".into()]);
        assert_eq!(r.redact("v=secret1234-extended"), "v=[redacted]");
    }

    #[test]
    fn writer_redacts_before_writing() {
        let r = Redactor::default();
        r.set_values(vec!["topsecretvalue".into()]);
        let mut sink = Vec::new();
        {
            let mut w = RedactingWriter {
                inner: &mut sink,
                redactor: r,
            };
            w.write_all(b"login topsecretvalue ok\n").unwrap();
        }
        assert_eq!(String::from_utf8(sink).unwrap(), "login [redacted] ok\n");
    }

    #[test]
    fn the_log_level_comes_from_the_settings_and_falls_back_to_info() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.toml");
        assert_eq!(level_from_settings(&file), "info", "no file");
        std::fs::write(&file, "[advanced]\nlog_level = \"debug\"\n").unwrap();
        assert_eq!(level_from_settings(&file), "debug");
        std::fs::write(&file, "[advanced]\nlog_level = \"loud\"\n").unwrap();
        assert_eq!(level_from_settings(&file), "info", "not a level");
        std::fs::write(&file, "this is = not [valid").unwrap();
        assert_eq!(
            level_from_settings(&file),
            "info",
            "a broken file never stops the app"
        );
        std::fs::write(&file, "[general]\nsound = true\n").unwrap();
        assert_eq!(level_from_settings(&file), "info", "not set");
    }
}
