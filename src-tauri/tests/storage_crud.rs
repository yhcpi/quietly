use quietly_lib::storage::{block_rule, session, block_hit};
use quietly_lib::storage::db;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

fn pool() -> Pool<SqliteConnectionManager> {
    // Bind TempDir to a variable so the directory lives for the test's duration;
    // otherwise tempfile drops it immediately and the DB path becomes invalid.
    let dir = tempfile::tempdir().unwrap();
    let manager = SqliteConnectionManager::file(dir.path().join("t.db"));
    let pool = Pool::builder().max_size(2).build(manager).unwrap();
    db::init_schema_sync(&pool.get().unwrap()).unwrap();
    pool
}

#[test]
fn block_rule_crud() {
    let p = pool();
    let id = block_rule::create(&p, &block_rule::BlockRule {
        id: None, kind: "url".into(), pattern: "twitter.com".into(),
        match_type: "contains".into(), note: "".into(),
        active: true, created_at: 0,
    }).unwrap();
    let list = block_rule::list(&p).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, Some(id));
    assert_eq!(list[0].pattern, "twitter.com");

    block_rule::set_active(&p, id, false).unwrap();
    let active = block_rule::list_active(&p).unwrap();
    assert_eq!(active.len(), 0);
}

#[test]
fn session_lifecycle() {
    let p = pool();
    let id = session::start(&p, "focus", 1500).unwrap();
    session::end(&p, id, true).unwrap();
    let s = &session::list_recent(&p, 0).unwrap()[0];
    assert_eq!(s.completed, true);
    assert_eq!(s.phase, "focus");
    assert_eq!(s.duration_seconds, 1500);
}

#[test]
fn block_hit_log_and_count() {
    let p = pool();
    block_hit::log(&p, "url", "https://x.com", Some(1)).unwrap();
    block_hit::log(&p, "url", "https://y.com", Some(2)).unwrap();
    let recent = block_hit::list_recent(&p, 0, 10).unwrap();
    assert_eq!(recent.len(), 2);
    // count_by_day uses strftime('%Y-%m-%d', ts, 'unixepoch', 'localtime')
    let counts = block_hit::count_by_day(&p, 0).unwrap();
    let total: i64 = counts.iter().map(|(_, n)| n).sum();
    assert_eq!(total, 2);
}