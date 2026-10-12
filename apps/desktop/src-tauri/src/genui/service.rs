//! Showing a card, waiting for its answer, and giving the answer back. One per app.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::oneshot;

use super::store::{CardStore, StoredCard};
use super::{check_answer, parse_card};
use crate::error::{AppError, AppResult};

/// How many cards may wait in one chat at once.
const MAX_PENDING: usize = 3;
/// The least time between two cards of one chat.
const MIN_GAP: Duration = Duration::from_millis(1000);

/// What the app is told when cards change.
pub trait CardEvents: Send + Sync {
    /// A card was shown, answered, dismissed or expired.
    fn cards_changed(&self, session_id: &str, composer_id: Option<&str>);
    /// A card began to wait for a person, for a notification when nobody is looking.
    fn card_requested(&self, _composer_id: Option<&str>) {}
}

/// What comes back to the agent.
enum Reply {
    Answered(Value),
    Dismissed,
}

pub struct GenuiService {
    pub store: CardStore,
    events: Arc<dyn CardEvents>,
    waiters: Mutex<HashMap<String, oneshot::Sender<Reply>>>,
    last: Mutex<HashMap<String, Instant>>,
    /// Whether Composers may ask. On unless switched off.
    composers: AtomicBool,
    /// Whether ordinary chats may ask. Off unless switched on.
    chats: AtomicBool,
}

/// Marks a card expired if the call that waits for it goes away first.
struct ExpireOnDrop<'a> {
    service: &'a GenuiService,
    card: &'a StoredCard,
    done: bool,
}

impl Drop for ExpireOnDrop<'_> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        self.service.waiters.lock().unwrap().remove(&self.card.uid);
        if self
            .service
            .store
            .finish(&self.card.uid, "expired", None)
            .unwrap_or(false)
        {
            self.service
                .events
                .cards_changed(&self.card.session_id, self.card.composer_id.as_deref());
        }
    }
}

impl GenuiService {
    pub fn new(store: CardStore, events: Arc<dyn CardEvents>) -> Arc<Self> {
        Arc::new(Self {
            store,
            events,
            waiters: Mutex::new(HashMap::new()),
            last: Mutex::new(HashMap::new()),
            composers: AtomicBool::new(true),
            chats: AtomicBool::new(false),
        })
    }

    pub fn set_composers(&self, on: bool) {
        self.composers.store(on, Ordering::SeqCst);
    }

    pub fn set_chats(&self, on: bool) {
        self.chats.store(on, Ordering::SeqCst);
    }

    pub fn composers_on(&self) -> bool {
        self.composers.load(Ordering::SeqCst)
    }

    pub fn chats_on(&self) -> bool {
        self.chats.load(Ordering::SeqCst)
    }

    /// Shows a card. Returns what the agent is told: the answer when it waited, or `shown`.
    /// The error is written for the agent to read.
    pub async fn show(
        &self,
        session_id: &str,
        composer_id: Option<&str>,
        input: &Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let card = parse_card(&input["card"])?;
        let wait = input["wait"].as_bool().unwrap_or(true) && card.has_input();

        // Not too many, and not too fast: a model in a loop must not bury the chat.
        {
            let mut last = self.last.lock().unwrap();
            if let Some(at) = last.get(session_id) {
                if at.elapsed() < MIN_GAP {
                    return Err("Cards are coming too fast. Wait a moment, and send fewer.".into());
                }
            }
            last.insert(session_id.to_string(), Instant::now());
        }
        let pending = self
            .store
            .pending_in(session_id)
            .map_err(|e| e.to_string())?;
        if pending >= MAX_PENDING {
            return Err(format!(
                "{pending} cards are still waiting for the person. Wait for them first."
            ));
        }

        let stored = self
            .store
            .create(
                session_id,
                composer_id,
                &card,
                if wait { "pending" } else { "shown" },
            )
            .map_err(|e| e.to_string())?;
        self.events.cards_changed(session_id, composer_id);
        if !wait {
            return Ok(json!({ "id": card.id, "status": "shown" }));
        }

        let (tx, rx) = oneshot::channel();
        self.waiters.lock().unwrap().insert(stored.uid.clone(), tx);
        self.events.card_requested(composer_id);
        let mut guard = ExpireOnDrop {
            service: self,
            card: &stored,
            done: false,
        };
        let outcome = tokio::time::timeout(timeout, rx).await;
        guard.done = true;
        self.waiters.lock().unwrap().remove(&stored.uid);
        match outcome {
            Ok(Ok(Reply::Answered(values))) => {
                Ok(json!({ "id": card.id, "status": "answered", "values": values }))
            }
            Ok(Ok(Reply::Dismissed)) => Ok(json!({ "id": card.id, "status": "dismissed" })),
            // The waiter was dropped: the card was closed some other way.
            Ok(Err(_)) => Ok(json!({ "id": card.id, "status": "expired" })),
            Err(_) => {
                if self
                    .store
                    .finish(&stored.uid, "expired", None)
                    .unwrap_or(false)
                {
                    self.events.cards_changed(session_id, composer_id);
                }
                Ok(json!({
                    "id": card.id,
                    "status": "expired",
                    "note": "The person did not answer in time."
                }))
            }
        }
    }

