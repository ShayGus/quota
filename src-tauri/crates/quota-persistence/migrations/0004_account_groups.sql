-- Account groups: several accounts the person treats as one provider account,
-- such as the API keys of one OpenRouter account.
--
-- A group holds a name and its provider, never a key. An account names at most
-- one group. A group with no account left is deleted by the repository after
-- every change, so a group exists only while it has members. Deleting a group
-- row by any other path leaves its accounts ungrouped, not deleted.
CREATE TABLE account_groups (
    id          TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    name        TEXT NOT NULL CHECK (length(trim(name)) > 0)
);

ALTER TABLE accounts
    ADD COLUMN group_id TEXT REFERENCES account_groups (id) ON DELETE SET NULL;
