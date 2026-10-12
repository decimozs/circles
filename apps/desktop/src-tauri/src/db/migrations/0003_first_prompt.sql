-- What the user typed in the Create dialog. The chat sends it when the workspace is first opened
-- and then clears it.
ALTER TABLE workspaces ADD COLUMN pending_prompt TEXT;
ALTER TABLE workspaces ADD COLUMN pending_model  TEXT;
ALTER TABLE workspaces ADD COLUMN pending_effort TEXT;
