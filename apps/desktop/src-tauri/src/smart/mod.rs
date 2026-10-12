//! Small text jobs done by a small model that ships inside Circles: a title for a chat, and a
//! suggestion for the next message. Nothing here may block a chat. Every call has a timeout, and
//! when no model is available the callers keep what they already have.

pub mod claude;
pub mod embedded;
pub mod naming;
pub mod sanitize;
pub mod service;

use std::time::Duration;

use async_trait::async_trait;

/// How long a model has to answer before the caller moves on.
///
/// The first answer after the model was unloaded includes loading it, and the Mac may be busy with
/// a build or a test run, so this is generous. The caller never waits on it.
pub const TIMEOUT: Duration = Duration::from_secs(25);

/// The most characters of someone's text that are given to a model. A title needs the start of a
/// message, and a long one only slows the model down.
pub const INPUT_LIMIT: usize = 1500;

/// Something that turns a short instruction and a short text into a short answer.
#[async_trait]
pub trait TextModel: Send + Sync {
    /// Whether this provider can answer now. Cheap.
    fn available(&self) -> bool;
    /// A short answer, or `None` when the model cannot help or took too long.
    ///
    /// `shots` are examples of the wanted answer, as (what the user wrote, what to reply). Small
    /// models follow an example far better than an instruction.
    async fn complete(
        &self,
        system: &str,
        shots: &[(&str, &str)],
        user: &str,
        max_tokens: u32,
    ) -> Option<String>;
}

/// Cuts text to at most `max` characters on a character boundary.
pub fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub const TITLE_SYSTEM: &str = "You name conversations. Reply with a title of 3 to 5 words that names the task the user typed. Never ask a question and never answer it. Reply with the title only.";

pub const TITLE_SHOTS: &[(&str, &str)] = &[
    (
        "can you explain how oauth refresh tokens work",
        "OAuth refresh tokens explained",
    ),
    (
        "fix the crash when the settings file is empty",
        "Fix empty settings crash",
    ),
    (
        "add a dark mode toggle to the settings page and remember it",
        "Add dark mode toggle",
    ),
    (
        "Review the open pull requests and tell me which are safe to merge",
        "Review open pull requests",
    ),
];

pub const SUGGEST_SYSTEM: &str = "You write the message a user would send next to a coding assistant. Reply with one short imperative sentence of at most 12 words, as a request, never a summary. Reply with the sentence only.";

pub const SUGGEST_SHOTS: &[(&str, &str)] = &[
    (
        "User asked: fix the failing checkout test\nAssistant answered: The discount was applied after tax. I moved it before tax and the test passes.",
        "Open a pull request for this fix",
    ),
    (
        "User asked: why is the build slow\nAssistant answered: The tests run serially and the cache is cold. Running them in parallel would cut it by half.",
        "Run the tests in parallel and measure again",
    ),
    (
        "User asked: add a dark mode toggle\nAssistant answered: Added the toggle to Settings and saved the choice in local storage.",
        "Add tests for the dark mode toggle",
    ),
];

/// A title for a chat that began with `first_message`. `None` when there is no good one.
pub async fn title_for(model: &dyn TextModel, first_message: &str) -> Option<String> {
    if !model.available() || first_message.trim().is_empty() {
        return None;
    }
    // A brief is long, with steps and notes. What the task is stands in its first paragraph, and
    // a small model given the rest tends to answer with one of its examples instead.
    let text = title_source(first_message);
    let raw = model.complete(TITLE_SYSTEM, TITLE_SHOTS, &text, 16).await?;
    let title = sanitize::title(&raw)?;
    if echoes_example(&title, &text) {
        tracing::info!(title = %title, "the model repeated an example, so the first words stay");
        return None;
    }
    Some(title)
}

/// The first paragraph of a message, cut short: the part a title is made from.
fn title_source(message: &str) -> String {
    let first = message
        .trim()
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty())
        .unwrap_or("");
    clip(first, TITLE_INPUT_LIMIT.min(INPUT_LIMIT))
}

/// How much of a message a title is made from.
const TITLE_INPUT_LIMIT: usize = 300;

/// Whether `title` is one of the examples given to the model, for a text that is not about it.
fn echoes_example(title: &str, text: &str) -> bool {
    if !TITLE_SHOTS
        .iter()
        .any(|(_, a)| a.eq_ignore_ascii_case(title))
    {
        return false;
    }
    let text = text.to_lowercase();
    let words: Vec<String> = title
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 4)
        .collect();
    if words.is_empty() {
        return false;
    }
    let found = words.iter().filter(|w| text.contains(w.as_str())).count();
    found * 2 < words.len()
}

pub const FIRST_SYSTEM: &str = "You write the first message a user would send to a work assistant, given what the assistant is for. Reply with one short imperative sentence of at most 14 words, as a request. Reply with the sentence only.";

pub const FIRST_SHOTS: &[(&str, &str)] = &[
    (
        "Assistant: Release captain\nWhat it does: Keeps main green, opens release pull requests and merges them when checks pass.",
        "Check which pull requests are ready to merge",
    ),
    (
        "Assistant: Ticket triager\nWhat it does: Reads Jira tickets assigned to me and starts a workspace for each one.",
        "List my open Jira tickets and start on the first one",
    ),
    (
        "Assistant: Code checker\nWhat it does: Reviews committed changes before a pull request is created.",
        "Review the latest commits before I open a pull request",
    ),
];

