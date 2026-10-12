//! What a Composer may do: the autonomy of each tool, the projects it may touch and its limits.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Tier, ToolDef};

/// What happens when the Composer calls a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    /// It runs.
    Allow,
    /// It waits for a person to approve.
    Ask,
    /// The Composer does not have the tool.
    Off,
}

impl Autonomy {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "allow" => Some(Autonomy::Allow),
            "ask" => Some(Autonomy::Ask),
            "off" => Some(Autonomy::Off),
            _ => None,
        }
    }
}

/// Reading is free. Everything else asks until the person says otherwise.
pub fn default_autonomy(tier: Tier) -> Autonomy {
    match tier {
        Tier::Read => Autonomy::Allow,
        _ => Autonomy::Ask,
    }
}

/// How much a Composer does on its own. The same four choices a chat has for its permission mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// Looking is free, everything else asks.
    #[default]
    Manual,
    /// Changes on this Mac go ahead. Anything visible outside it, and anything that deletes, asks.
    AcceptEdits,
    /// It may only look and make a plan. It has no tool that changes anything.
    Plan,
    /// Nothing asks. Merging past failing checks still needs its own switch.
    Bypass,
}

impl Mode {
    pub const ALL: [&'static str; 4] = ["manual", "accept_edits", "plan", "bypass"];

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "manual" => Some(Mode::Manual),
            "accept_edits" => Some(Mode::AcceptEdits),
            "plan" => Some(Mode::Plan),
            "bypass" => Some(Mode::Bypass),
            _ => None,
        }
    }

    /// What the mode does with a tool of this tier.
    pub fn autonomy(self, tier: Tier) -> Autonomy {
        match (self, tier) {
            (Mode::Bypass, _) => Autonomy::Allow,
            (_, Tier::Read) => Autonomy::Allow,
            (Mode::Manual, _) => Autonomy::Ask,
            (Mode::AcceptEdits, Tier::Write) => Autonomy::Allow,
            (Mode::AcceptEdits, _) => Autonomy::Ask,
            (Mode::Plan, _) => Autonomy::Off,
        }
    }
}

/// How much a run may use. Reaching a limit makes the next call refuse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct Limits {
    /// Tool calls in one run.
    pub max_steps: u32,
    /// Chats started by a run that are working at the same moment.
    pub max_children_at_once: u32,
    /// Workspaces and chats one run may start.
    pub max_children_total: u32,
    /// Minutes since the run began.
    pub max_minutes: u32,
    /// Passed to Claude Code as its own spending limit.
    pub max_budget_usd: Option<f64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_steps: 100,
            max_children_at_once: 3,
            max_children_total: 10,
            max_minutes: 120,
            max_budget_usd: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Policy {
    /// What the Composer does with a tool that has no setting of its own.
    pub mode: Mode,
    pub autonomy: BTreeMap<String, Autonomy>,
    /// Projects the Composer may touch. `None` means all.
    pub projects: Option<Vec<String>>,
    pub limits: Limits,
    /// Whether `merge_pr` may be told to ignore the merge blockers.
    pub allow_override: bool,
}

impl Policy {
    pub fn autonomy_of(&self, def: &ToolDef) -> Autonomy {
        self.autonomy
            .get(def.name)
            .copied()
            .unwrap_or_else(|| self.mode.autonomy(def.tier))
    }

    /// Whether the Composer is told about the tool at all.
    pub fn exposes(&self, def: &ToolDef) -> bool {
        self.autonomy_of(def) != Autonomy::Off
    }

    pub fn allows_project(&self, project_id: &str) -> bool {
        self.projects
            .as_ref()
            .is_none_or(|list| list.iter().any(|p| p == project_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::catalog;

    #[test]
    fn defaults_ask_for_everything_but_reading() {
        let policy = Policy::default();
        for def in catalog::all() {
            let want = if def.tier == Tier::Read {
                Autonomy::Allow
            } else {
                Autonomy::Ask
            };
            assert_eq!(policy.autonomy_of(&def), want, "{}", def.name);
        }
    }

    #[test]
    fn each_mode_decides_by_tier() {
        use Autonomy::{Allow, Ask, Off};
        let table = [
            (Mode::Manual, [Allow, Ask, Ask, Ask]),
            (Mode::AcceptEdits, [Allow, Allow, Ask, Ask]),
            (Mode::Plan, [Allow, Off, Off, Off]),
            (Mode::Bypass, [Allow, Allow, Allow, Allow]),
        ];
        for (mode, want) in table {
            let got = [Tier::Read, Tier::Write, Tier::External, Tier::Destructive]
                .map(|t| mode.autonomy(t));
            assert_eq!(got, want, "{mode:?}");
        }
    }

    #[test]
    fn modes_are_named_like_the_chat_modes() {
        for name in Mode::ALL {
            assert!(Mode::parse(name).is_some(), "{name}");
        }
        assert_eq!(Mode::parse("bypass"), Some(Mode::Bypass));
        assert_eq!(Mode::parse("yolo"), None);
        assert_eq!(Mode::default(), Mode::Manual);
    }

    #[test]
    fn a_tool_with_its_own_setting_keeps_it_whatever_the_mode() {
        let mut policy = Policy {
            mode: Mode::Bypass,
            ..Policy::default()
        };
        let merge = catalog::get("merge_pr").unwrap();
        assert_eq!(policy.autonomy_of(&merge), Autonomy::Allow);
        policy.autonomy.insert("merge_pr".into(), Autonomy::Off);
        assert!(!policy.exposes(&merge), "off beats bypass");
        policy.mode = Mode::Plan;
        policy
            .autonomy
            .insert("create_workspace".into(), Autonomy::Ask);
        assert_eq!(
            policy.autonomy_of(&catalog::get("create_workspace").unwrap()),
            Autonomy::Ask
        );
    }

    #[test]
    fn a_setting_beats_the_default_and_off_hides_the_tool() {
        let mut policy = Policy::default();
        let merge = catalog::get("merge_pr").unwrap();
        policy.autonomy.insert("merge_pr".into(), Autonomy::Allow);
        assert_eq!(policy.autonomy_of(&merge), Autonomy::Allow);
        policy.autonomy.insert("merge_pr".into(), Autonomy::Off);
        assert!(!policy.exposes(&merge));
        assert!(policy.exposes(&catalog::get("list_projects").unwrap()));
    }

    #[test]
    fn projects_can_be_limited() {
        let mut policy = Policy::default();
        assert!(policy.allows_project("any"));
        policy.projects = Some(vec!["p1".into()]);
        assert!(policy.allows_project("p1"));
        assert!(!policy.allows_project("p2"));
        policy.projects = Some(vec![]);
        assert!(!policy.allows_project("p1"));
    }

    #[test]
    fn limits_fall_back_to_defaults_for_what_is_missing() {
        let limits: Limits = serde_json::from_str(r#"{"max_steps": 5}"#).unwrap();
        assert_eq!(limits.max_steps, 5);
        assert_eq!(limits.max_children_at_once, 3);
        assert_eq!(limits.max_budget_usd, None);
    }
}
