-- Answers from GitHub that are shown at once the next time and refreshed in the background.
-- Anything here can be deleted: it is fetched again.
CREATE TABLE cache (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    last_used INTEGER NOT NULL,
    bytes INTEGER NOT NULL
);

CREATE INDEX cache_last_used ON cache (last_used);
