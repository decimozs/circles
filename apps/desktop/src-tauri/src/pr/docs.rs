//! What a repository writes down about how it wants work done: its pull request template and the
//! files that name its conventions. They go into the prompts that draft a pull request or group
//! commits, so the result follows the repository without anyone copying its rules into settings.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The most of a pull request template that is read.
const TEMPLATE_LIMIT: usize = 6_000;
/// The most of one convention file that is read, and of all of them together.
const FILE_LIMIT: usize = 4_000;
const TOTAL_LIMIT: usize = 10_000;

/// Where a pull request template may be, in the order tried.
const TEMPLATE_PLACES: &[(&str, &str)] = &[
    (".github", "pull_request_template.md"),
    ("", "pull_request_template.md"),
    ("docs", "pull_request_template.md"),
];
/// A folder of several templates. One named `default`, or the first by name, is used.
const TEMPLATE_DIR: (&str, &str) = (".github", "PULL_REQUEST_TEMPLATE");

/// The files that name a repository's conventions, in the order they are shown.
const CONVENTION_FILES: &[(&str, &str)] = &[
    ("", "AGENTS.md"),
    ("", "CLAUDE.md"),
    ("", "CONTRIBUTING.md"),
    (".github", "CONTRIBUTING.md"),
    ("docs", "CONTRIBUTING.md"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoDocs {
    /// The pull request template, as the repository wrote it.
    pub pr_template: Option<String>,
    /// The convention files, each under a line that names it.
    pub conventions: Option<String>,
}

impl RepoDocs {
    pub fn is_empty(&self) -> bool {
        self.pr_template.is_none() && self.conventions.is_none()
    }
}

/// The entry of `dir` whose name is `name` in any letter case.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn read_capped(path: &Path, limit: usize) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push_str("\n(cut here because it is long)");
    }
    Some(out)
}

fn pr_template(root: &Path) -> Option<String> {
    for (folder, name) in TEMPLATE_PLACES {
        let dir = if folder.is_empty() {
            root.to_path_buf()
        } else {
            match find(root, folder) {
                Some(d) if d.is_dir() => d,
                _ => continue,
            }
        };
        if let Some(text) = find(&dir, name).and_then(|p| read_capped(&p, TEMPLATE_LIMIT)) {
            return Some(text);
        }
    }
    let (folder, name) = TEMPLATE_DIR;
    let dir = find(&find(root, folder)?, name)?;
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
        .collect();
    files.sort();
    let chosen = files
        .iter()
        .find(|p| {
            p.file_stem()
                .is_some_and(|s| s.eq_ignore_ascii_case("default"))
        })
        .or(files.first())?;
    read_capped(chosen, TEMPLATE_LIMIT)
}

fn conventions(root: &Path) -> Option<String> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = String::new();
    for (folder, name) in CONVENTION_FILES {
        let dir = if folder.is_empty() {
            root.to_path_buf()
        } else {
            match find(root, folder) {
                Some(d) if d.is_dir() => d,
                _ => continue,
            }
        };
        let Some(path) = find(&dir, name) else {
            continue;
        };
        // AGENTS.md is often a link to CLAUDE.md: the same text is not shown twice.
        let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.contains(&real) {
            continue;
        }
        seen.push(real);
        let room = TOTAL_LIMIT.saturating_sub(out.chars().count());
        if room < 200 {
            break;
        }
        if let Some(text) = read_capped(&path, FILE_LIMIT.min(room)) {
            let label = if folder.is_empty() {
                (*name).to_string()
            } else {
                format!("{folder}/{name}")
            };
            out.push_str(&format!("From {label}:\n{text}\n\n"));
        }
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// The pull request template and convention files of the repository at `root`.
pub fn read(root: &Path) -> RepoDocs {
    RepoDocs {
        pr_template: pr_template(root),
        conventions: conventions(root),
    }
}

/// The part of a prompt that tells the agent what the repository wrote down. `with_template` is
/// for pull requests, where the template shapes the description.
pub fn section(docs: &RepoDocs, with_template: bool) -> String {
    let mut out = String::new();
    if with_template {
        if let Some(template) = &docs.pr_template {
            out.push_str(&format!(
                "\nThe repository has a pull request template. Write the description by filling \
                 it in: keep its headings and their order, answer each part from the changes, \
                 and remove its placeholder comments and parts that do not apply.\n\
                 ```markdown\n{template}\n```\n"
            ));
        }
    }
    if let Some(text) = &docs.conventions {
        out.push_str(&format!(
            "\nThe repository documents its conventions in the files below. Follow what they say \
             about {}, and ignore the rest.\n\n{text}\n",
            if with_template {
                "pull request titles and descriptions"
            } else {
                "commit messages"
            }
        ));
    }
    out
}

/// A repository's conventions as Claude proposes to keep them in the settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Learned {
    /// How branches are named, such as `{type}/{ticket}-{slug}`.
    pub branch_pattern: Option<String>,
    /// How to write the title and description of a pull request.
    pub pr: Option<String>,
    /// How commit messages should look.
    pub commits: Option<String>,
}