    /// A person's answer. Checked against the card, and counted once.
    pub fn answer(&self, uid: &str, values: &Value) -> AppResult<()> {
        let stored = self.store.get(uid)?;
        if stored.status != "pending" {
            return Err(AppError::Invalid("That card was already answered.".into()));
        }
        let checked = check_answer(&stored.card, values).map_err(AppError::Invalid)?;
        if !self.store.finish(uid, "answered", Some(&checked))? {
            return Err(AppError::Invalid("That card was already answered.".into()));
        }
        if let Some(tx) = self.waiters.lock().unwrap().remove(uid) {
            let _ = tx.send(Reply::Answered(checked));
        }
        self.events
            .cards_changed(&stored.session_id, stored.composer_id.as_deref());
        Ok(())
    }

    /// The person closes a card without answering.
    pub fn dismiss(&self, uid: &str) -> AppResult<()> {
        let stored = self.store.get(uid)?;
        if !self.store.finish(uid, "dismissed", None)? {
            return Err(AppError::Invalid("That card was already answered.".into()));
        }
        if let Some(tx) = self.waiters.lock().unwrap().remove(uid) {
            let _ = tx.send(Reply::Dismissed);
        }
        self.events
            .cards_changed(&stored.session_id, stored.composer_id.as_deref());
        Ok(())
    }

