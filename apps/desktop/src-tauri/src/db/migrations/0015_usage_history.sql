-- What Claude Code's transcripts say about usage, kept so the Usage page opens at once and keeps
-- history after Claude Code deletes old transcripts. Only numbers, ids and the model are kept.
CREATE TABLE usage_files (
    path TEXT PRIMARY KEY,
    -- Size and modification time when it was last read, to see whether it changed.
    len INTEGER NOT NULL,
    mtime_ms INTEGER NOT NULL,
    -- How far it was read: the end of the last complete line.
    offset INTEGER NOT NULL,
    skipped INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE usage_messages (
    -- The API message id. A message written to a transcript several times is one row.
    id TEXT PRIMARY KEY,
    folder TEXT NOT NULL,
    file TEXT NOT NULL,
    model TEXT NOT NULL,
    at INTEGER NOT NULL,
    day TEXT NOT NULL,
    session TEXT NOT NULL,
    input INTEGER NOT NULL,
    output INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write_5m INTEGER NOT NULL,
    cache_write_1h INTEGER NOT NULL
);

CREATE INDEX usage_messages_at ON usage_messages (at);
CREATE INDEX usage_messages_file ON usage_messages (file);
