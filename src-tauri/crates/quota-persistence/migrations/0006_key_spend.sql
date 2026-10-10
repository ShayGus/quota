-- What an account's API key spent in the current periods, as its last
-- reading reported it. It is the key's own reading, kept apart from the
-- account's prepaid balance, so a key that may not read the balance keeps it
-- too. NULL when the provider reports none.
ALTER TABLE accounts ADD COLUMN key_spend_json TEXT;
