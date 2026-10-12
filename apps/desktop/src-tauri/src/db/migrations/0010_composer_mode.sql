-- How much a Composer does on its own, as one of the chat's modes: manual, accept_edits, plan or
-- bypass. A tool with its own setting in `autonomy` keeps it. The mode covers the rest.
ALTER TABLE composers ADD COLUMN mode TEXT NOT NULL DEFAULT 'manual';
