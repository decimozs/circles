-- Outside services a Composer may use, as a JSON list such as ["jira"]. None until a person turns one on.
ALTER TABLE composers ADD COLUMN integrations TEXT NOT NULL DEFAULT '[]';
