-- User defined groups of workspaces in the sidebar.
CREATE TABLE sections (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    position   INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

ALTER TABLE workspaces ADD COLUMN section_id TEXT REFERENCES sections(id) ON DELETE SET NULL;

-- The lines shown at the top of a workspace chat: "branched from origin/main", warnings, hints.
CREATE TABLE workspace_events (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    -- created | info | warning | hint | error
    kind         TEXT NOT NULL,
    message      TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_workspace_events_workspace ON workspace_events(workspace_id);
