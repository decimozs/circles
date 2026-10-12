-- Comments on lines of a workspace's files. They are written by the user while reviewing, or come
-- from an agent's review, and are sent to the chat as context.
CREATE TABLE review_comments (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    source TEXT NOT NULL DEFAULT 'user',
    sent_at INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX review_comments_workspace ON review_comments (workspace_id, path);
