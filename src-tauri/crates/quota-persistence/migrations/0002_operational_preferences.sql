-- Store notification, operational privacy, and polling settings beside monitoring state.
-- The defaults preserve the current documented behaviour for existing databases.
ALTER TABLE monitoring_preferences
    ADD COLUMN preferences_revision INTEGER NOT NULL DEFAULT 0 CHECK (preferences_revision >= 0);

ALTER TABLE monitoring_preferences
    ADD COLUMN notification_policy_json TEXT NOT NULL DEFAULT '{"enabled":true,"thresholds":{"low_percent":20.0,"critical_percent":10.0,"hysteresis_percent":3.0},"quiet_hours":{"kind":"never"},"recovery_enabled":true}';

ALTER TABLE monitoring_preferences
    ADD COLUMN operational_privacy_json TEXT NOT NULL DEFAULT '{"retain_history":true,"export_identities":false}';

ALTER TABLE monitoring_preferences
    ADD COLUMN polling_policies_json TEXT NOT NULL DEFAULT '[]';
