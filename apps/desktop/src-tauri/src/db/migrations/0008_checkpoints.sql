-- Checkpoints record the state before each turn. A checkpoint is `turn` for a normal one,
-- `pre_revert` for the state kept just before a revert so it can be undone, and `reverted` for a
-- turn that a revert removed (its turn number is made negative while it waits for an undo).
ALTER TABLE checkpoints ADD COLUMN kind TEXT NOT NULL DEFAULT 'turn';
ALTER TABLE checkpoints ADD COLUMN event_seq INTEGER;
-- Large untracked files left out of the snapshot, as a JSON list of paths.
ALTER TABLE checkpoints ADD COLUMN skipped TEXT;

-- What a chat is told about its earlier conversation when its Claude session was started over.
ALTER TABLE sessions ADD COLUMN replay_context TEXT;

CREATE TABLE reverts (
    id                TEXT PRIMARY KEY,
    workspace_id      TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id        TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn              INTEGER NOT NULL,
    message_seq       INTEGER NOT NULL,
    pre_revert_ref    TEXT NOT NULL,
    claude_session_id TEXT,
    replay_context    TEXT,
    created_at        INTEGER NOT NULL,
    undone            INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX reverts_session ON reverts (session_id, created_at);

-- The messages and events a revert removed, kept for the undo.
CREATE TABLE reverted_messages (
    revert_id  TEXT NOT NULL REFERENCES reverts(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    session_id TEXT NOT NULL,
    event_id   TEXT,
    seq        INTEGER NOT NULL,
    role       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE reverted_events (
    revert_id  TEXT NOT NULL REFERENCES reverts(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    session_id TEXT NOT NULL,
    seq        INTEGER NOT NULL,
    kind       TEXT NOT NULL,
    raw        TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
