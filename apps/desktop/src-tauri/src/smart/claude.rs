//! Claude Haiku through Claude Code, for the small text jobs: a title, a suggestion. It spends the
//! person's Claude subscription and sends the text to Anthropic, a little for each job.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::TextModel;
use crate::env::ResolvedEnv;

pub struct ClaudeHaiku {
    pub program: PathBuf,
    pub env: std::sync::Arc<ResolvedEnv>,
}

/// The arguments for one job. The text to work on is not among them: it goes in through stdin, so
/// a message that starts with a dash cannot be read as an option.
fn arguments(instruction: &str) -> Vec<String> {
    [
        "-p",
        "--model",
        "haiku",
        // No tools, no commands, nothing saved in the person's history.
        "--tools",
        "",
        "--disable-slash-commands",
        "--no-session-persistence",
        // Not the person's own settings and CLAUDE.md: only the instruction shapes the answer.
        "--setting-sources",
        "project,local",
        "--append-system-prompt",
        instruction,
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[async_trait]
impl TextModel for ClaudeHaiku {
    fn available(&self) -> bool {
        self.program.exists()
    }

    async fn complete(
        &self,
        system: &str,
        shots: &[(&str, &str)],
        user: &str,
        _max: u32,
    ) -> Option<String> {
        // The examples go in the instruction, as plain lines.
        let mut instruction = system.to_string();
        if !shots.is_empty() {
            instruction.push_str("\n\nExamples:\n");
            for (input, output) in shots {
                instruction.push_str(&format!("Input: {input}\nReply: {output}\n"));
            }
        }
        let mut command = Command::new(&self.program);
        self.env.apply(&mut command);
        command
            .args(arguments(&instruction))
            // Away from any project, so no project file is read either.
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().ok()?;
        let mut stdin = child.stdin.take()?;
        stdin.write_all(user.as_bytes()).await.ok()?;
        // Closing the pipe tells Claude Code the text is complete.
        drop(stdin);
        let out = tokio::time::timeout(Duration::from_secs(25), child.wait_with_output())
            .await
            .ok()?
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(dir: &std::path::Path, script: &str) -> ClaudeHaiku {
        let path = dir.join("claude");
        std::fs::write(&path, script).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        ClaudeHaiku {
            program: path,
            env: std::sync::Arc::new(ResolvedEnv::for_test(&[
                ("PATH", "/usr/bin:/bin"),
                ("LOG", dir.join("log").to_str().unwrap()),
            ])),
        }
    }

    #[tokio::test]
    async fn the_text_goes_in_through_stdin_and_the_run_is_isolated() {
        let tmp = tempfile::tempdir().unwrap();
        let model = fake(
            tmp.path(),
            "#!/bin/sh\necho \"$@\" > \"$LOG\"\ncat >> \"$LOG\"\necho 'Fix empty settings crash'\n",
        );
        assert!(model.available());
        let out = model
            .complete(
                "You name conversations.",
                &[("a", "b")],
                "- fix the crash",
                16,
            )
            .await
            .unwrap();
        assert_eq!(out.trim(), "Fix empty settings crash");
        let log = std::fs::read_to_string(tmp.path().join("log")).unwrap();
        for wanted in [
            "--model haiku",
            "--no-session-persistence",
            "--disable-slash-commands",
            "--setting-sources project,local",
            "You name conversations.",
            "Input: a\nReply: b",
        ] {
            assert!(log.contains(wanted), "{wanted} not in {log}");
        }
        assert!(
            log.trim_end().ends_with("- fix the crash"),
            "the text is the stdin, not an argument: {log}"
        );
    }

    #[tokio::test]
    async fn a_failing_run_gives_no_answer() {
        let tmp = tempfile::tempdir().unwrap();
        let model = fake(tmp.path(), "#!/bin/sh\ncat > /dev/null\nexit 1\n");
        assert_eq!(model.complete("s", &[], "x", 16).await, None);
    }
}
