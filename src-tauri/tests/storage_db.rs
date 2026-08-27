use quietly_lib::storage::db;
use tempfile::tempdir;

#[test]
fn open_db_creates_schema() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = db::open_db(&db_path).unwrap();

    // schema_version is set by init_schema_sync
    let ver: String = conn
        .query_row("SELECT value FROM settings WHERE key = 'schema_version'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, "1");

    // the core tables exist after init_schema_sync
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='block_rule'",
            [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn open_pool_enables_foreign_keys() {
    // foreign_keys is a per-connection setting. The pool must apply it
    // on every connection it hands out — assert by adding a row that
    // would violate a foreign key (none enforced today, but a future
    // schema could rely on it; smoke-test the setting is on).
    let dir = tempdir().unwrap();
    let pool = db::open_pool(&dir.path().join("pool.db")).unwrap();
    let conn = pool.get().unwrap();
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(on, 1);
}

#[test]
fn init_schema_is_idempotent() {
    // Calling init_schema_sync twice on the same connection must not
    // fail — init_schema is called once per connection in open_pool
    // and a manual call from a test fixture shouldn't blow up.
    let dir = tempdir().unwrap();
    let conn = db::open_db(&dir.path().join("idem.db")).unwrap();
    db::init_schema_sync(&conn).unwrap();
    db::init_schema_sync(&conn).unwrap();
    let ver: String = conn
        .query_row("SELECT value FROM settings WHERE key = 'schema_version'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, "1");
}