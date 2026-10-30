//! The messages that start an agent on a task, with the repository's own instructions added.

use super::docs::{section, RepoDocs};
use crate::github::pr::{Check, ReviewThread};
use crate::settings::Instructions;

/// Logs longer than this are cut from the front, because the failure is at the end.
pub const LOG_LIMIT: usize = 6000;

/// The instruction text for a task: the general one, then the one for the task.
fn with_instructions(body: String, general: Option<&str>, specific: Option<&str>) -> String {
    let mut out = body;
    for text in [general, specific].into_iter().flatten() {
        let text = text.trim();
        if !text.is_empty() {
            out.push_str("\n\nRepository instructions:\n");
            out.push_str(text);
        }
    }
    out
}

pub fn fix_checks(
    pr_number: u32,
    failing: &[(Check, Option<String>)],
    instructions: &Instructions,
) -> String {
    let mut body =
        format!(
        "{} on pull request #{pr_number}. Find the cause of each, fix it, run the relevant tests \
         locally, and commit the fix.\n",
        if failing.len() == 1 { "A check is failing" } else { "Some checks are failing" }
    );
    for (check, log) in failing {
        body.push_str(&format!("\n## {}", check.name));
        if !check.workflow.is_empty() {
            body.push_str(&format!(" ({})", check.workflow));
        }
        if !check.link.is_empty() {
            body.push_str(&format!("\n{}", check.link));
        }
        match log {
            Some(log) if !log.trim().is_empty() => {
                body.push_str(&format!("\n\n```\n{}\n```\n", log.trim_end()));
            }
            _ => body.push_str("\n\nNo log was available. Run it locally to see why it fails.\n"),
        }
    }
    with_instructions(
        body,
        instructions.general.as_deref(),
        instructions.fix_errors.as_deref(),
    )
}

pub fn respond_to_review(
    pr_number: u32,
    threads: &[ReviewThread],
    instructions: &Instructions,
) -> String {
    let mut body = format!(
        "Reviewers left comments on pull request #{pr_number} that are not resolved yet. Address \
         each one: make the change, or explain in a reply here why not. Commit your changes.\n"
    );
    for thread in threads.iter().filter(|t| !t.resolved) {
        let place = match thread.line {
            Some(line) => format!("{}:{line}", thread.path),
            None => thread.path.clone(),
        };
        body.push_str(&format!("\n## {place}"));
        if thread.outdated {
            body.push_str(" (the code has changed since)");
        }
        for comment in &thread.comments {
            body.push_str(&format!("\n{}: {}", comment.author, comment.body.trim()));
        }
        body.push('\n');
    }
    with_instructions(
        body,
        instructions.general.as_deref(),
        instructions.review.as_deref(),
    )
}

pub fn resolve_conflicts(base: &str, instructions: &Instructions) -> String {
    let body = format!(
        "This branch conflicts with {base}. Bring in the latest {base}, resolve every conflict so \
         the behaviour of both sides is kept, make sure the project still builds and its tests \
         pass, and commit the result."
    );
    with_instructions(
        body,
        instructions.general.as_deref(),
        instructions.conflicts.as_deref(),
    )
}

/// The block an agent answers a pull request draft with.
pub const PR_FENCE: &str = "circles-pr";

/// How much of the diff is put in the prompt. The agent is given the changes so it needs no tools,
/// which would each ask for approval.
pub const DIFF_LIMIT: usize = 20_000;

pub fn draft_pr(
    base: &str,
    commits: &[String],
    changes: &str,
    instructions: &Instructions,
    docs: &RepoDocs,
) -> String {
    let mut body = format!(
        "Write the title and description for a pull request of this branch into {base}. The \
         commits and the changes are below, so you do not need to run any tool. Say what changed \
         and why.\n"
    );
    if !commits.is_empty() {
        body.push_str("\nCommits:\n");
        for subject in commits {
            body.push_str(&format!("- {subject}\n"));
        }
    }
    if !changes.trim().is_empty() {
        let (shown, cut) = match changes.char_indices().nth(DIFF_LIMIT) {
            Some((end, _)) => (&changes[..end], true),
            None => (changes, false),
        };
        body.push_str(&format!("\nChanges:\n```diff\n{}\n```\n", shown.trim_end()));
        if cut {
            body.push_str("(The diff was cut here because it is long.)\n");
        }
    }
    body.push_str(&section(docs, true));
    body.push_str(&format!(
        "\nDo not use any tool, do not change any files and do not create the pull request. Do not add a line that credits Claude or Claude Code. Reply with one fenced code \
         block labelled {PR_FENCE} that holds a JSON object with \"title\" and \"body\" (markdown)."
    ));
    with_instructions(
        body,
        instructions.general.as_deref(),
        instructions.pr.as_deref(),
    )
}

/// The block an agent answers a commit plan with.
pub const COMMITS_FENCE: &str = "circles-commits";

