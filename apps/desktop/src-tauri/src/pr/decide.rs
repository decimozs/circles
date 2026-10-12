//! What stands between a branch and a merge, and what to do next. Pure functions of the facts, so
//! every case can be tested without git or GitHub.

use serde::Serialize;
use ts_rs::TS;

use crate::github::pr::{Check, CheckBucket, Mergeable, PrState, PrStatus};

/// Everything the decision needs to know.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub dirty: bool,
    /// Commits the branch has that its base lacks.
    pub ahead: u32,
    pub conflicts_with_base: bool,
    pub pr: Option<PrStatus>,
    pub checks: Vec<Check>,
    pub unresolved_threads: u32,
    pub open_todos: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BlockerKind {
    Todos,
    ChecksFailing,
    ChecksPending,
    Conflicts,
    ChangesRequested,
    Draft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Blocker {
    pub kind: BlockerKind,
    pub message: String,
    /// The user may merge anyway. A draft cannot be merged at all.
    pub overridable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NextAction {
    CreatePr,
    ResolveConflicts,
    FixChecks,
    RespondToReview,
    /// Checks are still running, so there is nothing to do yet.
    WaitForChecks,
    Merge,
    /// The pull request was merged. The workspace can be archived.
    Archive,
    None,
}

fn plural(count: u32, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub fn failing(checks: &[Check]) -> u32 {
    checks
        .iter()
        .filter(|c| c.bucket == CheckBucket::Fail)
        .count() as u32
}

pub fn pending(checks: &[Check]) -> u32 {
    checks
        .iter()
        .filter(|c| c.bucket == CheckBucket::Pending)
        .count() as u32
}

/// The reasons not to merge, worst first. Empty means the branch is ready.
pub fn blockers(facts: &Facts) -> Vec<Blocker> {
    let mut out = Vec::new();
    let Some(pr) = facts.pr.as_ref().filter(|p| p.state == PrState::Open) else {
        return out;
    };
    if pr.is_draft {
        out.push(Blocker {
            kind: BlockerKind::Draft,
            message: "The pull request is still a draft.".into(),
            overridable: false,
        });
    }
    if pr.mergeable == Mergeable::Conflicting || facts.conflicts_with_base {
        out.push(Blocker {
            kind: BlockerKind::Conflicts,
            message: "The branch conflicts with its base branch.".into(),
            overridable: true,
        });
    }
    let failed = failing(&facts.checks);
    if failed > 0 {
        out.push(Blocker {
            kind: BlockerKind::ChecksFailing,
            message: format!("{} failing.", plural(failed, "check is", "checks are")),
            overridable: true,
        });
    }
    let waiting = pending(&facts.checks);
    if waiting > 0 {
        out.push(Blocker {
            kind: BlockerKind::ChecksPending,
            message: format!(
                "{} still running.",
                plural(waiting, "check is", "checks are")
            ),
            overridable: true,
        });
    }
    if pr.review_decision.as_deref() == Some("changes_requested") || facts.unresolved_threads > 0 {
        let message = if facts.unresolved_threads > 0 {
            format!(
                "{} waiting for an answer.",
                plural(
                    facts.unresolved_threads,
                    "review thread is",
                    "review threads are"
                )
            )
        } else {
            "A reviewer asked for changes.".into()
        };
        out.push(Blocker {
            kind: BlockerKind::ChangesRequested,
            message,
            overridable: true,
        });
    }
    if facts.open_todos > 0 {
        out.push(Blocker {
            kind: BlockerKind::Todos,
            message: format!(
                "{} still open.",
                plural(facts.open_todos, "todo is", "todos are")
            ),
            overridable: true,
        });
    }
    out
}

/// The one thing most worth doing next.
pub fn next_action(facts: &Facts) -> NextAction {
    let Some(pr) = &facts.pr else {
        // Uncommitted work counts too: creating the pull request starts by committing it.
        return if facts.ahead > 0 || facts.dirty {
            NextAction::CreatePr
        } else {
            NextAction::None
        };
    };
    match pr.state {
        PrState::Merged => return NextAction::Archive,
        PrState::Closed => return NextAction::None,
        PrState::Open => {}
    }
    if pr.mergeable == Mergeable::Conflicting || facts.conflicts_with_base {
        return NextAction::ResolveConflicts;
    }
    if failing(&facts.checks) > 0 {
        return NextAction::FixChecks;
    }
    if facts.unresolved_threads > 0 || pr.review_decision.as_deref() == Some("changes_requested") {
        return NextAction::RespondToReview;
    }
    if pr.is_draft {
        return NextAction::None;
    }
    if pending(&facts.checks) > 0 {
        return NextAction::WaitForChecks;
    }
    NextAction::Merge
}

/// Whether nothing at all holds the merge back.
pub fn ready_to_merge(facts: &Facts) -> bool {
    facts.pr.as_ref().is_some_and(|p| p.state == PrState::Open) && blockers(facts).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(extra: impl FnOnce(&mut PrStatus)) -> PrStatus {
        let mut status = PrStatus {
            number: 7,
            url: "https://github.com/o/r/pull/7".into(),
            title: "t".into(),
            state: PrState::Open,
            is_draft: false,
            mergeable: Mergeable::Clean,
            merge_state: "clean".into(),
            review_decision: None,
            head_ref: "feature".into(),
            base_ref: "main".into(),
        };
        extra(&mut status);
        status
    }

    fn check(bucket: CheckBucket) -> Check {
        Check {
            name: "ci".into(),
            workflow: String::new(),
            bucket,
            state: String::new(),
            description: String::new(),
            link: String::new(),
            run_id: None,
        }
    }

    fn facts(pr_status: Option<PrStatus>) -> Facts {
        Facts {
            ahead: 2,
            pr: pr_status,
            ..Facts::default()
        }
    }

    #[test]
    fn without_a_pull_request_the_next_step_is_to_create_one_when_there_is_work() {
        assert_eq!(next_action(&facts(None)), NextAction::CreatePr);
        assert_eq!(next_action(&Facts::default()), NextAction::None);
        // Only uncommitted work: the pull request flow commits it first.
        assert_eq!(
            next_action(&Facts {
                ahead: 0,
                dirty: true,
                ..Facts::default()
            }),
            NextAction::CreatePr
        );
        assert!(blockers(&facts(None)).is_empty());
        assert!(!ready_to_merge(&facts(None)));
    }

    #[test]
    fn a_clean_open_pull_request_is_ready_to_merge() {
        let mut f = facts(Some(pr(|_| {})));
        f.checks = vec![check(CheckBucket::Pass), check(CheckBucket::Skipping)];
        assert_eq!(next_action(&f), NextAction::Merge);
        assert!(blockers(&f).is_empty());
        assert!(ready_to_merge(&f));
    }

    #[test]
    fn conflicts_come_first() {
        let mut f = facts(Some(pr(|p| p.mergeable = Mergeable::Conflicting)));
        f.checks = vec![check(CheckBucket::Fail)];
        f.unresolved_threads = 2;
        assert_eq!(next_action(&f), NextAction::ResolveConflicts);
        assert_eq!(blockers(&f)[0].kind, BlockerKind::Conflicts);

        let local = Facts {
            conflicts_with_base: true,
            ..facts(Some(pr(|_| {})))
        };
        assert_eq!(next_action(&local), NextAction::ResolveConflicts);
    }

    #[test]
    fn failing_checks_come_before_review_threads() {
        let mut f = facts(Some(pr(|_| {})));
        f.checks = vec![
            check(CheckBucket::Fail),
            check(CheckBucket::Fail),
            check(CheckBucket::Pass),
        ];
        f.unresolved_threads = 1;
        assert_eq!(next_action(&f), NextAction::FixChecks);
        let kinds: Vec<_> = blockers(&f).iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [BlockerKind::ChecksFailing, BlockerKind::ChangesRequested]
        );
        assert_eq!(blockers(&f)[0].message, "2 checks are failing.");
    }

    #[test]
    fn open_threads_or_requested_changes_call_for_a_response() {
        let mut f = facts(Some(pr(|_| {})));
        f.unresolved_threads = 1;
        assert_eq!(next_action(&f), NextAction::RespondToReview);
        assert_eq!(
            blockers(&f)[0].message,
            "1 review thread is waiting for an answer."
        );

        let asked = facts(Some(pr(|p| {
            p.review_decision = Some("changes_requested".into())
        })));
        assert_eq!(next_action(&asked), NextAction::RespondToReview);
        assert_eq!(blockers(&asked)[0].message, "A reviewer asked for changes.");
    }

    #[test]
    fn running_checks_wait_and_hold_the_merge_back() {
        let mut f = facts(Some(pr(|_| {})));
        f.checks = vec![check(CheckBucket::Pending), check(CheckBucket::Pass)];
        assert_eq!(next_action(&f), NextAction::WaitForChecks);
        assert_eq!(blockers(&f)[0].kind, BlockerKind::ChecksPending);
        assert!(!ready_to_merge(&f));
    }

    #[test]
    fn open_todos_block_the_merge_but_the_merge_is_still_the_next_step() {
        let mut f = facts(Some(pr(|_| {})));
        f.open_todos = 3;
        assert_eq!(next_action(&f), NextAction::Merge);
        let b = blockers(&f);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, BlockerKind::Todos);
        assert_eq!(b[0].message, "3 todos are still open.");
        assert!(b[0].overridable);
        assert!(!ready_to_merge(&f));
        f.open_todos = 1;
        assert_eq!(blockers(&f)[0].message, "1 todo is still open.");
    }

    #[test]
    fn a_draft_cannot_be_merged_even_with_an_override() {
        let f = facts(Some(pr(|p| p.is_draft = true)));
        assert_eq!(next_action(&f), NextAction::None);
        let b = blockers(&f);
        assert_eq!(b[0].kind, BlockerKind::Draft);
        assert!(!b[0].overridable);
    }

    #[test]
    fn merged_pull_requests_offer_archiving_and_closed_ones_nothing() {
        assert_eq!(
            next_action(&facts(Some(pr(|p| p.state = PrState::Merged)))),
            NextAction::Archive
        );
        assert_eq!(
            next_action(&facts(Some(pr(|p| p.state = PrState::Closed)))),
            NextAction::None
        );
        let merged = Facts {
            open_todos: 5,
            ..facts(Some(pr(|p| p.state = PrState::Merged)))
        };
        assert!(blockers(&merged).is_empty());
    }
}