/// The block an agent answers the learning prompt with.
pub const LEARN_FENCE: &str = "circles-conventions";

/// What the repository shows of its habits, for the model to read.
pub struct Evidence<'a> {
    pub docs: &'a RepoDocs,
    /// Subjects of recent commits.
    pub commits: &'a [String],
    /// Names of recent branches.
    pub branches: &'a [String],
}

impl Evidence<'_> {
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty() && self.commits.is_empty() && self.branches.is_empty()
    }
}

pub fn learn_prompt(evidence: &Evidence<'_>) -> String {
    let mut body = String::from(
        "Read what this repository shows of how its work is done, and write down its conventions \
         for a tool that drafts pull requests and commit messages and names branches.\n",
    );
    if let Some(template) = &evidence.docs.pr_template {
        body.push_str(&format!(
            "\nPull request template:\n```markdown\n{template}\n```\n"
        ));
    }
    if let Some(text) = &evidence.docs.conventions {
        body.push_str(&format!("\nConvention files:\n{text}\n"));
    }
    if !evidence.commits.is_empty() {
        body.push_str("\nRecent commit subjects:\n");
        for s in evidence.commits {
            body.push_str(&format!("- {s}\n"));
        }
    }
    if !evidence.branches.is_empty() {
        body.push_str("\nRecent branch names:\n");
        for b in evidence.branches {
            body.push_str(&format!("- {b}\n"));
        }
    }
    body.push_str(&format!(
        "\nReply with one fenced code block labelled {LEARN_FENCE} that holds a JSON object with \
         three fields, each a string or null when the repository shows nothing about it:\n\
         - \"branch_pattern\": how branches are named, written with {{type}} (feat, fix, chore), \
         {{slug}} (lowercase words joined by dashes) and {{ticket}} (an issue key such as \
         PROJ-123), for example \"{{type}}/{{ticket}}-{{slug}}\". Only when the branches or the \
         files show a clear rule.\n\
         - \"pr\": short instructions for writing the title and description of a pull request: \
         the title format and any required sections, in a few lines.\n\
         - \"commits\": short instructions for commit messages: the subject format, mood, scope \
         and body rules, in a few lines.\n\
         Say only what the evidence supports. Do not invent a rule. Do not use any tool."
    ));
    body
}

