-- Who named a chat. `auto` is a title made from the first message or by a model, and may be
-- replaced. `user` is a name a person typed, and is never replaced. Chats that exist already keep
-- the name they have as a person's.
ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'user';
