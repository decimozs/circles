//! Helpers behind the composer: file mentions, slash commands and attachments.

pub mod project_skills;

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

/// A file or folder the user can mention with `@`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FileHit {
    /// Relative to the workspace folder.
    pub path: String,
    pub is_dir: bool,
}

/// How many entries are looked at before the search gives up, so a huge tree stays quick.
const WALK_LIMIT: usize = 30_000;

/// Score of `query` against `path`, or `None` when it does not match. Higher is better. The query
/// matches when its letters appear in order, and matches in the file name count for more.
fn score(path: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(-(path.len() as i64));
    }
    let lower = path.to_lowercase();
    let name_start = lower.rfind('/').map_or(0, |i| i + 1);
    let mut total = 0i64;
    let mut at = 0usize;
    let mut previous: Option<usize> = None;
    for q in query.to_lowercase().chars() {
        let found = lower[at..].find(q)? + at;
        total += 1;
        if previous == Some(found.wrapping_sub(1)) {
            total += 3;
        }
        if found >= name_start {
            total += 2;
        }
        if found == 0 || lower.as_bytes()[found - 1] == b'/' {
            total += 2;
        }
        previous = Some(found);
        at = found + q.len_utf8();
    }
    if lower[name_start..].contains(&query.to_lowercase()) {
        total += 20;
    }
    Some(total * 10 - path.len() as i64)
}

/// Files and folders under `root` that match `query`, best first. `.gitignore` is honoured and the
/// `.git` and `.context` folders are skipped.
pub fn search_files(root: &Path, query: &str, limit: usize) -> Vec<FileHit> {
    let mut hits: Vec<(i64, FileHit)> = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| {
            let name = entry.file_name();
            name != ".git" && name != ".context"
        })
        .build();
    for entry in walker.flatten().take(WALK_LIMIT) {
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            continue;
        }
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        if let Some(score) = score(&relative, query.trim()) {
            hits.push((
                score,
                FileHit {
                    path: relative,
                    is_dir,
                },
            ));
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    hits.into_iter().take(limit).map(|(_, hit)| hit).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CommandSource {
    Builtin,
    Project,
    User,
    Skill,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SlashCommand {
    /// Without the leading slash. Folders become `:`, as in `frontend:review`.
    pub name: String,
    pub description: String,
    pub source: CommandSource,
}

const BUILTINS: &[(&str, &str)] = &[
    ("btw", "Ask a side question. The chat is not changed"),
    ("add-dir", "Let the agent work in another folder"),
    ("compact", "Summarise the conversation to free up space"),
];

/// The `description:` of a markdown file's front matter, or its first text line.
fn describe(text: &str) -> String {
    let mut lines = text.lines();
    let mut body_start = text;
    if text.starts_with("---") {
        lines.next();
        let mut consumed = 4;
        for line in lines.by_ref() {
            consumed += line.len() + 1;
            if line.trim() == "---" {
                break;
            }
            if let Some(value) = line.strip_prefix("description:") {
                return value.trim().trim_matches(['"', '\'']).to_string();
            }
        }
        body_start = text.get(consumed.min(text.len())..).unwrap_or("");
    }
    body_start
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(120)
        .collect()
}

fn read_commands(dir: &Path, source: CommandSource, out: &mut Vec<SlashCommand>) {
    let walker = WalkBuilder::new(dir)
        .standard_filters(false)
        .follow_links(true)
        .build();
    for entry in walker.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let Ok(relative) = path.strip_prefix(dir) else {
            continue;
        };
        let name = relative
            .with_extension("")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        let text = fs::read_to_string(path).unwrap_or_default();
        out.push(SlashCommand {
            name,
            description: describe(&text),
            source,
        });
    }
}

fn read_skills(dir: &Path, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let skill = entry.path().join("SKILL.md");
        let Ok(text) = fs::read_to_string(&skill) else {
            continue;
        };
        out.push(SlashCommand {
            name: entry.file_name().to_string_lossy().into_owned(),
            description: describe(&text),
            source: CommandSource::Skill,
        });
    }
}

