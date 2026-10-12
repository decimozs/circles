use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::*;
use crate::git::testutil::{repo_with_commit, sh, test_git};
use crate::harness::parser::{MessageKind, NewMessage, Role};
use crate::harness::store::{NewSession, SessionStatus, SessionStore};

struct Rig {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    service: Checkpoints,
    store: SessionStore,
    session: String,
}

async fn rig() -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let git = test_git(&home);
    let dir = fs::canonicalize(tmp.path()).unwrap().join("repo");
    fs::create_dir_all(&dir).unwrap();
    repo_with_commit(&git, &dir).await;
    fs::write(dir.join(".git/info/exclude"), ".context/\n").unwrap();

    let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
    db.write(|c| {
        c.execute(
            "INSERT INTO projects VALUES ('p','n',?1,'main',NULL,1,1)",
            params![dir.to_string_lossy()],
        )?;
        c.execute_batch(
            "INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
               VALUES ('w1','p','Oslo','main','main','/w1','ready',1,1);",
        )
    })
    .unwrap();
    let store = SessionStore::new(db.clone());
    let session = store.create("w1", NewSession::default()).unwrap().id;
    Rig {
        _tmp: tmp,
        dir: dir.clone(),
        service: Checkpoints::new(git, db),
        store,
        session,
    }
}

fn say(role: Role, kind: MessageKind, content: serde_json::Value) -> NewMessage {
    NewMessage {
        role,
        kind,
        content,
    }
}

impl Rig {
    /// Runs a turn the way the app does: snapshot, the user's message, then the agent's changes
    /// and answer.
    async fn turn(&self, ask: &str, answer: &str, write: &[(&str, &str)]) -> String {
        let claude = self.store.get(&self.session).unwrap().claude_session_id;
        let id = self
            .service
            .record_turn("w1", &self.session, claude.as_deref(), &self.dir)
            .await
            .unwrap();
        let (event, _) = self
            .store
            .append_event(&self.session, "sent:user", ask)
            .unwrap();
        let user = self
            .store
            .append_messages(
                &self.session,
                Some(&event),
                &[say(
                    Role::User,
                    MessageKind::Text,
                    json!({ "text": ask, "steered": false }),
                )],
            )
            .unwrap();
        self.service.attach_message(&id, user[0].seq).unwrap();
        for (path, text) in write {
            fs::write(self.dir.join(path), text).unwrap();
        }
        self.store
            .append_event(&self.session, "assistant", answer)
            .unwrap();
        self.store
            .append_messages(
                &self.session,
                None,
                &[say(
                    Role::Assistant,
                    MessageKind::Text,
                    json!({ "text": answer }),
                )],
            )
            .unwrap();
        self.store
            .set_claude_session(&self.session, "claude-1", None)
            .unwrap();
        id
    }

    fn texts(&self) -> Vec<String> {
        self.store
            .messages(&self.session, None)
            .unwrap()
            .iter()
            .map(|m| m.content["text"].as_str().unwrap_or("").to_string())
            .collect()
    }

