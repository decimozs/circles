//! Claude Haiku through Claude Code, for people who turn it on. It spends the subscription and
//! sends the text to Anthropic, which is why it is never chosen by itself.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use super::TextModel;
use crate::env::ResolvedEnv;

pub struct ClaudeHaiku {
    pub program: PathBuf,
    pub env: std::sync::Arc<ResolvedEnv>,
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
            .args([
                "-p",
                "--model",
                "haiku",
                // No tools, no MCP servers: it only writes a line of text.
                "--tools",
                "",
                "--append-system-prompt",
                &instruction,
                user,
            ])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(15), command.output())
            .await
            .ok()?
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
