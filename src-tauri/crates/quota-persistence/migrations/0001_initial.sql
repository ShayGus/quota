-- Quota durable monitoring state, version 1.
--
-- Every timestamp is RFC 3339 UTC text. No table holds a token, a cookie, or
-- an authorization header, and no column is named after one: credentials live
-- in the operating-system secure store, never here.
--
-- `schema_migrations` is created by the migration runner, not here, so this
-- file cannot collide with the table that records whether it has been applied.
--
-- `measurement_history` intentionally has no foreign key. It is optional,
-- prunable history, and a retention sweep of an account's rows must never
-- cascade into any other account's rows.

CREATE TABLE connections (
    id                   TEXT PRIMARY KEY,
    provider_id          TEXT NOT NULL,
    credential_ownership TEXT NOT NULL,
    generation           INTEGER NOT NULL,
    profile_label        TEXT,
    cardinality          TEXT NOT NULL,
    state                TEXT NOT NULL,
    principal_id         TEXT,
    workspace_id         TEXT,
    entitlement_id       TEXT
);

CREATE TABLE accounts (
    id                       TEXT PRIMARY KEY,
    connection_id            TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
    provider_id              TEXT NOT NULL,
    nickname                 TEXT NOT NULL,
    connection_ordinal       INTEGER NOT NULL,
    monitoring_enabled       INTEGER NOT NULL,
    connection_state         TEXT NOT NULL,
    fetch_state              TEXT NOT NULL,
    last_attempt_at          TEXT,
    last_success_at          TEXT,
    next_attempt_at          TEXT,
    verified_principal_label TEXT,
    verified_workspace_label TEXT,
    verified_plan_label      TEXT,
    identity_source          TEXT,
    UNIQUE (connection_id, connection_ordinal)
);

CREATE INDEX idx_accounts_connection_id ON accounts (connection_id);

CREATE TABLE quota_pools (
    id          TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    shared      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE account_pool_bindings (
    account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    pool_id    TEXT NOT NULL REFERENCES quota_pools (id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, pool_id)
);

CREATE INDEX idx_account_pool_bindings_pool_id ON account_pool_bindings (pool_id);

CREATE TABLE quota_windows (
    id                 TEXT PRIMARY KEY,
    pool_id            TEXT NOT NULL REFERENCES quota_pools (id) ON DELETE CASCADE,
    provider_bucket_id TEXT,
    scope_resource     TEXT NOT NULL,
    scope_label        TEXT NOT NULL,
    category           TEXT NOT NULL,
    semantics          TEXT NOT NULL,
    duration_seconds   INTEGER,
    metric_role        TEXT NOT NULL,
    enforcement        TEXT NOT NULL,
    source_kind        TEXT NOT NULL,
    completeness       TEXT NOT NULL,
    definition_version INTEGER NOT NULL
);

CREATE INDEX idx_quota_windows_pool_id ON quota_windows (pool_id);

CREATE TABLE latest_measurements (
    account_id       TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    window_id        TEXT NOT NULL REFERENCES quota_windows (id) ON DELETE CASCADE,
    measurement_kind TEXT NOT NULL,
    measurement_json TEXT NOT NULL,
    period_started_at TEXT,
    boundary_at      TEXT,
    boundary_kind    TEXT,
    observed_at      TEXT,
    received_at      TEXT NOT NULL,
    valid_until      TEXT,
    issues_json      TEXT NOT NULL,
    PRIMARY KEY (account_id, window_id)
);

CREATE INDEX idx_latest_measurements_window_id ON latest_measurements (window_id);

CREATE TABLE measurement_history (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id        TEXT NOT NULL,
    window_id         TEXT NOT NULL,
    remaining_percent REAL,
    observed_at       TEXT NOT NULL,
    received_at       TEXT NOT NULL
);

CREATE INDEX idx_measurement_history_account ON measurement_history (account_id, observed_at);
CREATE INDEX idx_measurement_history_window ON measurement_history (window_id, observed_at);

CREATE TABLE alert_episodes (
    account_id         TEXT NOT NULL,
    window_id          TEXT NOT NULL,
    definition_version INTEGER NOT NULL,
    level              TEXT NOT NULL,
    opened_at          TEXT NOT NULL,
    armed_at           TEXT,
    closed_at          TEXT,
    PRIMARY KEY (account_id, window_id, definition_version, level)
);

CREATE TABLE notification_outbox (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id  TEXT NOT NULL,
    window_id   TEXT NOT NULL,
    level       TEXT NOT NULL,
    episode_key TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL,
    delivered_at TEXT
);

CREATE TABLE refresh_backoff (
    scope_kind          TEXT NOT NULL,
    scope_id            TEXT NOT NULL,
    attempts            INTEGER NOT NULL,
    next_eligible_at    TEXT NOT NULL,
    provider_retry_after TEXT,
    PRIMARY KEY (scope_kind, scope_id)
);

CREATE INDEX idx_refresh_backoff_next_eligible_at ON refresh_backoff (next_eligible_at);

CREATE TABLE monitoring_preferences (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    monitoring_state TEXT NOT NULL
);
