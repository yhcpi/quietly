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
fn block_rule_update_rewrites_fields() {
    // update() is what the edit-rule form hits; without it the only
    // way to "edit" a rule is delete-and-recreate, which orphans the
    // id and any block_hits.rule_id rows that pointed at it.
    let p = pool();
    let mut rule = block_rule::BlockRule {
        id: None,
        kind: "url".into(),
        pattern: "twitter.com".into(),
        match_type: "contains".into(),
        note: "".into(),
        active: true,
        created_at: 0,
    };
    let id = block_rule::create(&p, &rule).unwrap();
    rule.id = Some(id);
    rule.pattern = "x.com".into();
    rule.note = "updated".into();
    rule.match_type = "exact".into();
    block_rule::update(&p, &rule).unwrap();
    let got = block_rule::list(&p).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].pattern, "x.com");
    assert_eq!(got[0].note, "updated");
    assert_eq!(got[0].match_type, "exact");
}

#[test]
fn block_rule_list_active_filters_inactive() {
    // list_active is the hot path on every proxy request — must not
    // surface inactive rows or the engine silently lets through rules
    // the user disabled.
    let p = pool();
    let id1 = block_rule::create(&p, &block_rule::BlockRule {
        id: None, kind: "url".into(), pattern: "a.com".into(),
        match_type: "contains".into(), note: "".into(),
        active: true, created_at: 0,
    }).unwrap();
    let _id2 = block_rule::create(&p, &block_rule::BlockRule {
        id: None, kind: "url".into(), pattern: "b.com".into(),
        match_type: "contains".into(), note: "".into(),
        active: false, created_at: 0,
    }).unwrap();
    let active = block_rule::list_active(&p).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, Some(id1));
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
fn session_end_with_completed_false() {
    // The non-completed branch is what cancel_focus_inner uses to
    // distinguish a user-cancelled session from a finished one. The
    // session row must still be marked ended (the bug fixed in the
    // open-source prep audit was that end() was never called at all
    // on cancel, leaving completed=NULL forever).
    let p = pool();
    let id = session::start(&p, "focus", 1500).unwrap();
    session::end(&p, id, false).unwrap();
    let s = &session::list_recent(&p, 0).unwrap()[0];
    assert_eq!(s.completed, false);
    assert_eq!(s.id, id);
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

#[test]
fn block_hit_count_by_day_empty_returns_no_rows() {
    // count_by_day is called from the Stats page on every render —
    // a fresh install (no hits yet) must produce an empty list, not
    // a list with a single (today, 0) entry that the chart then has
    // to special-case.
    let p = pool();
    let counts = block_hit::count_by_day(&p, 0).unwrap();
    assert!(counts.is_empty(), "fresh db should have no day rows, got {:?}", counts);
}