/// The proposal in the last block labelled with `LEARN_FENCE`. A branch pattern with no
/// placeholder is not a pattern, so it is dropped, and empty texts become `None`.
pub fn parse_learned(text: &str) -> Option<Learned> {
    let mut found = None;
    for part in text.split(&format!("```{LEARN_FENCE}")).skip(1) {
        let Some(body) = part.split("```").next() else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
            continue;
        };
        let field = |name: &str| {
            value
                .get(name)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        found = Some(Learned {
            branch_pattern: field("branch_pattern").filter(|p| {
                ["{type}", "{slug}", "{ticket}"]
                    .iter()
                    .any(|placeholder| p.contains(placeholder))
            }),
            pr: field("pr"),
            commits: field("commits"),
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }

    #[test]
    fn a_pull_request_template_is_found_in_any_letter_case() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".github/PULL_REQUEST_TEMPLATE.md",
            "## Summary\n",
        );
        assert_eq!(read(dir.path()).pr_template.as_deref(), Some("## Summary"));
    }

    #[test]
    fn a_folder_of_templates_gives_default_or_the_first() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".github/PULL_REQUEST_TEMPLATE/bug.md", "bug");
        write(
            dir.path(),
            ".github/PULL_REQUEST_TEMPLATE/feature.md",
            "feature",
        );
        assert_eq!(read(dir.path()).pr_template.as_deref(), Some("bug"));
        write(
            dir.path(),
            ".github/PULL_REQUEST_TEMPLATE/default.md",
            "default",
        );
        assert_eq!(read(dir.path()).pr_template.as_deref(), Some("default"));
    }

    #[test]
    fn a_repository_with_nothing_has_no_docs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path()).is_empty());
        write(dir.path(), "README.md", "hello");
        write(dir.path(), "CLAUDE.md", "   \n");
        assert!(read(dir.path()).is_empty());
    }

    #[test]
    fn convention_files_are_labelled_and_a_link_to_the_same_file_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "CLAUDE.md", "Branches are feat/KEY-1-slug.");
        write(
            dir.path(),
            "CONTRIBUTING.md",
            "Titles start with the ticket.",
        );
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("CLAUDE.md"), dir.path().join("AGENTS.md"))
            .unwrap();
        let text = read(dir.path()).conventions.unwrap();
        // AGENTS.md comes first, so the shared text is labelled with it and CLAUDE.md is skipped.
        assert!(text.contains("From AGENTS.md:\nBranches are feat/KEY-1-slug."));
        assert!(text.contains("From CONTRIBUTING.md:"));
        assert_eq!(text.matches("Branches are").count(), 1, "{text}");
        assert!(!text.contains("From CLAUDE.md"), "{text}");
    }

    #[test]
    fn long_files_are_cut_and_the_total_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "CLAUDE.md", &"a".repeat(FILE_LIMIT * 3));
        write(dir.path(), "CONTRIBUTING.md", &"b".repeat(FILE_LIMIT * 3));
        let text = read(dir.path()).conventions.unwrap();
        assert!(text.contains("cut here because it is long"));
        assert!(text.chars().count() < TOTAL_LIMIT + 400, "{}", text.len());
    }

    #[test]
    fn the_prompt_section_holds_the_template_for_a_pull_request_only() {
        let docs = RepoDocs {
            pr_template: Some("## Summary".into()),
            conventions: Some("From CLAUDE.md:\nrules".into()),
        };
        let pr = section(&docs, true);
        assert!(pr.contains("## Summary") && pr.contains("rules"));
        assert!(pr.contains("pull request titles and descriptions"));
        let commit = section(&docs, false);
        assert!(!commit.contains("## Summary") && commit.contains("commit messages"));
        assert_eq!(section(&RepoDocs::default(), true), "");
    }

    #[test]
    fn what_was_learned_is_read_from_the_last_block_and_cleaned() {
        let text = "Here.\n```circles-conventions\n{\"branch_pattern\":\"x\"}\n```\n```circles-conventions\n{\"branch_pattern\":\" {type}/{ticket}-{slug} \",\"pr\":\"Start with the ticket.\",\"commits\":\"  \"}\n```";
        assert_eq!(
            parse_learned(text),
            Some(Learned {
                branch_pattern: Some("{type}/{ticket}-{slug}".into()),
                pr: Some("Start with the ticket.".into()),
                commits: None,
            })
        );
        // A pattern with no placeholder is not a pattern.
        let plain = "```circles-conventions\n{\"branch_pattern\":\"main\"}\n```";
        assert_eq!(parse_learned(plain).unwrap().branch_pattern, None);
        assert_eq!(parse_learned("no block"), None);
    }

    #[test]
    fn the_learning_prompt_lists_only_what_there_is() {
        let docs = RepoDocs {
            pr_template: None,
            conventions: Some("From CLAUDE.md:\nrules".into()),
        };
        let evidence = Evidence {
            docs: &docs,
            commits: &["feat(ui): add login".to_string()],
            branches: &[],
        };
        let prompt = learn_prompt(&evidence);
        assert!(prompt.contains("rules") && prompt.contains("feat(ui): add login"));
        assert!(!prompt.contains("Pull request template") && !prompt.contains("Recent branch"));
        assert!(prompt.contains("circles-conventions"));
    }
}
