-- Terminal tabs of a workspace. Every workspace has one `setup` and one `run` tab, plus any
-- number of free `terminal` tabs. The shell itself is not stored: it is started on demand and
-- the last part of its output is kept on disk so a tab can be restored after a restart.
CREATE TABLE terminals (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    -- setup | run | terminal
    kind         TEXT NOT NULL,
    title        TEXT NOT NULL,
    position     INTEGER NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_terminals_workspace ON terminals(workspace_id);
CREATE UNIQUE INDEX idx_terminals_setup ON terminals(workspace_id) WHERE kind = 'setup';
CREATE UNIQUE INDEX idx_terminals_run   ON terminals(workspace_id) WHERE kind = 'run';
