CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS block_rule (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,
    pattern      TEXT NOT NULL,
    match_type   TEXT NOT NULL,
    note         TEXT NOT NULL DEFAULT '',
    active       INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session (
    id               INTEGER PRIMARY KEY,
    started_at       INTEGER NOT NULL,
    ended_at         INTEGER,
    duration_seconds INTEGER NOT NULL,
    phase            TEXT NOT NULL,
    completed        INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS block_hit (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    target      TEXT NOT NULL,
    rule_id     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_session_started ON session(started_at);
CREATE INDEX IF NOT EXISTS idx_block_hit_ts    ON block_hit(ts);

CREATE TABLE IF NOT EXISTS app_quota (
    id              INTEGER PRIMARY KEY,
    app_pattern     TEXT NOT NULL,
    daily_minutes   INTEGER NOT NULL,
    used_seconds    INTEGER NOT NULL DEFAULT 0,
    used_day        TEXT NOT NULL,
    active          INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_app_quota_pattern ON app_quota(app_pattern);

CREATE TABLE IF NOT EXISTS usage_log (
    id                INTEGER PRIMARY KEY,
    kind              TEXT NOT NULL,
    target            TEXT NOT NULL,
    ts                INTEGER NOT NULL,
    duration_seconds  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_log_kind_ts ON usage_log(kind, ts);
CREATE INDEX IF NOT EXISTS idx_usage_log_target  ON usage_log(target);