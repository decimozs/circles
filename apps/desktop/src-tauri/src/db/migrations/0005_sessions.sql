-- What a session needs beyond the first schema: cost and turn counts for display, the Claude
-- session to fork from on first start, and the reason it last failed.
ALTER TABLE sessions ADD COLUMN total_cost_usd REAL NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN turns INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN fork_of TEXT;
ALTER TABLE sessions ADD COLUMN last_error TEXT;