/// An example of a first message for an assistant with this name and purpose.
pub async fn first_prompt_for(model: &dyn TextModel, name: &str, purpose: &str) -> Option<String> {
    if !model.available() || (name.trim().is_empty() && purpose.trim().is_empty()) {
        return None;
    }
    let text = format!(
        "Assistant: {}\nWhat it does: {}",
        clip(name.trim(), 80),
        clip(purpose.trim(), 900)
    );
    let raw = model.complete(FIRST_SYSTEM, FIRST_SHOTS, &text, 28).await?;
    sanitize::suggestion(&raw)
}

/// One likely next message, after `asked` was answered with `answer`.
pub async fn suggestion_for(model: &dyn TextModel, asked: &str, answer: &str) -> Option<String> {
    if !model.available() || asked.trim().is_empty() {
        return None;
    }
    let text = format!(
        "User asked: {}\nAssistant answered: {}",
        clip(asked.trim(), 600),
        clip(answer.trim(), 900)
    );
    let raw = model
        .complete(SUGGEST_SYSTEM, SUGGEST_SHOTS, &text, 28)
        .await?;
    sanitize::suggestion(&raw)
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::Mutex;

    /// A provider that answers with a fixed text, or nothing, and keeps what it was asked.
    pub struct Fixed {
        pub answer: Option<String>,
        pub asked: Mutex<Vec<(String, String)>>,
    }

    impl Fixed {
        pub fn says(answer: &str) -> Self {
            Self {
                answer: Some(answer.into()),
                asked: Mutex::new(Vec::new()),
            }
        }
        pub fn silent() -> Self {
            Self {
                answer: None,
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl TextModel for Fixed {
        fn available(&self) -> bool {
            self.answer.is_some()
        }
        async fn complete(
            &self,
            system: &str,
            _shots: &[(&str, &str)],
            user: &str,
            _max: u32,
        ) -> Option<String> {
            self.asked
                .lock()
                .unwrap()
                .push((system.into(), user.into()));
            self.answer.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Fixed;
    use super::*;

    #[tokio::test]
    async fn a_title_is_cleaned_and_a_silent_model_gives_none() {
        let model = Fixed::says("\"Fix the failing checkout test.\"\n");
        assert_eq!(
            title_for(&model, "fix the failing test in checkout")
                .await
                .as_deref(),
            Some("Fix the failing checkout test")
        );
        assert!(title_for(&Fixed::silent(), "anything").await.is_none());
        assert!(title_for(&model, "   ").await.is_none());
    }

    #[tokio::test]
    async fn a_title_that_repeats_an_example_is_not_used() {
        // A long brief about something else, answered with the last example.
        let model = Fixed::says("Review open pull requests");
        let brief =
            "Add a theme toggle feature (light/dark) to this project.\n\nSteps:\n1. Explore.";
        assert!(title_for(&model, brief).await.is_none());
        // The same words, asked about pull requests, are the right title.
        let about = "Review the open pull requests and tell me which are safe";
        assert_eq!(
            title_for(&model, about).await.as_deref(),
            Some("Review open pull requests")
        );
    }

    #[tokio::test]
    async fn only_the_first_paragraph_of_a_brief_reaches_the_model() {
        let model = Fixed::says("Add theme toggle");
        title_for(
            &model,
            "Add a theme toggle.\n\nSteps:\n1. Explore the repo.\n2. Review.",
        )
        .await;
        let asked = model.asked.lock().unwrap();
        assert_eq!(asked[0].1, "Add a theme toggle.");
    }

    #[tokio::test]
    async fn long_messages_are_cut_before_they_reach_the_model() {
        let model = Fixed::says("A title");
        title_for(&model, &"word ".repeat(2000)).await;
        let asked = model.asked.lock().unwrap();
        assert!(asked[0].1.chars().count() <= INPUT_LIMIT);
    }

    #[tokio::test]
    async fn a_first_message_example_is_asked_from_the_name_and_purpose() {
        let model = Fixed::says("\"List my open Jira tickets.\"");
        let example = first_prompt_for(&model, "Ticket triager", "Reads my Jira tickets")
            .await
            .unwrap();
        assert_eq!(example, "List my open Jira tickets.");
        {
            let asked = model.asked.lock().unwrap();
            assert!(asked[0].1.contains("Assistant: Ticket triager"));
            assert!(asked[0].1.contains("Reads my Jira tickets"));
        }
        assert!(first_prompt_for(&model, "", "  ").await.is_none());
        assert!(first_prompt_for(&Fixed::silent(), "A", "B").await.is_none());
    }

    #[tokio::test]
    async fn a_suggestion_is_one_short_sentence() {
        let model = Fixed::says("Open a pull request for these changes.\nAlso do more things");
        let s = suggestion_for(&model, "fix it", "done").await.unwrap();
        assert_eq!(s, "Open a pull request for these changes.");
    }
}
