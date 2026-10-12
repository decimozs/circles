-- Composers: manager agents that use Circles' own tools.
--
-- A Composer owns a hidden scratch workspace, in a hidden project (the one with the id
-- `composers`), so every chat feature works on it. The runs of a Composer are chats in that
-- workspace.

CREATE TABLE composers (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    instructions TEXT NOT NULL DEFAULT '',
    workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(id) ON DELETE CASCADE,
    -- {"tool": "allow" | "ask" | "off"}. A tool that is not there gets the default for its tier.
    autonomy     TEXT NOT NULL DEFAULT '{}',
    -- JSON list of project ids the Composer may touch, or NULL for all of them.
    projects     TEXT,
    limits       TEXT NOT NULL DEFAULT '{}',
    allow_override INTEGER NOT NULL DEFAULT 0,
    notes        TEXT NOT NULL DEFAULT '',
    model        TEXT,
    effort       TEXT,
    pinned       INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

-- Who started a run. Chats in a Composer's workspace without a row here were started by a person.
CREATE TABLE composer_runs (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    composer_id TEXT NOT NULL REFERENCES composers(id) ON DELETE CASCADE,
    -- person | schedule | trigger
    started_by TEXT NOT NULL,
    trigger_id TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX composer_runs_composer ON composer_runs (composer_id, created_at);

-- Every tool call, allowed or not.
CREATE TABLE tool_audit (
    id          TEXT PRIMARY KEY,
    at          INTEGER NOT NULL,
    composer_id TEXT,
    session_id  TEXT,
    tool        TEXT NOT NULL,
    tier        TEXT,
    -- Inputs with secrets removed and long text cut.
    input       TEXT NOT NULL DEFAULT '',
    -- ok | error | denied | expired
    outcome     TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    approval_id TEXT
);
CREATE INDEX tool_audit_session ON tool_audit (session_id, at);
CREATE INDEX tool_audit_composer ON tool_audit (composer_id, at);

CREATE TABLE approvals (
    id          TEXT PRIMARY KEY,
    composer_id TEXT NOT NULL REFERENCES composers(id) ON DELETE CASCADE,
    session_id  TEXT NOT NULL,
    tool        TEXT NOT NULL,
    tier        TEXT NOT NULL,
    input       TEXT NOT NULL DEFAULT '',
    -- pending | allowed | denied | expired
    status      TEXT NOT NULL DEFAULT 'pending',
    created_at  INTEGER NOT NULL,
    answered_at INTEGER
);
CREATE INDEX approvals_status ON approvals (status, created_at);
CREATE INDEX approvals_session ON approvals (session_id, created_at);

-- What a run started.
CREATE TABLE composer_children (
    id           TEXT PRIMARY KEY,
    composer_id  TEXT NOT NULL REFERENCES composers(id) ON DELETE CASCADE,
    session_id   TEXT NOT NULL,
    workspace_id TEXT,
    chat_id      TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX composer_children_run ON composer_children (session_id);
CREATE INDEX composer_children_workspace ON composer_children (workspace_id);

CREATE TABLE composer_triggers (
    id           TEXT PRIMARY KEY,
    composer_id  TEXT NOT NULL REFERENCES composers(id) ON DELETE CASCADE,
    -- schedule | pull_request | check_failed
    kind         TEXT NOT NULL,
    -- JSON: {"every_minutes": 60} or {"daily_at": "09:00"} for a schedule, {} otherwise.
    config       TEXT NOT NULL DEFAULT '{}',
    project_id   TEXT REFERENCES projects(id) ON DELETE CASCADE,
    -- The text that starts the run. The event is added below it.
    prompt       TEXT NOT NULL DEFAULT '',
    enabled      INTEGER NOT NULL DEFAULT 1,
    failures     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    last_fired_at INTEGER
);
CREATE INDEX composer_triggers_composer ON composer_triggers (composer_id);

-- An event a trigger already fired for.
CREATE TABLE trigger_seen (
    trigger_id TEXT NOT NULL REFERENCES composer_triggers(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    seen_at    INTEGER NOT NULL,
    PRIMARY KEY (trigger_id, key)
);
