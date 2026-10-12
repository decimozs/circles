//! Which side questions are running, one per chat. Asking a new one stops the old one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::oneshot;

#[derive(Default)]
pub struct SideService {
    /// Per chat: which question it is (a number) and how to stop it.
    running: Mutex<HashMap<String, (u64, oneshot::Sender<()>)>>,
    next: AtomicU64,
}

impl SideService {
    /// Registers a question for a chat and stops the one that was there. Gives the way to hear
    /// that it is stopped, and the number of this question.
    pub fn begin(&self, session_id: &str) -> (oneshot::Receiver<()>, u64) {
        let (tx, rx) = oneshot::channel();
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let old = self
            .running
            .lock()
            .unwrap()
            .insert(session_id.to_string(), (id, tx));
        if let Some((_, old)) = old {
            let _ = old.send(());
        }
        (rx, id)
    }

    /// Stops the question of a chat, if there is one.
    pub fn stop(&self, session_id: &str) {
        let old = self.running.lock().unwrap().remove(session_id);
        if let Some((_, tx)) = old {
            let _ = tx.send(());
        }
    }

    /// Forgets a question that ended by itself. A newer question of the same chat stays.
    pub fn finished(&self, session_id: &str, id: u64) {
        let mut running = self.running.lock().unwrap();
        if running
            .get(session_id)
            .is_some_and(|(current, _)| *current == id)
        {
            running.remove(session_id);
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.running.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_question_stops_the_old_one_of_the_same_chat_only() {
        let service = SideService::default();
        let (mut first, first_id) = service.begin("a");
        let (mut other, other_id) = service.begin("b");
        let (_second, second_id) = service.begin("a");
        assert!(first.try_recv().is_ok(), "the old one was told to stop");
        assert!(other.try_recv().is_err(), "another chat is not touched");
        // The old question ending late does not forget the new one.
        service.finished("a", first_id);
        assert_eq!(service.count(), 2);
        service.finished("a", second_id);
        service.finished("b", other_id);
        assert_eq!(service.count(), 0);
    }
}