/// Asks for the uncommitted changes to be split into atomic commits.
pub fn commit_plan(
    files: &[(String, String)],
    changes: &str,
    instructions: &Instructions,
    docs: &RepoDocs,
) -> String {
    let mut body = String::from(
        "Split these uncommitted changes into atomic commits: each commit is one logical change \
         that makes sense on its own, with a short message in the imperative mood (for example \
         \"Add pagination metadata\"). Put files that belong together in the same commit. Every \
         file must be in exactly one commit.\n\nFiles:\n",
    );
    for (path, status) in files {
        body.push_str(&format!("- {status} {path}\n"));
    }
    if !changes.trim().is_empty() {
        let (shown, cut) = match changes.char_indices().nth(DIFF_LIMIT) {
            Some((end, _)) => (&changes[..end], true),
            None => (changes, false),
        };
        body.push_str(&format!("\nChanges:\n```diff\n{}\n```\n", shown.trim_end()));
        if cut {
            body.push_str("(The diff was cut here because it is long.)\n");
        }
    }
    body.push_str(&section(docs, false));
    body.push_str(&format!(
        "\nDo not use any tool and do not run git. Reply with one fenced code block labelled \
         {COMMITS_FENCE} that holds a JSON array of objects with \"message\" and \"files\" (the \
         paths exactly as listed), in the order the commits should be made. Do not add a line \
         that credits Claude or Claude Code."
    ));
    with_instructions(
        body,
        instructions.general.as_deref(),
        instructions.commits.as_deref(),
    )
}

/// The body without lines that credit a tool, such as "Generated with Claude Code" or a
/// `Co-Authored-By: Claude` trailer. They are never wanted in a pull request.
pub fn without_attribution(body: &str) -> String {
    let lowered = |line: &str| line.to_lowercase();
    let kept: Vec<&str> = body
        .lines()
        .filter(|line| {
            let l = lowered(line);
            !(l.contains("generated with")
                && (l.contains("claude code") || l.contains("claude.com/claude-code")))
                && !l.contains("claude.com/claude-code")
                && !(l.trim_start().starts_with("co-authored-by:") && l.contains("claude"))
        })
        .collect();
    kept.join("\n").trim().to_string()
}

/// A title and body from the commits, for when nobody drafts one.
pub fn fallback_title(branch: &str, commits: &[String]) -> String {
    match commits {
        [only] => only.clone(),
        [first, ..] => first.clone(),
        [] => branch
            .rsplit('/')
            .next()
            .unwrap_or(branch)
            .replace(['-', '_'], " "),
    }
}

