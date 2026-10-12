-- Cards an agent put in a chat, and what the person answered. A card that waits for an answer
-- is `pending`. The others are `answered`, `dismissed`, `expired` (the app restarted or the call
-- went away) and `shown` (a card that asks for nothing).
CREATE TABLE ui_cards (
    uid         TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    composer_id TEXT,
    card        TEXT NOT NULL,
    status      TEXT NOT NULL,
    answer      TEXT,
    created_at  INTEGER NOT NULL,
    answered_at INTEGER
);
CREATE INDEX ui_cards_session ON ui_cards (session_id, created_at);
CREATE INDEX ui_cards_pending ON ui_cards (status) WHERE status = 'pending';
