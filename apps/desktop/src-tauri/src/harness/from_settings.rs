//! What the settings change about chats: how a new one starts, and how Claude Code is run.

use std::path::PathBuf;

use super::sessions::{AgentSettings, NotificationKind};
use super::store::{NewSession, PermissionMode};
use crate::settings::Settings;

/// Fills in what the person did not choose for a new chat from their defaults. A choice made in
/// the Create dialog or the chat always wins.
pub fn new_session_defaults(settings: &Settings, mut new: NewSession) -> NewSession {
    if new.model.as_deref().is_none_or(|m| m.trim().is_empty()) {
        new.model = settings.models.model.clone();
    }
    if new.effort.as_deref().is_none_or(|e| e.trim().is_empty()) {
        new.effort = settings.models.effort.clone();
    }
    if new.permission_mode.is_none() {
        new.permission_mode = settings
            .agents
            .permission_mode
            .as_deref()
            .and_then(PermissionMode::parse)
            .or((settings.models.plan_by_default == Some(true)).then_some(PermissionMode::Plan));
    }
    new
}

pub fn agent_settings(settings: &Settings) -> AgentSettings {
    AgentSettings {
        claude_path: settings
            .agents
            .claude_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .map(PathBuf::from),
        extra_args: settings.agents.extra_args.clone(),
    }
}

/// What to do when an agent finishes or needs the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyPlan {
    pub notification: bool,
    pub sound: bool,
}

/// A message and a sound are for when the app is in the background: someone looking at the app sees
/// it already. Both are on unless switched off. A failure is told like a finished turn.
pub fn notify_plan(settings: &Settings, app_focused: bool) -> NotifyPlan {
    if app_focused {
        return NotifyPlan {
            notification: false,
            sound: false,
        };
    }
    NotifyPlan {
        notification: settings.general.notifications.unwrap_or(true),
        sound: settings.general.sound.unwrap_or(true),
    }
}

/// The words of the message.
pub fn notification_text(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::Finished => "The agent finished.",
        NotificationKind::NeedsApproval => "The agent needs your approval.",
        NotificationKind::Failed => "Something went wrong.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(toml_text: &str) -> Settings {
        toml::from_str(toml_text).unwrap()
    }

    #[test]
    fn defaults_fill_in_what_was_not_chosen() {
        let s = settings("[models]\nmodel = \"opus\"\neffort = \"high\"\n[agents]\npermission_mode = \"accept_edits\"");
        let new = new_session_defaults(&s, NewSession::default());
        assert_eq!(new.model.as_deref(), Some("opus"));
        assert_eq!(new.effort.as_deref(), Some("high"));
        assert_eq!(new.permission_mode, Some(PermissionMode::AcceptEdits));
    }

    #[test]
    fn a_choice_wins_over_a_default() {
        let s = settings("[models]\nmodel = \"opus\"\neffort = \"high\"\nplan_by_default = true\n[agents]\npermission_mode = \"bypass\"");
        let chosen = NewSession {
            model: Some("haiku".into()),
            effort: Some("low".into()),
            permission_mode: Some(PermissionMode::Manual),
            ..NewSession::default()
        };
        let new = new_session_defaults(&s, chosen);
        assert_eq!(new.model.as_deref(), Some("haiku"));
        assert_eq!(new.effort.as_deref(), Some("low"));
        assert_eq!(new.permission_mode, Some(PermissionMode::Manual));
        // An empty choice counts as no choice.
        let blank = NewSession {
            model: Some(" ".into()),
            ..NewSession::default()
        };
        assert_eq!(
            new_session_defaults(&s, blank).model.as_deref(),
            Some("opus")
        );
    }

    #[test]
    fn plan_by_default_starts_chats_in_plan_mode_unless_a_mode_is_set() {
        let plan = settings("[models]\nplan_by_default = true");
        assert_eq!(
            new_session_defaults(&plan, NewSession::default()).permission_mode,
            Some(PermissionMode::Plan)
        );
        let both =
            settings("[models]\nplan_by_default = true\n[agents]\npermission_mode = \"manual\"");
        assert_eq!(
            new_session_defaults(&both, NewSession::default()).permission_mode,
            Some(PermissionMode::Manual)
        );
        let off = settings("[models]\nplan_by_default = false");
        assert_eq!(
            new_session_defaults(&off, NewSession::default()).permission_mode,
            None
        );
        assert_eq!(
            new_session_defaults(&Settings::default(), NewSession::default()).model,
            None
        );
    }

    #[test]
    fn the_agent_settings_come_from_the_agents_section() {
        let s = settings(
            "[agents]\nclaude_path = \"/opt/claude\"\nextra_args = [\"--add-dir\", \"/data\"]",
        );
        let agent = agent_settings(&s);
        assert_eq!(agent.claude_path, Some(PathBuf::from("/opt/claude")));
        assert_eq!(agent.extra_args, ["--add-dir", "/data"]);
        let none = agent_settings(&Settings::default());
        assert_eq!(none.claude_path, None);
        assert!(none.extra_args.is_empty());
        assert_eq!(
            agent_settings(&settings("[agents]\nclaude_path = \"  \"")).claude_path,
            None
        );
    }

    #[test]
    fn a_message_and_a_sound_are_for_the_background_and_follow_the_switches() {
        let defaults = Settings::default();
        assert_eq!(
            notify_plan(&defaults, false),
            NotifyPlan {
                notification: true,
                sound: true
            }
        );
        assert_eq!(
            notify_plan(&defaults, true),
            NotifyPlan {
                notification: false,
                sound: false
            },
            "the app is in front"
        );
        let quiet = settings("[general]\nnotifications = false\nsound = false");
        assert_eq!(
            notify_plan(&quiet, false),
            NotifyPlan {
                notification: false,
                sound: false
            }
        );
        let silent = settings("[general]\nsound = false");
        assert_eq!(
            notify_plan(&silent, false),
            NotifyPlan {
                notification: true,
                sound: false
            }
        );
        let only_sound = settings("[general]\nnotifications = false");
        assert_eq!(
            notify_plan(&only_sound, false),
            NotifyPlan {
                notification: false,
                sound: true
            }
        );
    }

    #[test]
    fn each_kind_has_its_words() {
        assert_eq!(
            notification_text(NotificationKind::Finished),
            "The agent finished."
        );
        assert_eq!(
            notification_text(NotificationKind::NeedsApproval),
            "The agent needs your approval."
        );
        assert_eq!(
            notification_text(NotificationKind::Failed),
            "Something went wrong."
        );
    }
}
