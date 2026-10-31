//! Reading Claude Code's own transcripts for token counts.
//!
//! Claude Code writes every conversation to `<config dir>/projects/<project>/<session>.jsonl`.
//! Each assistant line has the model, a timestamp and a `usage` object. The format is not a
//! contract, so a line that does not look as expected is counted and skipped, never fatal. Only
//! numbers, ids and the model are kept: the text of a conversation is not stored.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde_json::Value;

/// The usage of one answer from the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// The API message id. One message can be written to a transcript several times while it
    /// streams, and the last copy is the complete one.
    pub id: String,
    pub model: String,
    /// Unix seconds.
    pub at: i64,
    /// `YYYY-MM-DD`, in UTC.
    pub day: String,
    pub session: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

/// Records read from one transcript, and how many lines were skipped.
#[derive(Debug, Default, Clone)]
pub struct Scanned {
    pub records: Vec<Record>,
    pub skipped: u32,
}

/// The folders that hold Claude Code's data: `CLAUDE_CONFIG_DIR` when it is set, and every
/// `~/.claude*` folder in the home folder that has a `projects` folder inside.
pub fn config_dirs(home: Option<&Path>, configured: Option<&str>) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut add = |dir: PathBuf| {
        if dir.join("projects").is_dir() && !found.contains(&dir) {
            found.push(dir);
        }
    };
    if let Some(dir) = configured.filter(|d| !d.is_empty()) {
        add(PathBuf::from(dir));
    }
    if let Some(home) = home {
        let mut names: Vec<PathBuf> = std::fs::read_dir(home)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".claude"))
            .map(|e| e.path())
            .collect();
        names.sort();
        for dir in names {
            add(dir);
        }
    }
    found
}

/// Every `.jsonl` file under `<dir>/projects`, subagent transcripts included.
pub fn transcripts(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.join("projects")];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `file_type` does not follow links, so a link cannot send the walk in circles.
            match entry.file_type() {
                Ok(t) if t.is_dir() => pending.push(path),
                Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "jsonl") => {
                    files.push(path)
                }
                _ => {}
            }
        }
    }
    files.sort();
    files
}

/// One line of a transcript. `None` when it is not an assistant answer with usage; `Err` when it
/// looks like one but cannot be understood.
pub fn parse_line(line: &str) -> Result<Option<Record>, ()> {
    // Most lines are user messages and tool output. Skip them without parsing.
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(line).map_err(|_| ())?;
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return Ok(None);
    }
    let message = value.get("message").ok_or(())?;
    let usage = message.get("usage").ok_or(())?;
    let id = message.get("id").and_then(Value::as_str).ok_or(())?;
    let model = message.get("model").and_then(Value::as_str).ok_or(())?;
    if model.starts_with('<') {
        // `<synthetic>`: Claude Code's own notices, with no model call behind them.
        return Ok(None);
    }
    let stamp = value.get("timestamp").and_then(Value::as_str).ok_or(())?;
    let at = parse_timestamp(stamp).ok_or(())?;
    let count = |v: &Value, key: &str| v.get(key).and_then(Value::as_u64).unwrap_or(0);
    let written = count(usage, "cache_creation_input_tokens");
    let (w5m, w1h) = match usage.get("cache_creation") {
        Some(split) => {
            let (a, b) = (
                count(split, "ephemeral_5m_input_tokens"),
                count(split, "ephemeral_1h_input_tokens"),
            );
            // An older transcript has the total but no split: treat the rest as 5 minute writes.
            (a + written.saturating_sub(a + b), b)
        }
        None => (written, 0),
    };
    Ok(Some(Record {
        id: id.to_string(),
        model: model.to_string(),
        at,
        day: stamp.get(..10).unwrap_or_default().to_string(),
        session: value
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        input: count(usage, "input_tokens"),
        output: count(usage, "output_tokens"),
        cache_read: count(usage, "cache_read_input_tokens"),
        cache_write_5m: w5m,
        cache_write_1h: w1h,
    }))
}

/// `2026-10-08T00:56:58.617Z` as unix seconds.
pub fn parse_timestamp(text: &str) -> Option<i64> {
    let (date, time) = text.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let time = time.trim_end_matches('Z');
    let time = time.split(['.', '+']).next()?;
    let mut t = time.split(':');
    let (h, min, s): (i64, i64, i64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3_600 + min * 60 + s)
}

fn read_file(path: &Path) -> Scanned {
    let mut out = Scanned::default();
    let Ok(file) = File::open(path) else {
        return out;
    };
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            out.skipped += 1;
            continue;
        };
        match parse_line(&line) {
            Ok(Some(record)) => out.records.push(record),
            Ok(None) => {}
            Err(()) => out.skipped += 1,
        }
    }
    out
}