    fn read(&self, path: &str) -> Option<String> {
        fs::read_to_string(self.dir.join(path)).ok()
    }
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[tokio::test]
async fn every_turn_gets_a_checkpoint_and_the_branch_is_untouched() {
    let r = rig().await;
    let head = git_out(&r.dir, &["rev-parse", "HEAD"]);
    let log = git_out(&r.dir, &["log", "--oneline"]);
    r.turn("first", "did one", &[("one.txt", "1")]).await;
    r.turn("second", "did two", &[("two.txt", "2")]).await;
    r.turn("third", "did three", &[("three.txt", "3")]).await;

    let list = r.service.list(&r.session).unwrap();
    assert_eq!(list.iter().map(|c| c.turn).collect::<Vec<_>>(), [1, 2, 3]);
    assert!(
        list.iter().all(|c| c.message_seq.is_some()),
        "each knows its message"
    );
    assert_eq!(
        list[0].claude_session_id, None,
        "the first turn had no agent session yet"
    );
    assert_eq!(list[1].claude_session_id.as_deref(), Some("claude-1"));
    assert_eq!(git_out(&r.dir, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git_out(&r.dir, &["log", "--oneline"]),
        log,
        "no commit was added to the branch"
    );
    let refs = git_out(
        &r.dir,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/circles/checkpoints/",
        ],
    );
    assert_eq!(refs.lines().count(), 3);
    assert!(
        git_out(&r.dir, &["status", "--porcelain"])
            .lines()
            .all(|l| l.starts_with("??")),
        "only the new files show, nothing staged"
    );
}

#[tokio::test]
async fn steered_messages_do_not_start_a_new_turn_number() {
    let r = rig().await;
    r.turn("first", "one", &[]).await;
    r.store
        .append_messages(
            &r.session,
            None,
            &[say(
                Role::User,
                MessageKind::Text,
                json!({ "text": "actually", "steered": true }),
            )],
        )
        .unwrap();
    r.turn("second", "two", &[]).await;
    assert_eq!(
        r.service
            .list(&r.session)
            .unwrap()
            .iter()
            .map(|c| c.turn)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[tokio::test]
async fn reverting_to_turn_two_restores_the_files_and_the_chat() {
    let r = rig().await;
    r.turn("first", "did one", &[("one.txt", "1")]).await;
    let second = r
        .turn(
            "second",
            "did two",
            &[("two.txt", "2"), ("one.txt", "1 changed")],
        )
        .await;
    r.turn("third", "did three", &[("three.txt", "3")]).await;
    fs::write(r.dir.join("loose.txt"), "typed by hand").unwrap();

    let preview = r.service.preview(&second, &r.dir).await.unwrap();
    assert_eq!(preview.later_turns, 1);
    assert!(!preview.running);
    assert_eq!(preview.other_chats_active, 0);
    let by = |p: &str| preview.files.iter().find(|f| f.path == p).map(|f| f.kind);
    assert_eq!(by("two.txt"), Some(snapshot::ChangeKind::Removed));
    assert_eq!(by("three.txt"), Some(snapshot::ChangeKind::Removed));
    assert_eq!(by("one.txt"), Some(snapshot::ChangeKind::Modified));
    assert_eq!(
        r.read("two.txt").as_deref(),
        Some("2"),
        "the preview changed nothing"
    );

    let outcome = r.service.revert(&second, &r.dir).await.unwrap();
    assert!(outcome.files.iter().any(|f| f.path == "two.txt"));
    assert_eq!(r.read("one.txt").as_deref(), Some("1"));
    assert_eq!(r.read("two.txt"), None);
    assert_eq!(r.read("three.txt"), None);
    assert_eq!(
        r.read("loose.txt"),
        None,
        "files made by hand after the turn go as well: it is the state before the turn"
    );
    assert_eq!(
        r.texts(),
        ["first", "did one"],
        "the chat has only what came before"
    );
    assert_eq!(
        r.store.event_count(&r.session).unwrap(),
        2,
        "so do the raw events"
    );

    // The agent starts over, told only about turn one.
    let session = r.store.get(&r.session).unwrap();
    assert_eq!(session.claude_session_id, None);
    let replay: Option<String> = r
        .service
        .db
        .read(|c| {
            c.query_row(
                "SELECT replay_context FROM sessions WHERE id = ?1",
                params![r.session],
                |x| x.get(0),
            )
        })
        .unwrap();
    let replay = replay.unwrap();
    assert!(replay.contains("User: first") && replay.contains("Assistant: did one"));
    assert!(
        !replay.contains("second") && !replay.contains("three"),
        "{replay}"
    );

    // Only turn one can be reverted to now.
    assert_eq!(
        r.service
            .list(&r.session)
            .unwrap()
            .iter()
            .map(|c| c.turn)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        git_out(&r.dir, &["log", "--oneline"]).lines().count(),
        1,
        "the branch history is as it was"
    );
}

#[tokio::test]
async fn after_a_revert_the_next_turn_takes_the_place_of_the_removed_one() {
    let r = rig().await;
    r.turn("first", "one", &[("one.txt", "1")]).await;
    let second = r.turn("second", "two", &[("two.txt", "2")]).await;
    r.service.revert(&second, &r.dir).await.unwrap();
    r.turn("second again", "two again", &[("two.txt", "different")])
        .await;
    assert_eq!(
        r.service
            .list(&r.session)
            .unwrap()
            .iter()
            .map(|c| c.turn)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(r.texts(), ["first", "one", "second again", "two again"]);
}

#[tokio::test]
async fn a_revert_can_be_undone_and_everything_comes_back() {
    let r = rig().await;
    r.turn("first", "did one", &[("one.txt", "1")]).await;
    let second = r.turn("second", "did two", &[("two.txt", "2")]).await;
    r.turn("third", "did three", &[("three.txt", "3")]).await;
    let before = r.texts();
    let outcome = r.service.revert(&second, &r.dir).await.unwrap();
    assert_eq!(r.texts().len(), 2);

    let undoable = r
        .service
        .last_revert(&r.session)
        .unwrap()
        .expect("it can be undone");
    assert_eq!(undoable.id, outcome.revert_id);
    assert_eq!(undoable.turn, 2);

    let files = r.service.undo(&outcome.revert_id, &r.dir).await.unwrap();
    assert!(files.iter().any(|f| f.path == "three.txt"));
    assert_eq!(r.read("two.txt").as_deref(), Some("2"));
    assert_eq!(r.read("three.txt").as_deref(), Some("3"));
    assert_eq!(r.texts(), before, "turn three is back in the chat");
    assert_eq!(
        r.store
            .get(&r.session)
            .unwrap()
            .claude_session_id
            .as_deref(),
        Some("claude-1"),
        "the agent's own session too"
    );
    assert_eq!(
        r.service
            .list(&r.session)
            .unwrap()
            .iter()
            .map(|c| c.turn)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(r.service.last_revert(&r.session).unwrap().is_none());

    let again = r
        .service
        .undo(&outcome.revert_id, &r.dir)
        .await
        .unwrap_err();
    assert_eq!(again.code(), "invalid");
    // The turns can be reverted to again.
    r.service.revert(&second, &r.dir).await.unwrap();
    assert_eq!(r.texts().len(), 2);
}

#[tokio::test]
async fn an_undo_is_refused_once_the_chat_has_moved_on_or_too_much_time_passed() {
    let r = rig().await;
    r.turn("first", "one", &[("one.txt", "1")]).await;
    let second = r.turn("second", "two", &[("two.txt", "2")]).await;
    let outcome = r.service.revert(&second, &r.dir).await.unwrap();
    r.turn("new direction", "ok", &[]).await;
    assert!(r.service.last_revert(&r.session).unwrap().is_none());
    assert_eq!(
        r.service
            .undo(&outcome.revert_id, &r.dir)
            .await
            .unwrap_err()
            .code(),
        "invalid"
    );

    let r = rig().await;
    r.turn("first", "one", &[]).await;
    let second = r.turn("second", "two", &[("two.txt", "2")]).await;
    let outcome = r.service.revert(&second, &r.dir).await.unwrap();
    r.service
        .db
        .write(|c| {
            c.execute(
                "UPDATE reverts SET created_at = created_at - ?1",
                params![UNDO_WINDOW_MS + 1000],
            )
        })
        .unwrap();
    assert!(r.service.last_revert(&r.session).unwrap().is_none());
    assert_eq!(
        r.service
            .undo(&outcome.revert_id, &r.dir)
            .await
            .unwrap_err()
            .code(),
        "invalid"
    );
    assert_eq!(
        r.service.undo("nope", &r.dir).await.unwrap_err().code(),
        "not_found"
    );
}

#[tokio::test]
async fn a_revert_is_blocked_while_a_turn_runs() {
    let r = rig().await;
    r.turn("first", "one", &[("one.txt", "1")]).await;
    let second = r.turn("second", "two", &[("two.txt", "2")]).await;
    r.store
        .set_status(&r.session, SessionStatus::Working, None)
        .unwrap();
    assert!(r.service.preview(&second, &r.dir).await.unwrap().running);
    let err = r.service.revert(&second, &r.dir).await.unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.to_string().contains("turn is running"));
    assert_eq!(r.read("two.txt").as_deref(), Some("2"));
    assert_eq!(r.texts().len(), 4);

    r.store
        .set_status(&r.session, SessionStatus::Blocked, None)
        .unwrap();
    assert!(r.service.revert(&second, &r.dir).await.is_err());
}

#[tokio::test]
async fn other_busy_chats_of_the_workspace_are_reported() {
    let r = rig().await;
    let second = r.turn("first", "one", &[]).await;
    let other = r.store.create("w1", NewSession::default()).unwrap();
    r.store
        .set_status(&other.id, SessionStatus::Working, None)
        .unwrap();
    assert_eq!(
        r.service
            .preview(&second, &r.dir)
            .await
            .unwrap()
            .other_chats_active,
        1
    );
}

#[tokio::test]
async fn a_failed_revert_leaves_the_files_and_the_chat_as_they_were() {
    let r = rig().await;
    r.turn("first", "one", &[("one.txt", "1")]).await;
    let second = r
        .turn(
            "second",
            "two",
            &[("two.txt", "2"), ("one.txt", "1 changed")],
        )
        .await;
    // A file where the staging folder belongs, so the revert cannot prepare.
    fs::create_dir_all(r.dir.join(".context")).unwrap();
    fs::write(r.dir.join(".context/checkpoint-staging"), "in the way").unwrap();

    let err = r.service.revert(&second, &r.dir).await.unwrap_err();
    assert!(!err.to_string().is_empty());
    assert_eq!(r.read("two.txt").as_deref(), Some("2"));
    assert_eq!(r.read("one.txt").as_deref(), Some("1 changed"));
    assert_eq!(r.texts().len(), 4, "no message was removed");
    assert_eq!(
        r.store
            .get(&r.session)
            .unwrap()
            .claude_session_id
            .as_deref(),
        Some("claude-1")
    );
    assert!(
        r.service.last_revert(&r.session).unwrap().is_none(),
        "no half made revert is left"
    );
    let pre = git_out(
        &r.dir,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/circles/checkpoints/w1/pre-revert/",
        ],
    );
    assert!(pre.trim().is_empty(), "the safety ref is removed: {pre}");
    assert_eq!(r.service.list(&r.session).unwrap().len(), 2);
}

#[tokio::test]
async fn a_turn_that_never_started_cannot_be_reverted_to() {
    let r = rig().await;
    let id = r
        .service
        .record_turn("w1", &r.session, None, &r.dir)
        .await
        .unwrap();
    assert_eq!(
        r.service.revert(&id, &r.dir).await.unwrap_err().code(),
        "invalid"
    );
    assert_eq!(r.service.get("nope").unwrap_err().code(), "not_found");
}

#[tokio::test]
async fn checkpoints_are_removed_with_the_workspace_and_when_old() {
    let r = rig().await;
    r.turn("first", "one", &[("one.txt", "1")]).await;
    let second = r.turn("second", "two", &[]).await;
    r.service.revert(&second, &r.dir).await.unwrap();
    assert!(!git_out(&r.dir, &["for-each-ref", "refs/circles/"])
        .trim()
        .is_empty());

    // Old ones first.
    r.service
        .db
        .write(|c| c.execute("UPDATE checkpoints SET created_at = 1", []))
        .unwrap();
    assert_eq!(
        r.service.prune_older_than(KEEP_DAYS).await.unwrap(),
        2,
        "the turn and the one a revert set aside"
    );
    assert!(r.service.list(&r.session).unwrap().is_empty());

    // Then a whole workspace.
    r.turn("again", "ok", &[]).await;
    r.service.prune_workspace("w1", &r.dir).await.unwrap();
    assert!(
        git_out(&r.dir, &["for-each-ref", "refs/circles/"])
            .trim()
            .is_empty(),
        "no refs are left"
    );
    let rows: i64 = r
        .service
        .db
        .read(|c| {
            c.query_row(
                "SELECT (SELECT COUNT(*) FROM checkpoints) + (SELECT COUNT(*) FROM reverts)",
                [],
                |x| x.get(0),
            )
        })
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(git_out(&r.dir, &["log", "--oneline"]).lines().count(), 1);
}

#[test]
fn the_transcript_tells_what_was_said_and_done_and_trims_the_oldest() {
    let rows = vec![
        (
            "user".to_string(),
            "text".to_string(),
            json!({ "text": "Add a login form" }),
        ),
        (
            "assistant".to_string(),
            "thinking".to_string(),
            json!({ "text": "hmm" }),
        ),
        (
            "assistant".to_string(),
            "tool_call".to_string(),
            json!({ "name": "Edit", "input": { "file_path": "src/login.tsx" } }),
        ),
        (
            "user".to_string(),
            "tool_result".to_string(),
            json!({ "content": "ok, 200 lines of output" }),
        ),
        (
            "assistant".to_string(),
            "tool_call".to_string(),
            json!({ "name": "Task", "input": {} }),
        ),
        (
            "assistant".to_string(),
            "text".to_string(),
            json!({ "text": "Done." }),
        ),
        (
            "system".to_string(),
            "system".to_string(),
            json!({ "text": "Interrupted" }),
        ),
    ];
    let text = transcript(&rows, 10_000);
    assert!(text.contains("User: Add a login form"));
    assert!(text.contains("[Assistant used Edit: src/login.tsx]"));
    assert!(text.contains("[Assistant used Task]"));
    assert!(text.contains("Assistant: Done."));
    assert!(!text.contains("hmm") && !text.contains("200 lines") && !text.contains("Interrupted"));
    assert!(text.ends_with("The user's new message:"));

    let long: Vec<(String, String, serde_json::Value)> = (0..50)
        .map(|i| {
            (
                "user".to_string(),
                "text".to_string(),
                json!({ "text": format!("message number {i:02} {}", "x".repeat(100)) }),
            )
        })
        .collect();
    let trimmed = transcript(&long, 1000);
    assert!(trimmed.contains("message number 49"), "the newest stays");
    assert!(!trimmed.contains("message number 00"), "the oldest goes");
    assert!(trimmed.contains("Earlier messages are left out"));
    assert_eq!(transcript(&[], 1000), "");
    assert_eq!(
        transcript(
            &[(
                "assistant".into(),
                "thinking".into(),
                json!({ "text": "x" })
            )],
            1000
        ),
        ""
    );
}

#[tokio::test]
async fn the_conversation_before_a_message_can_start_another_chat() {
    let r = rig().await;
    r.turn("first", "did one", &[]).await;
    r.turn("second", "did two", &[]).await;
    let list = r.service.list(&r.session).unwrap();
    let text = r
        .service
        .context_before(&r.session, list[1].message_seq.unwrap())
        .unwrap()
        .unwrap();
    assert!(text.contains("User: first") && !text.contains("second"));
    assert!(
        r.service
            .context_before(&r.session, list[0].message_seq.unwrap())
            .unwrap()
            .is_none(),
        "nothing came before the first message"
    );
    let _ = sh;
}

#[test]
fn how_long_checkpoints_are_kept_comes_from_the_settings_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.toml");
    assert_eq!(keep_days_from_settings(&file), KEEP_DAYS, "no file");
    for (text, want) in [
        ("[advanced]\ncheckpoint_days = 7\n", 7),
        ("[advanced]\ncheckpoint_days = 0\n", KEEP_DAYS),
        ("[advanced]\ncheckpoint_days = 9000\n", KEEP_DAYS),
        ("[advanced]\ncheckpoint_days = \"x\"\n", KEEP_DAYS),
        ("this is [ not toml", KEEP_DAYS),
    ] {
        std::fs::write(&file, text).unwrap();
        assert_eq!(keep_days_from_settings(&file), want, "{text}");
    }
}
