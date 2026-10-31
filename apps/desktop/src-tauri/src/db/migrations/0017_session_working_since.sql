-- When the turn that is running now began, so a screen can count how long it has gone on. Set when
-- a chat starts working, kept while it waits for a permission, cleared when it stops.
ALTER TABLE sessions ADD COLUMN working_since INTEGER;
