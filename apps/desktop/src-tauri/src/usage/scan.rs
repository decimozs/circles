//! Reading Claude Code's own transcripts for token counts.
//!
//! Claude Code writes every conversation to `<config dir>/projects/<project>/<session>.jsonl`.
//! Each assistant line has the model, a timestamp and a `usage` object. The format is not a
//! contract, so a line that does not look as expected is counted and skipped, never fatal. Only
//! numbers, ids and the model are kept: the text of a conversation is not stored.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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

/// What was read from a transcript from some point on.
#[derive(Debug, Default, Clone)]
pub struct Read {
    pub records: Vec<Record>,
    pub skipped: u32,
    /// Where to start next time: the end of the last complete line. A line still being written has
    /// no end yet, so it is left for the next read.
    pub offset: u64,
}

/// Reads the complete lines of `path` from byte `from`.
pub fn read_from(path: &Path, from: u64) -> std::io::Result<Read> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(from))?;
    let mut reader = BufReader::new(file);
    let mut out = Read {
        offset: from,
        ..Read::default()
    };
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 || line.last() != Some(&b'\n') {
            break;
        }
        out.offset += n as u64;
        match parse_line(&String::from_utf8_lossy(&line)) {
            Ok(Some(record)) => out.records.push(record),
            Ok(None) => {}
            Err(()) => out.skipped += 1,
        }
    }
    Ok(out)
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
    fn only_complete_lines_are_read_and_reading_goes_on_from_where_it_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.jsonl");
        let first = assistant("m1", "2026-10-08T00:00:00Z", "");
        let second = assistant("m2", "2026-10-08T00:00:01Z", "");
        // The second line is still being written: it has no end yet.
        std::fs::write(&path, format!("{first}\n{{\"type\":\"user\"}}\n{second}")).unwrap();
        let one = read_from(&path, 0).unwrap();
        assert_eq!(one.records.len(), 1);
        assert_eq!(one.records[0].id, "m1");
        assert_eq!(
            one.offset as usize,
            first.len() + 1 + "{\"type\":\"user\"}".len() + 1
        );

        // It is finished and a broken line follows. Reading from the offset gives only the new.
        std::fs::write(
            &path,
            format!("{first}\n{{\"type\":\"user\"}}\n{second}\nnot json but has \"usage\" and \"assistant\"\n"),
        )
        .unwrap();
        let two = read_from(&path, one.offset).unwrap();
        assert_eq!((two.records.len(), two.skipped), (1, 1));
        assert_eq!(two.records[0].id, "m2");
        assert_eq!(two.offset, std::fs::metadata(&path).unwrap().len());
        // Nothing more to read.
        let none = read_from(&path, two.offset).unwrap();
        assert!(none.records.is_empty() && none.skipped == 0);
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