/// Slash commands for a workspace: the built-ins, then the project's and the user's own commands
/// and skills. A project command hides a user command of the same name.
pub fn slash_commands(workspace: &Path, home: Option<&Path>) -> Vec<SlashCommand> {
    let mut found: Vec<SlashCommand> = BUILTINS
        .iter()
        .map(|(name, description)| SlashCommand {
            name: (*name).into(),
            description: (*description).into(),
            source: CommandSource::Builtin,
        })
        .collect();
    let project = workspace.join(".claude");
    read_commands(
        &project.join("commands"),
        CommandSource::Project,
        &mut found,
    );
    read_skills(&project.join("skills"), &mut found);
    if let Some(home) = home {
        let user = home.join(".claude");
        read_commands(&user.join("commands"), CommandSource::User, &mut found);
        read_skills(&user.join("skills"), &mut found);
    }
    let mut seen = std::collections::HashSet::new();
    found.retain(|c| seen.insert(c.name.clone()));
    found[BUILTINS.len()..].sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Most of one skill that is put in front of a Composer, so a huge file cannot fill its context.
const SKILL_TEXT_LIMIT: usize = 20_000;

/// The skills and commands a Composer's instructions can name: the user's own, not the built-ins.
pub fn user_skills(home: Option<&Path>) -> Vec<SlashCommand> {
    let Some(home) = home else {
        return Vec::new();
    };
    let user = home.join(".claude");
    let mut found = Vec::new();
    read_commands(&user.join("commands"), CommandSource::User, &mut found);
    read_skills(&user.join("skills"), &mut found);
    let mut seen = std::collections::HashSet::new();
    found.retain(|c| seen.insert(c.name.clone()));
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// What a skill (or command) says, without its front matter. Only names made of letters, digits,
/// `-`, `_`, `.` and `:` are looked up, so a name can never point outside the folders.
pub fn skill_text(name: &str, home: &Path) -> Option<String> {
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return None;
    }
    let user = home.join(".claude");
    let text = fs::read_to_string(user.join("skills").join(name).join("SKILL.md"))
        .or_else(|_| {
            fs::read_to_string(
                user.join("commands")
                    .join(format!("{}.md", name.replace(':', "/"))),
            )
        })
        .ok()?;
    let body = match text.strip_prefix("---") {
        Some(rest) => rest.split_once("\n---").map_or(rest, |(_, after)| after),
        None => &text,
    };
    Some(body.trim().chars().take(SKILL_TEXT_LIMIT).collect())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Attachment {
    pub name: String,
    /// Absolute path inside the workspace's `.context/attachments` folder.
    pub path: String,
    pub size: u64,
}

/// Biggest attachment accepted, to keep a stray paste from filling the disk.
pub const ATTACHMENT_LIMIT: usize = 25 * 1024 * 1024;

fn safe_name(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() {
        "attachment".into()
    } else {
        cleaned.chars().take(100).collect()
    }
}

/// Saves an attachment into `<workspace>/.context/attachments`, which is never committed.
pub fn save_attachment(workspace: &Path, name: &str, data_base64: &str) -> AppResult<Attachment> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.trim())
        .map_err(|_| AppError::Invalid("That attachment could not be read.".into()))?;
    if bytes.len() > ATTACHMENT_LIMIT {
        return Err(AppError::Invalid(
            "Attachments can be at most 25 MB.".into(),
        ));
    }
    let dir = workspace.join(".context").join("attachments");
    fs::create_dir_all(&dir)?;
    let name = safe_name(name);
    let unique = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let path: PathBuf = dir.join(format!("{unique}-{name}"));
    fs::write(&path, &bytes)?;
    Ok(Attachment {
        name,
        path: path.to_string_lossy().into_owned(),
        size: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for file in [
            "src/main.rs",
            "src/chat/mod.rs",
            "src/components/button.tsx",
            "README.md",
            "target/debug/junk.rs",
            ".git/config",
            ".context/notes.md",
            ".claude/commands/review.md",
            ".claude/commands/frontend/lint.md",
            ".claude/skills/pdf/SKILL.md",
        ] {
            let path = tmp.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        fs::write(tmp.path().join(".gitignore"), "target\n").unwrap();
        tmp
    }

    fn paths(hits: &[FileHit]) -> Vec<&str> {
        hits.iter().map(|h| h.path.as_str()).collect()
    }

    #[test]
    fn search_matches_letters_in_order_and_prefers_file_names() {
        let tmp = tree();
        let hits = search_files(tmp.path(), "btn", 10);
        assert_eq!(paths(&hits)[0], "src/components/button.tsx");
        let hits = search_files(tmp.path(), "main", 10);
        assert_eq!(paths(&hits)[0], "src/main.rs");
    }

    #[test]
    fn search_skips_ignored_and_internal_folders() {
        let tmp = tree();
        let all = search_files(tmp.path(), "", 100);
        let all = paths(&all);
        assert!(all.contains(&"README.md"));
        assert!(
            all.contains(&".claude/commands/review.md"),
            "dot folders are searched"
        );
        assert!(
            !all.iter().any(|p| p.starts_with("target")),
            "gitignore is honoured"
        );
        assert!(!all.iter().any(|p| p.starts_with(".git/") || *p == ".git"));
        assert!(!all.iter().any(|p| p.starts_with(".context")));
    }

    #[test]
    fn search_reports_folders_and_respects_the_limit() {
        let tmp = tree();
        let hits = search_files(tmp.path(), "chat", 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0],
            FileHit {
                path: "src/chat".into(),
                is_dir: true
            }
        );
        assert!(search_files(tmp.path(), "zzzz", 5).is_empty());
    }

    #[test]
    fn slash_commands_come_from_the_project_and_the_user() {
        let tmp = tree();
        let home = tempfile::tempdir().unwrap();
        let user = home.path().join(".claude/commands");
        fs::create_dir_all(&user).unwrap();
        fs::write(
            user.join("deploy.md"),
            "---\ndescription: Ship it\n---\nbody",
        )
        .unwrap();
        fs::write(user.join("review.md"), "user version").unwrap();

        let commands = slash_commands(tmp.path(), Some(home.path()));
        let names: Vec<_> = commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(&names[..3], ["btw", "add-dir", "compact"]);
        assert!(names.contains(&"frontend:lint"), "folders become colons");
        assert!(names.contains(&"pdf"), "skills are listed");
        let deploy = commands.iter().find(|c| c.name == "deploy").unwrap();
        assert_eq!(deploy.description, "Ship it");
        assert_eq!(deploy.source, CommandSource::User);
        let review = commands
            .iter()
            .filter(|c| c.name == "review")
            .collect::<Vec<_>>();
        assert_eq!(review.len(), 1, "the project command hides the user one");
        assert_eq!(review[0].source, CommandSource::Project);
    }

    #[test]
    fn descriptions_fall_back_to_the_first_line() {
        assert_eq!(describe("# Review the diff\nmore"), "Review the diff");
        assert_eq!(
            describe("---\nname: x\n---\n\nDo the thing\nmore"),
            "Do the thing"
        );
        assert_eq!(describe(""), "");
    }

    #[test]
    fn attachments_are_saved_under_the_context_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let saved = save_attachment(tmp.path(), "../../shot one.png", "aGVsbG8=").unwrap();
        assert_eq!(saved.name, "shot one.png");
        assert_eq!(saved.size, 5);
        let path = PathBuf::from(&saved.path);
        assert!(path.starts_with(tmp.path().join(".context/attachments")));
        assert_eq!(fs::read(path).unwrap(), b"hello");
        let again = save_attachment(tmp.path(), "shot one.png", "aGVsbG8=").unwrap();
        assert_ne!(again.path, saved.path, "the same name does not overwrite");
    }

    #[test]
    fn bad_attachments_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(save_attachment(tmp.path(), "a.txt", "***").is_err());
        assert_eq!(safe_name("/etc/passwd"), "passwd");
        assert_eq!(safe_name("..hidden"), "hidden");
        assert_eq!(safe_name(""), "attachment");
        assert_eq!(safe_name("a/b\\c?.txt"), "b_c_.txt");
    }
}
