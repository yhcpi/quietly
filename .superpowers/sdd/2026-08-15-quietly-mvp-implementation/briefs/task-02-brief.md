# Task 2 Brief — SQLite 数据库初始化 + 迁移

## Position

Task 2 of 19. Builds on Task 1's scaffold (`c0d1b97`). Adds the
storage layer's entry point: a connection pool + a single migration
SQL + the `schema_version` setting row.

Do NOT pre-implement Task 3 (the rest of the storage CRUD). Only the
db plumbing this task owns.

## Working directory

`D:/ProgramData/Focus/`

## Toolchain (already verified)

- Rust 1.97.1, MSVC toolchain default
- MSVC + Windows SDK 22621 (verified by Task 1 cargo check)
- pnpm 11.21.0
- Wrapper: `D:\ProgramData\Focus\scripts\cargo-msvc.bat`

## Critical quirks

- The wrapper `cargo-msvc.bat` strips Git's MSYS `link.exe` from PATH
  then loads MSVC env vars then runs cargo. Always invoke cargo via
  this wrapper for Rust work.
- NEVER run `pnpm create tauri-app ... --force` on a non-empty
  directory — it wipes everything before writing. See Task 1's report.

## Files to create/modify

- Create `src-tauri/src/storage/mod.rs`
- Create `src-tauri/src/storage/db.rs`
- Create `src-tauri/migrations/0001_init.sql`
- Modify `src-tauri/src/lib.rs` (call `open_pool` in `setup`)
- Modify `src-tauri/Cargo.toml` (add deps: `rusqlite`, `r2d2`, `r2d2_sqlite`, `once_cell`, `anyhow`, `tempfile` for dev-deps, `tauri-plugin-path` already implicit through tauri features — confirm in your read)
- Create `src-tauri/tests/storage_db.rs` (failing test)

## Interfaces produced

```rust
pub type DbPool = Pool<SqliteConnectionManager>;
pub fn open_pool<P: AsRef<Path>>(path: &P) -> Result<DbPool, anyhow::Error>;
pub fn open_db(path: &Path) -> Result<Connection, anyhow::Error>;  // for tests
pub fn init_schema_sync(conn: &Connection) -> Result<(), anyhow::Error>;
```

These signatures match what Task 3+ reference. Don't deviate.

## Steps

### Step 1: failing test

`src-tauri/tests/storage_db.rs`:

```rust
use quietly_lib::storage::db;
use tempfile::tempdir;

#[test]
fn open_db_creates_schema() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = db::open_db(&db_path).unwrap();

    // schema_version 应被设置
    let ver: String = conn
        .query_row("SELECT value FROM settings WHERE key = 'schema_version'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, "1");

    // 关键表应存在
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='block_rule'",
            [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
```

### Step 2: verify test fails

```bash
cmd /c "D:\ProgramData\Focus\scripts\cargo-msvc.bat" test --manifest-path src-tauri/Cargo.toml --test storage_db -- open_db_creates_schema
```

