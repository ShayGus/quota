-- A prepaid balance, measured from its last top-up, and whether an account's
-- API key spend limit is shown on its card.
--
-- Both belong to one account and leave with it: the ledger through its foreign
-- key, the switch as a column of the account row. Existing accounts keep their
-- key limit hidden, which is the default for a new account too.
ALTER TABLE accounts
    ADD COLUMN show_key_limit INTEGER NOT NULL DEFAULT 0 CHECK (show_key_limit IN (0, 1));

-- One row per account with a prepaid balance. `ledger_json` is the typed
-- `BalanceLedger`: the amounts at the last reading, the baseline the balance is
-- measured from, the top-ups seen and the spending samples of the last week.
-- It holds amounts and instants only, never a key or an account name.
CREATE TABLE account_balances (
    account_id  TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    ledger_json TEXT NOT NULL
);