    /// Everything that waited is over after a restart.
    pub fn expire_after_restart(&self) {
        match self.store.expire_pending() {
            Ok(0) => {}
            Ok(n) => tracing::info!(cards = n, "cards expired by the restart"),
            Err(e) => tracing::warn!(error = %e, "cannot expire old cards"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct Events {
        changed: AtomicUsize,
        requested: AtomicUsize,
    }
    impl CardEvents for Events {
        fn cards_changed(&self, _s: &str, _c: Option<&str>) {
            self.changed.fetch_add(1, Ordering::SeqCst);
        }
        fn card_requested(&self, _c: Option<&str>) {
            self.requested.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn service() -> (tempfile::TempDir, Arc<GenuiService>, Arc<Events>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let events = Arc::new(Events::default());
        (
            dir,
            GenuiService::new(CardStore::new(db), events.clone()),
            events,
        )
    }

    fn pick_card(wait: Option<bool>) -> Value {
        let mut input = json!({ "card": {
            "id": "pick", "title": "Which?",
            "body": [{"type": "choice_list", "id": "t", "multiple": true,
                "options": [{"value": "a", "label": "A"}, {"value": "b", "label": "B"}]}]
        }});
        if let Some(w) = wait {
            input["wait"] = json!(w);
        }
        input
    }

    async fn first_pending(service: &GenuiService) -> StoredCard {
        for _ in 0..200 {
            if let Some(c) = service.store.pending().unwrap().into_iter().next() {
                return c;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no card is waiting");
    }

    #[tokio::test]
    async fn a_card_waits_for_the_answer_and_hands_it_back() {
        let (_d, service, events) = service();
        let s = service.clone();
        let call = tokio::spawn(async move {
            s.show(
                "chat1",
                Some("c1"),
                &pick_card(None),
                Duration::from_secs(30),
            )
            .await
        });
        let card = first_pending(&service).await;
        assert_eq!(events.requested.load(Ordering::SeqCst), 1);
        service.answer(&card.uid, &json!({"t": ["b"]})).unwrap();
        let reply = call.await.unwrap().unwrap();
        assert_eq!(
            reply,
            json!({"id": "pick", "status": "answered", "values": {"t": ["b"]}})
        );
        assert_eq!(service.store.get(&card.uid).unwrap().status, "answered");
    }

    #[tokio::test]
    async fn a_second_answer_is_refused() {
        let (_d, service, _e) = service();
        let s = service.clone();
        let call = tokio::spawn(async move {
            s.show("chat1", None, &pick_card(None), Duration::from_secs(30))
                .await
        });
        let card = first_pending(&service).await;
        service.answer(&card.uid, &json!({"t": ["a"]})).unwrap();
        assert!(service.answer(&card.uid, &json!({"t": ["b"]})).is_err());
        assert!(service.dismiss(&card.uid).is_err());
        call.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_bad_answer_leaves_the_card_waiting() {
        let (_d, service, _e) = service();
        let s = service.clone();
        let call = tokio::spawn(async move {
            s.show("chat1", None, &pick_card(None), Duration::from_secs(30))
                .await
        });
        let card = first_pending(&service).await;
        assert!(service.answer(&card.uid, &json!({"t": ["nope"]})).is_err());
        assert_eq!(service.store.get(&card.uid).unwrap().status, "pending");
        service.dismiss(&card.uid).unwrap();
        let reply = call.await.unwrap().unwrap();
        assert_eq!(reply["status"], "dismissed");
    }

    #[tokio::test]
    async fn a_card_that_is_not_answered_in_time_expires() {
        let (_d, service, _e) = service();
        let reply = service
            .show("chat1", None, &pick_card(None), Duration::from_millis(50))
            .await
            .unwrap();
        assert_eq!(reply["status"], "expired");
        assert!(service.store.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_call_that_goes_away_expires_its_card() {
        let (_d, service, _e) = service();
        let s = service.clone();
        let call = tokio::spawn(async move {
            s.show("chat1", None, &pick_card(None), Duration::from_secs(30))
                .await
        });
        let card = first_pending(&service).await;
        call.abort();
        let _ = call.await;
        for _ in 0..100 {
            if service.store.get(&card.uid).unwrap().status == "expired" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the card did not expire");
    }

    #[tokio::test]
    async fn a_card_with_no_input_or_with_wait_off_does_not_wait() {
        let (_d, service, _e) = service();
        let text = json!({"card": {"id": "t", "body": [{"type": "text", "text": "hi"}]}});
        let reply = service
            .show("chat1", None, &text, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(reply["status"], "shown");
        tokio::time::sleep(MIN_GAP).await;
        let reply = service
            .show(
                "chat1",
                None,
                &pick_card(Some(false)),
                Duration::from_secs(30),
            )
            .await
            .unwrap();
        assert_eq!(reply["status"], "shown");
        assert!(service.store.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cards_cannot_come_too_fast_or_pile_up() {
        let (_d, service, _e) = service();
        service
            .show(
                "chat1",
                None,
                &pick_card(Some(false)),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        let err = service
            .show(
                "chat1",
                None,
                &pick_card(Some(false)),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(err.contains("too fast"));
        // Another chat is not held back.
        service
            .show(
                "chat2",
                None,
                &pick_card(Some(false)),
                Duration::from_secs(1),
            )
            .await
            .unwrap();

        // Three waiting cards fill a chat.
        for _ in 0..MAX_PENDING {
            service
                .store
                .create(
                    "chat3",
                    None,
                    &parse_card(&pick_card(None)["card"]).unwrap(),
                    "pending",
                )
                .unwrap();
        }
        let err = service
            .show("chat3", None, &pick_card(None), Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(err.contains("still waiting"));
    }

    #[tokio::test]
    async fn a_hostile_card_is_an_error_for_the_agent_and_stores_nothing() {
        let (_d, service, _e) = service();
        let err = service
            .show(
                "chat1",
                None,
                &json!({"card": {"id": "x", "body": [{"type": "html", "html": "<img onerror=x>"}]}}),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(err.contains("components"));
        assert!(service.store.list("chat1", 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn what_waited_before_a_restart_is_expired() {
        let (_d, service, _e) = service();
        let card = parse_card(&pick_card(None)["card"]).unwrap();
        let stored = service
            .store
            .create("chat1", None, &card, "pending")
            .unwrap();
        service.expire_after_restart();
        assert_eq!(service.store.get(&stored.uid).unwrap().status, "expired");
    }
}
