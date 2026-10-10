-- What a group shows. A group can leave out the line of what its keys spent
-- together, and each key can be left out of its group's card and the widget.
-- A key left out still counts in the account's total. Both are on by default.
ALTER TABLE account_groups
    ADD COLUMN spend_shown INTEGER NOT NULL DEFAULT 1 CHECK (spend_shown IN (0, 1));

ALTER TABLE accounts
    ADD COLUMN group_key_shown INTEGER NOT NULL DEFAULT 1 CHECK (group_key_shown IN (0, 1));
