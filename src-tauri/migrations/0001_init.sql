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

CREATE TABLE IF NOT EXISTS schedule (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    kind           TEXT NOT NULL,
    enabled        INTEGER NOT NULL DEFAULT 1,
    config_json    TEXT NOT NULL,
    block_rule_ids TEXT NOT NULL,
    created_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session (
    id               INTEGER PRIMARY KEY,
    schedule_id      INTEGER,
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
    rule_id     INTEGER,
    schedule_id INTEGER
);

CREATE INDEX IF NOT EXISTS idx_session_started ON session(started_at);
CREATE INDEX IF NOT EXISTS idx_block_hit_ts    ON block_hit(ts);