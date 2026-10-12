-- Todos came with the first schema. This tells apart the ones a person wrote from the ones an
-- agent suggested.
ALTER TABLE todos ADD COLUMN source TEXT NOT NULL DEFAULT 'user';
