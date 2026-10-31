//! Drafting a pull request's title and description with a single, tool-less Claude Code call, so
//! no chat is involved.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use ts_rs::TS;

use super::prompts::{without_attribution, PR_FENCE};
use crate::env::ResolvedEnv;
use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrDraft {
    pub title: String,
    pub body: String,
}

/// The title and description in the last `circles-pr` block of a reply.
pub fn parse_draft(text: &str) -> Option<PrDraft> {
    let mut found = None;
    for part in text.split(&format!("```{PR_FENCE}")).skip(1) {
        let Some(body) = part.split("```").next() else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
            continue;
        };
        let title = value
            .get("title")
            .and_then(|t| t.as_str())
            .map(str::trim)
            .unwrap_or("");
        if title.is_empty() {
            continue;
        }
        let description = value.get("body").and_then(|b| b.as_str()).unwrap_or("");
        found = Some(PrDraft {
            title: without_attribution(title),
            body: without_attribution(description),
        });
    }
    found
}

/// Asks Claude Code once, with every tool turned off and nothing saved, and returns its answer.
/// The prompt goes in through stdin because it can be long.
pub async fn run_once(
    env: &ResolvedEnv,
    cwd: &Path,
    prompt: &str,
    model: &str,
) -> AppResult<String> {
    run_once_with(env, cwd, prompt, model, &[]).await
}