pub fn fallback_body(commits: &[String]) -> String {
    if commits.len() <= 1 {
        return String::new();
    }
    commits
        .iter()
        .map(|c| format!("- {c}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::pr::{CheckBucket, ThreadComment};

    fn check(name: &str, workflow: &str, link: &str) -> Check {
        Check {
            name: name.into(),
            workflow: workflow.into(),
            bucket: CheckBucket::Fail,
            state: "failure".into(),
            description: String::new(),
            link: link.into(),
            run_id: None,
        }
    }

    fn instructions() -> Instructions {
        Instructions {
            general: Some("Follow CONTRIBUTING.md.".into()),
            fix_errors: Some("Run the whole suite.".into()),
            review: Some("Be polite.".into()),
            conflicts: Some("Prefer rebasing.".into()),
            pr: Some("Imperative title.".into()),
            ..Instructions::default()
        }
    }

    #[test]
    fn the_fix_prompt_has_each_failing_check_with_its_log() {
        let text = fix_checks(
            12,
            &[
                (
                    check("build", "CI", "https://x/runs/1"),
                    Some("error: boom\n".into()),
                ),
                (check("lint", "", ""), None),
            ],
            &instructions(),
        );
        assert!(text.starts_with("Some checks are failing on pull request #12."));
        assert!(text.contains("## build (CI)\nhttps://x/runs/1"));
        assert!(text.contains("```\nerror: boom\n```"));
        assert!(text.contains("## lint\n\nNo log was available"));
        assert!(text.contains("Repository instructions:\nFollow CONTRIBUTING.md."));
        assert!(text.contains("Repository instructions:\nRun the whole suite."));
        assert!(
            !text.contains("Be polite."),
            "other tasks' instructions stay out"
        );
    }

    #[test]
    fn a_single_failing_check_is_worded_for_one() {
        let text = fix_checks(3, &[(check("t", "", ""), None)], &Instructions::default());
        assert!(text.starts_with("A check is failing on pull request #3."));
        assert!(!text.contains("Repository instructions"));
    }

    #[test]
    fn the_review_prompt_lists_only_unresolved_threads_with_their_place() {
        let threads = vec![
            ReviewThread {
                id: "1".into(),
                path: "src/a.ts".into(),
                line: Some(12),
                resolved: false,
                outdated: false,
                comments: vec![ThreadComment {
                    author: "alice".into(),
                    body: " Rename this ".into(),
                    url: String::new(),
                }],
            },
            ReviewThread {
                id: "2".into(),
                path: "b.rs".into(),
                line: None,
                resolved: true,
                outdated: false,
                comments: vec![],
            },
            ReviewThread {
                id: "3".into(),
                path: "c.rs".into(),
                line: Some(1),
                resolved: false,
                outdated: true,
                comments: vec![],
            },
        ];
        let text = respond_to_review(9, &threads, &instructions());
        assert!(text.contains("## src/a.ts:12\nalice: Rename this"));
        assert!(!text.contains("b.rs"));
        assert!(text.contains("## c.rs:1 (the code has changed since)"));
        assert!(text.contains("Be polite."));
        assert!(!text.contains("Prefer rebasing."));
    }

    #[test]
    fn the_conflict_prompt_names_the_base_and_adds_its_instructions() {
        let text = resolve_conflicts("main", &instructions());
        assert!(text.contains("conflicts with main"));
        assert!(text.contains("Prefer rebasing."));
        assert!(!text.contains("Run the whole suite."));
    }

    #[test]
    fn the_draft_prompt_asks_for_the_labelled_block_and_lists_commits() {
        let text = draft_pr(
            "main",
            &["Add login".into(), "Fix typo".into()],
            "diff --git a/x b/x\n+hello",
            &instructions(),
            &RepoDocs::default(),
        );
        assert!(text.contains("into main"));
        assert!(text.contains("- Add login\n- Fix typo"));
        assert!(text.contains("circles-pr"));
        assert!(text.contains("Imperative title."));
        assert!(text.contains("do not create the pull request"));
        assert!(text.contains("```diff\ndiff --git a/x b/x\n+hello\n```"));
        assert!(text.contains("do not need to run any tool"));
        assert!(!text.contains("was cut"));
        let long = draft_pr(
            "main",
            &[],
            &"x".repeat(DIFF_LIMIT + 50),
            &Instructions::default(),
            &RepoDocs::default(),
        );
        assert!(long.contains("The diff was cut here"));
        assert!(!long.contains(&"x".repeat(DIFF_LIMIT + 1)));
        assert!(!draft_pr(
            "main",
            &[],
            "",
            &Instructions::default(),
            &RepoDocs::default()
        )
        .contains("Changes:"));
    }

    #[test]
    fn the_commit_prompt_lists_files_and_asks_for_the_labelled_plan() {
        let text = commit_plan(
            &[
                ("src/a.ts".into(), "M".into()),
                ("new.ts".into(), "A".into()),
            ],
            "diff --git a/src/a.ts b/src/a.ts\n+x",
            &Instructions {
                commits: Some("Conventional commits.".into()),
                ..Instructions::default()
            },
            &RepoDocs::default(),
        );
        assert!(text.contains("- M src/a.ts\n- A new.ts"));
        assert!(text.contains("```diff"));
        assert!(text.contains("circles-commits"));
        assert!(text.contains("Do not use any tool"));
        assert!(text.contains("Repository instructions:\nConventional commits."));
        assert!(
            !commit_plan(&[], "", &Instructions::default(), &RepoDocs::default())
                .contains("Changes:")
        );
    }

    #[test]
    fn what_the_repository_wrote_down_goes_into_the_prompts() {
        let docs = RepoDocs {
            pr_template: Some("## Summary\n## Test plan".into()),
            conventions: Some("From CLAUDE.md:\nTitles start with the ticket.".into()),
        };
        let pr = draft_pr("main", &[], "", &Instructions::default(), &docs);
        assert!(pr.contains("```markdown\n## Summary\n## Test plan\n```"));
        assert!(pr.contains("Titles start with the ticket."));
        // The template and the rules come before the request for the answer.
        assert!(pr.find("## Summary").unwrap() < pr.find("circles-pr").unwrap());
        let commits = commit_plan(&[], "", &Instructions::default(), &docs);
        assert!(commits.contains("Titles start with the ticket."));
        assert!(!commits.contains("## Test plan"));
    }

    #[test]
    fn attribution_lines_are_removed_from_a_description() {
        let body = "## What\nA login form.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\nCo-Authored-By: Claude <noreply@anthropic.com>\nCo-authored-by: Alice <a@x.com>\n";
        assert_eq!(
            without_attribution(body),
            "## What\nA login form.\n\nCo-authored-by: Alice <a@x.com>"
        );
        assert_eq!(without_attribution("Generated with Claude Code"), "");
        assert_eq!(without_attribution("Plain text"), "Plain text");
        assert_eq!(
            without_attribution("Generated with a code generator"),
            "Generated with a code generator"
        );
    }

    #[test]
    fn a_fallback_title_and_body_come_from_the_commits_or_the_branch() {
        assert_eq!(fallback_title("me/x", &["Add login".into()]), "Add login");
        assert_eq!(
            fallback_title("me/x", &["Newest".into(), "Older".into()]),
            "Newest"
        );
        assert_eq!(fallback_title("me/fix-the_login", &[]), "fix the login");
        assert_eq!(fallback_body(&["only".into()]), "");
        assert_eq!(fallback_body(&["a".into(), "b".into()]), "- a\n- b");
    }
}
