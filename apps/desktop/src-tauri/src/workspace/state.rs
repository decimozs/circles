//! Workspace lifecycle. Every state change goes through `can_transition`, so an invalid change
//! fails in one place instead of leaving a workspace in a state nothing expects.

use std::str::FromStr;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceState {
    /// Worktree is being created (also while restoring an archived workspace).
    Creating,
    SetupRunning,
    Ready,
    Working,
    Idle,
    /// Waiting for the user, for example a permission prompt.
    Blocked,
    Archived,
    Errored,
}

use WorkspaceState::*;

pub const ALL_STATES: [WorkspaceState; 8] = [
    Creating,
    SetupRunning,
    Ready,
    Working,
    Idle,
    Blocked,
    Archived,
    Errored,
];

impl WorkspaceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Creating => "creating",
            SetupRunning => "setup_running",
            Ready => "ready",
            Working => "working",
            Idle => "idle",
            Blocked => "blocked",
            Archived => "archived",
            Errored => "errored",
        }
    }

    /// States where the workspace is being built or torn down by Circles itself.
    pub fn is_provisioning(self) -> bool {
        matches!(self, Creating | SetupRunning)
    }

    /// States where an agent is active, so the workspace must not be archived or changed.
    pub fn is_busy(self) -> bool {
        matches!(self, Working | Blocked)
    }

    /// States that have a worktree on disk.
    pub fn has_worktree(self) -> bool {
        !matches!(self, Archived | Errored)
    }
}

impl FromStr for WorkspaceState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ALL_STATES
            .into_iter()
            .find(|state| state.as_str() == s)
            .ok_or_else(|| format!("unknown workspace state \"{s}\""))
    }
}

/// States `from` may move to. Staying in the same state is always allowed and is a no-op.
pub fn allowed_targets(from: WorkspaceState) -> &'static [WorkspaceState] {
    match from {
        Creating => &[SetupRunning, Ready, Errored],
        SetupRunning => &[Ready, Errored],
        Ready => &[Working, Idle, Blocked, Archived, Errored],
        Working => &[Idle, Blocked, Errored],
        Idle => &[Working, Blocked, Archived, Errored],
        Blocked => &[Working, Idle, Errored],
        // Restoring recreates the worktree first.
        Archived => &[Creating, Errored],
        // Retry, repair when the folder reappears, or give up and archive.
        Errored => &[Creating, Ready, Idle, Archived],
    }
}

pub fn can_transition(from: WorkspaceState, to: WorkspaceState) -> bool {
    from == to || allowed_targets(from).contains(&to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip() {
        for state in ALL_STATES {
            assert_eq!(state.as_str().parse::<WorkspaceState>().unwrap(), state);
        }
        assert!("nope".parse::<WorkspaceState>().is_err());
    }

    #[test]
    fn the_happy_path_is_allowed() {
        assert!(can_transition(Creating, Ready));
        assert!(can_transition(Creating, SetupRunning));
        assert!(can_transition(SetupRunning, Ready));
        assert!(can_transition(Ready, Working));
        assert!(can_transition(Working, Blocked));
        assert!(can_transition(Blocked, Working));
        assert!(can_transition(Working, Idle));
        assert!(can_transition(Idle, Archived));
        assert!(can_transition(Archived, Creating));
        assert!(can_transition(Creating, Ready));
    }

    #[test]
    fn invalid_moves_are_rejected() {
        // An agent must stop before the workspace can be archived.
        assert!(!can_transition(Working, Archived));
        assert!(!can_transition(Blocked, Archived));
        // Provisioning cannot be skipped or interrupted by an agent.
        assert!(!can_transition(Creating, Working));
        assert!(!can_transition(Creating, Archived));
        assert!(!can_transition(SetupRunning, Idle));
        // Archived workspaces only come back through Creating.
        assert!(!can_transition(Archived, Ready));
        assert!(!can_transition(Archived, Working));
        assert!(!can_transition(Archived, Idle));
        // Nothing returns to Creating except archived and errored workspaces.
        for from in [Ready, Working, Idle, Blocked, Creating, SetupRunning] {
            assert!(
                !can_transition(from, Creating) || from == Creating,
                "{from:?}"
            );
        }
    }

    #[test]
    fn every_state_can_fail() {
        // Archived has no worktree, so only a failed restore (via Creating) can break it.
        for from in ALL_STATES {
            if matches!(
                from,
                Errored | Idle | Ready | Working | Blocked | Creating | SetupRunning | Archived
            ) {
                assert!(can_transition(from, Errored), "{from:?} -> Errored");
            }
        }
    }

    #[test]
    fn same_state_is_a_noop() {
        for state in ALL_STATES {
            assert!(can_transition(state, state));
        }
    }

    #[test]
    fn every_state_has_a_way_out_or_is_terminal_by_design() {
        for from in ALL_STATES {
            assert!(!allowed_targets(from).is_empty(), "{from:?} is a dead end");
            assert!(
                !allowed_targets(from).contains(&from),
                "{from:?} lists itself"
            );
        }
    }

    #[test]
    fn helper_predicates() {
        assert!(Working.is_busy() && Blocked.is_busy() && !Idle.is_busy());
        assert!(
            Creating.is_provisioning()
                && SetupRunning.is_provisioning()
                && !Ready.is_provisioning()
        );
        assert!(Ready.has_worktree() && !Archived.has_worktree() && !Errored.has_worktree());
    }
}