Expected: compile error or test panic (function `open_db` doesn't exist yet).

### Step 3: Cargo.toml additions

Add under `[dependencies]`:

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"
once_cell = "1.19"
anyhow = "1.0"
```

Under `[dev-dependencies]`:

```toml
tempfile = "3.10"
```

Note: the `quietly_lib` crate (created by Task 1's scaffold) needs
`pub mod storage;` declared in its `lib.rs`. Tauri 2 normally exposes
the lib as a binary entry; check that `src-tauri/Cargo.toml` has
`[lib] name = "quietly_lib"`.

### Step 4: implement `src-tauri/src/storage/mod.rs`

```rust
pub mod db;
pub mod settings;
pub mod block_rule;
pub mod schedule;
pub mod session;
pub mod block_hit;
```

The other modules are placeholders for Task 3. Create stub files
(`src-tauri/src/storage/{settings,block_rule,schedule,session,block_hit}.rs`)
each containing just `// stub for task 3`. They'll be filled in by
Task 3.

### Step 5: implement `src-tauri/src/storage/db.rs`

```rust
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;
use once_cell::sync::Lazy;

const INIT_SQL: &str = include_str!("../../migrations/0001_init.sql");

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn open_pool<P: AsRef<Path>>(path: &P) -> Result<DbPool, anyhow::Error> {
    let manager = SqliteConnectionManager::file(path);
    let pool = Pool::builder().max_size(8).build(manager)?;
    {
        let conn = pool.get()?;
        init_schema(&conn)?;
    }
    Ok(pool)
}

pub fn open_db(path: &Path) -> Result<Connection, anyhow::Error> {
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn init_schema_sync(conn: &Connection) -> Result<(), anyhow::Error> {
    init_schema(conn)
}

fn init_schema(conn: &Connection) -> Result<(), anyhow::Error> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(INIT_SQL)?;

    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM settings WHERE key='schema_version'", [], |r| r.get(0))?;
    if exists == 0 {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('schema_version', '1')", [])?;
    }
    Ok(())
}
```

### Step 6: implement `src-tauri/migrations/0001_init.sql`

```sql
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
```

### Step 7: wire into `src-tauri/src/lib.rs`

Look at current `src-tauri/src/lib.rs`. In `setup`, add:

```rust
let app_data = app.path().app_data_dir().expect("no app data dir");
std::fs::create_dir_all(&app_data).ok();
let pool = storage::db::open_pool(&app_data.join("quietly.db"))
    .expect("open db");
app.manage(pool);
```

Add the module declaration `pub mod storage;` at the top of the lib.

### Step 8: verify test passes

```bash
cmd /c "D:\ProgramData\Focus\scripts\cargo-msvc.bat" test --manifest-path src-tauri/Cargo.toml --test storage_db -- open_db_creates_schema
```

Expected: PASS, 1 test.

### Step 9: full cargo test sweep

```bash
cmd /c "D:\ProgramData\Focus\scripts\cargo-msvc.bat" test --manifest-path src-tauri/Cargo.toml
```

Expected: 1 PASS (only `open_db_creates_schema` exists). No regressions
in the scaffold's existing tests (there are none yet).

### Step 10: tauri build smoke (verify lib still compiles with Tauri)

```bash
cmd /c "D:\ProgramData\Focus\scripts\cargo-msvc.bat" check --manifest-path src-tauri/Cargo.toml
```

Expected: exit 0. Confirm `quietly_lib::storage::db` is reachable from
`main.rs` indirectly through the lib.

### Step 11: commit

```bash
cd D:/ProgramData/Focus
git add -A
git status
git commit -m "feat(storage): sqlite schema + migration"
```

## Acceptance

- `open_db_creates_schema` test PASSES
- `cargo check` exits 0 (no warnings from new code unless you justify)
- The DB file `quietly.db` actually appears at
  `%APPDATA%\com.quietly.app\quietly.db` if you call `open_pool`
  there once (test it manually if quick, skip if not — main concern is
  the test passing)
- One commit on the existing branch

## What NOT to do

- Don't implement settings/block_rule/schedule/session/block_hit
  modules beyond the stubs. Task 3 owns those.
- Don't change the existing scaffold (don't re-write
  `App.tsx`/`main.rs`/`tauri.conf.json` unless it's necessary to make
  your test compile — usually it isn't).
- Don't introduce new top-level dependencies beyond what's listed.

## Report

Write to `D:/ProgramData/Focus/.superpowers/sdd/2026-08-15-quietly-mvp-implementation/reports/task-02-report.md`:

- Status: DONE | DONE_WITH_CONCERNS | NEEDS_CONTEXT | BLOCKED
- Summary
- Commands run + outcomes
- Commit hash(es)
- Acceptance criteria results (✓/✗)
- Deviations + justifications
- Concerns