/// Like `run_once`, with more arguments for Claude Code, such as which settings to read.
pub async fn run_once_with(
    env: &ResolvedEnv,
    cwd: &Path,
    prompt: &str,
    model: &str,
    extra: &[&str],
) -> AppResult<String> {
    let bin = env.which("claude").ok_or_else(|| {
        AppError::ClaudeMissing(
            "Claude Code (`claude`) was not found. Install it from https://claude.com/claude-code, then run `claude` once to sign in."
                .into(),
        )
    })?;
    let mut cmd = Command::new(bin);
    cmd.args([
        "-p",
        "--model",
        model,
        "--no-session-persistence",
        "--tools",
        "",
        "--disable-slash-commands",
        "--output-format",
        "text",
    ])
    .args(extra)
    .current_dir(cwd)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    env.apply(&mut cmd);
    cmd.env("NO_COLOR", "1");

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Internal(format!("cannot start Claude Code: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        // A program that fails at once, such as one with a bad key, never reads the prompt, and
        // writing to it then fails with a broken pipe. That is not what went wrong: what it said
        // is, and it is read below.
        if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                return Err(e.into());
            }
        }
        // Dropping closes the pipe, which tells Claude Code the prompt is complete.
    }
    let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            AppError::Internal("The agent took too long to draft the pull request.".into())
        })??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("it stopped without saying why");
        return Err(AppError::Internal(format!(
            "The agent could not draft it: {}",
            first.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(dir: &Path, script: &str) -> ResolvedEnv {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let path = bin.join("claude");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path_var = format!("{}:/usr/bin:/bin", bin.display());
        ResolvedEnv::for_test(&[
            ("PATH", path_var.as_str()),
            ("LOG", dir.join("log").to_str().unwrap()),
        ])
    }

    #[test]
    fn the_title_and_body_come_from_the_last_labelled_block() {
        let text = "Sure.\n```circles-pr\n{\"title\":\"First\",\"body\":\"a\"}\n```\n```circles-pr\n{\"title\":\" Add login \",\"body\":\"A form.\\n\\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\"}\n```";
        assert_eq!(
            parse_draft(text),
            Some(PrDraft {
                title: "Add login".into(),
                body: "A form.".into()
            })
        );
    }

    #[test]
    fn answers_without_a_usable_block_give_nothing() {
        assert_eq!(parse_draft("no block"), None);
        assert_eq!(parse_draft("```circles-pr\nnot json\n```"), None);
        assert_eq!(
            parse_draft("```circles-pr\n{\"title\":\"  \",\"body\":\"x\"}\n```"),
            None
        );
        assert_eq!(parse_draft("```circles-pr\n{\"body\":\"x\"}\n```"), None);
        assert_eq!(
            parse_draft("```circles-pr\n{\"title\":\"T\"}\n```"),
            Some(PrDraft {
                title: "T".into(),
                body: String::new()
            })
        );
    }

    #[tokio::test]
    async fn claude_is_run_once_with_no_tools_and_nothing_saved() {
        let tmp = tempfile::tempdir().unwrap();
        let env = fake(
            tmp.path(),
            "#!/bin/sh\necho \"$@\" > \"$LOG\"\ncat >> \"$LOG\"\nprintf '```circles-pr\\n{\"title\":\"From fake\",\"body\":\"ok\"}\\n```\\n'\n",
        );
        let out = run_once(&env, tmp.path(), "Write a title\nfor this", "sonnet")
            .await
            .unwrap();
        assert_eq!(parse_draft(&out).unwrap().title, "From fake");
        let log = std::fs::read_to_string(tmp.path().join("log")).unwrap();
        assert!(log.contains("--model sonnet"), "{log}");
        assert!(log.contains("--no-session-persistence"), "{log}");
        assert!(log.contains("--disable-slash-commands"), "{log}");
        assert!(log.contains("-p"), "{log}");
        assert!(
            log.contains("Write a title\nfor this"),
            "the prompt goes through stdin: {log}"
        );
    }

    #[tokio::test]
    async fn extra_arguments_reach_claude_after_the_fixed_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let env = fake(tmp.path(), "#!/bin/sh\necho \"$@\" > \"$LOG\"\necho done\n");
        let out = run_once_with(
            &env,
            tmp.path(),
            "x",
            "sonnet",
            &["--setting-sources", "project,local"],
        )
        .await
        .unwrap();
        assert_eq!(out.trim(), "done");
        let log = std::fs::read_to_string(tmp.path().join("log")).unwrap();
        assert!(log.contains("--tools"), "{log}");
        assert!(log.contains("--setting-sources project,local"), "{log}");
    }

    #[tokio::test]
    async fn a_run_that_fails_before_reading_its_prompt_still_says_why() {
        let tmp = tempfile::tempdir().unwrap();
        // It exits at once without reading a byte, and the prompt is far larger than a pipe holds,
        // so writing it cannot succeed: this used to report "Broken pipe" instead of the message.
        let env = fake(
            tmp.path(),
            "#!/bin/sh\necho 'Invalid API key' >&2\nexit 1\n",
        );
        let big = "x".repeat(4 * 1024 * 1024);
        for _ in 0..5 {
            let err = run_once(&env, tmp.path(), &big, "haiku").await.unwrap_err();
            assert!(err.to_string().contains("Invalid API key"), "{err}");
        }
    }

    #[tokio::test]
    async fn a_failing_run_says_why_and_a_missing_claude_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let env = fake(
            tmp.path(),
            "#!/bin/sh\necho 'Invalid API key' >&2\nexit 1\n",
        );
        let err = run_once(&env, tmp.path(), "x", "haiku").await.unwrap_err();
        assert!(err.to_string().contains("Invalid API key"), "{err}");

        let none = ResolvedEnv::for_test(&[("PATH", "/nonexistent")]);
        assert_eq!(
            run_once(&none, tmp.path(), "x", "haiku")
                .await
                .unwrap_err()
                .code(),
            "claude_missing"
        );
    }

    /// Uses the real Claude Code and a little API credit:
    /// `cargo test --lib -- --ignored --nocapture real_claude`
    #[tokio::test]
    #[ignore = "uses the real Claude Code and a little API credit"]
    async fn real_claude_drafts_from_a_diff_without_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let env = crate::env::resolve().await;
        let prompt = crate::pr::prompts::draft_pr(
            "main",
            &["Add a greeting".into()],
            "diff --git a/hello.py b/hello.py\n+++ b/hello.py\n+print(\"hello\")\n",
            &crate::settings::Instructions::default(),
            &crate::pr::docs::RepoDocs::default(),
        );
        let answer = run_once(&env, tmp.path(), &prompt, "haiku").await.unwrap();
        println!("{answer}");
        let draft = parse_draft(&answer).expect("an answer in the asked form");
        assert!(!draft.title.is_empty());
        assert!(!draft.body.to_lowercase().contains("claude code"));
    }
}
