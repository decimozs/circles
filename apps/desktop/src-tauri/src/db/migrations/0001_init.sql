-- Timestamps are UTC unix milliseconds. Ids are ULIDs.

CREATE TABLE projects (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    path           TEXT NOT NULL UNIQUE,
    default_branch TEXT NOT NULL,
    remote_url     TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE TABLE workspaces (
    id             TEXT PRIMARY KEY,
    project_id     TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    city           TEXT NOT NULL,
    title          TEXT,
    branch         TEXT NOT NULL,
    base_branch    TEXT NOT NULL,
    path           TEXT NOT NULL UNIQUE,
    -- creating | setup_running | ready | working | idle | blocked | archived | errored
    state          TEXT NOT NULL,
    error          TEXT,
    pinned         INTEGER NOT NULL DEFAULT 0,
    pr_number      INTEGER,
    pr_url         TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    last_opened_at INTEGER,
    archived_at    INTEGER,
    UNIQUE (project_id, city)
);
CREATE INDEX idx_workspaces_project ON workspaces(project_id);
CREATE INDEX idx_workspaces_state ON workspaces(state);

CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    workspace_id      TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    claude_session_id TEXT,
    title             TEXT,
    -- idle | working | blocked | interrupted | closed
    status            TEXT NOT NULL DEFAULT 'idle',
    model             TEXT,
    effort            TEXT,
    permission_mode   TEXT,
    position          INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);
CREATE INDEX idx_sessions_workspace ON sessions(workspace_id);

-- Raw harness events. This is the source of truth for chat history.
CREATE TABLE events (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,
    kind       TEXT NOT NULL,
    raw        TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (session_id, seq)
);

-- Normalised projection of events used to render the chat.
CREATE TABLE messages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    event_id   TEXT REFERENCES events(id) ON DELETE SET NULL,
    seq        INTEGER NOT NULL,
    -- user | assistant | system
    role       TEXT NOT NULL,
    -- text | thinking | tool_call | tool_result | system
    kind       TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (session_id, seq)
);

CREATE TABLE todos (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    text         TEXT NOT NULL,
    done         INTEGER NOT NULL DEFAULT 0,
    position     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    completed_at INTEGER
);
CREATE INDEX idx_todos_workspace ON todos(workspace_id);

CREATE TABLE checkpoints (
    id                TEXT PRIMARY KEY,
    workspace_id      TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id        TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    turn              INTEGER NOT NULL,
    ref_name          TEXT NOT NULL,
    claude_session_id TEXT,
    message_seq       INTEGER,
    created_at        INTEGER NOT NULL,
    UNIQUE (workspace_id, turn, session_id)
);

-- Each workspace owns a block of 10 consecutive ports starting at base_port.
CREATE TABLE ports (
    workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    base_port    INTEGER NOT NULL UNIQUE,
    allocated_at INTEGER NOT NULL
);

CREATE TABLE kv (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Child processes we started, so a crash never leaves orphans behind.
CREATE TABLE processes (
    pid        INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    owner_id   TEXT,
    command    TEXT NOT NULL,
    started_at INTEGER NOT NULL
);