type Cache = HashMap<PathBuf, (u64, SystemTime, Scanned)>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// All records under the given folders. A transcript that has not changed since the last call is
/// not read again, so opening the dialog twice costs one read of the files that grew.
pub fn scan(dirs: &[PathBuf]) -> Scanned {
    let mut all: HashMap<String, Record> = HashMap::new();
    let mut skipped = 0;
    let mut seen: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        for path in transcripts(dir) {
            let (len, modified) = std::fs::metadata(&path)
                .map(|m| (m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
                .unwrap_or((0, SystemTime::UNIX_EPOCH));
            let cached = cache()
                .lock()
                .ok()
                .and_then(|c| c.get(&path).cloned())
                .filter(|(l, m, _)| *l == len && *m == modified)
                .map(|(_, _, scanned)| scanned);
            let scanned = match cached {
                Some(scanned) => scanned,
                None => {
                    let scanned = read_file(&path);
                    if let Ok(mut c) = cache().lock() {
                        c.insert(path.clone(), (len, modified, scanned.clone()));
                    }
                    scanned
                }
            };
            skipped += scanned.skipped;
            for record in scanned.records {
                // The same message can be in more than one transcript when a conversation is
                // resumed or branched. It is one answer, so it is counted once.
                all.insert(record.id.clone(), record);
            }
            seen.push(path);
        }
    }
    if let Ok(mut c) = cache().lock() {
        c.retain(|path, _| seen.contains(path));
    }
    Scanned {
        records: all.into_values().collect(),
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(id: &str, stamp: &str, extra: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{stamp}","sessionId":"s1","message":{{"id":"{id}","model":"claude-sonnet-5-5","usage":{{"input_tokens":2,"output_tokens":107,"cache_read_input_tokens":23193,"cache_creation_input_tokens":24632{extra}}}}}}}"#
        )
    }

    #[test]
    fn a_timestamp_becomes_unix_seconds() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_timestamp("2026-10-08T00:56:58.617Z"),
            Some(1_791_421_018)
        );
        assert_eq!(parse_timestamp("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(parse_timestamp("nonsense"), None);
        assert_eq!(parse_timestamp("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn an_answer_gives_its_counts_and_the_split_of_cache_writes() {
        let line = assistant(
            "m1",
            "2026-10-08T00:56:58.617Z",
            r#","cache_creation":{"ephemeral_1h_input_tokens":24632,"ephemeral_5m_input_tokens":0}"#,
        );
        let r = parse_line(&line).unwrap().unwrap();
        assert_eq!(r.model, "claude-sonnet-5-5");
        assert_eq!(r.day, "2026-10-08");
        assert_eq!(r.session, "s1");
        assert_eq!(
            (
                r.input,
                r.output,
                r.cache_read,
                r.cache_write_5m,
                r.cache_write_1h
            ),
            (2, 107, 23193, 0, 24632)
        );
    }

    #[test]
    fn cache_writes_without_a_split_count_as_five_minute_writes() {
        let r = parse_line(&assistant("m1", "2026-10-08T00:00:00Z", ""))
            .unwrap()
            .unwrap();
        assert_eq!((r.cache_write_5m, r.cache_write_1h), (24632, 0));
    }

    #[test]
    fn lines_that_are_not_answers_are_not_counted_and_broken_ones_are_reported() {
        assert_eq!(
            parse_line(r#"{"type":"user","message":{"content":"hi"}}"#),
            Ok(None)
        );
        let synthetic =
            assistant("m1", "2026-10-08T00:00:00Z", "").replace("claude-sonnet-5-5", "<synthetic>");
        assert_eq!(parse_line(&synthetic), Ok(None));
        assert_eq!(parse_line(r#"{"type":"assistant","usage": nope"#), Err(()));
        assert_eq!(
            parse_line(r#"{"type":"assistant","message":{"usage":{}}}"#),
            Err(())
        );
    }

    #[test]
    fn a_message_written_several_times_is_counted_once_with_its_last_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cfg");
        let project = dir.join("projects").join("p");
        std::fs::create_dir_all(&project).unwrap();
        let early = assistant("m1", "2026-10-08T00:00:00Z", "")
            .replace("\"output_tokens\":107", "\"output_tokens\":5");
        let late = assistant("m1", "2026-10-08T00:00:01Z", "");
        let other = assistant("m2", "2026-10-09T00:00:00Z", "");
        std::fs::write(
            project.join("a.jsonl"),
            format!("{early}\n{{\"type\":\"user\"}}\n{late}\n{other}\nnot json but has \"usage\" and \"assistant\"\n"),
        )
        .unwrap();
        // A resumed conversation repeats an earlier message in another file.
        std::fs::write(project.join("b.jsonl"), format!("{late}\n")).unwrap();

        let scanned = scan(&[dir]);
        assert_eq!(scanned.skipped, 1);
        let mut records = scanned.records;
        records.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].output, 107);
    }

    #[test]
    fn the_config_folders_are_the_set_one_and_every_claude_folder_with_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        for name in [".claude", ".claude-work", ".claude-empty"] {
            std::fs::create_dir_all(home.join(name)).unwrap();
        }
        std::fs::create_dir_all(home.join(".claude/projects")).unwrap();
        std::fs::create_dir_all(home.join(".claude-work/projects")).unwrap();
        std::fs::write(home.join(".claude.json"), "{}").unwrap();
        let dirs = config_dirs(
            Some(home),
            Some(home.join(".claude-work").to_str().unwrap()),
        );
        assert_eq!(dirs, vec![home.join(".claude-work"), home.join(".claude")]);
        assert!(config_dirs(None, None).is_empty());
    }
}
