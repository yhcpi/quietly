use quietly_lib::storage::app_quota;
use quietly_lib::storage::db;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

fn pool() -> Pool<SqliteConnectionManager> {
    let dir = tempfile::tempdir().unwrap();
    let manager = SqliteConnectionManager::file(dir.path().join("t.db"));
    let pool = Pool::builder().max_size(2).build(manager).unwrap();
    db::init_schema_sync(&pool.get().unwrap()).unwrap();
    pool
}

#[test]
fn app_quota_crud() {
    let p = pool();

    let id = app_quota::create(&p, &app_quota::AppQuota {
        id: None,
        app_pattern: "Discord.exe".into(),
        daily_minutes: 30,
        used_seconds: 0,
        used_day: "2026-08-16".into(),
        active: true,
        created_at: 0,
    }).unwrap();

    let list = app_quota::list(&p).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, Some(id));
    // app_quota::create lowercases the pattern so the DB matches
    // tracker output exactly (Windows filenames are case-insensitive
    // but our quota store is case-sensitive).
    assert_eq!(list[0].app_pattern, "discord.exe");
    assert_eq!(list[0].daily_minutes, 30);

    // toggle off
    app_quota::set_active(&p, id, false).unwrap();
    assert_eq!(app_quota::list_active(&p).unwrap().len(), 0);
    // toggle back on
    app_quota::set_active(&p, id, true).unwrap();
    assert_eq!(app_quota::list_active(&p).unwrap().len(), 1);

    // update
    let mut row = app_quota::list(&p).unwrap().remove(0);
    row.daily_minutes = 60;
    app_quota::update(&p, &row).unwrap();
    let after = app_quota::list(&p).unwrap().remove(0);
    assert_eq!(after.daily_minutes, 60);

    // delete
    app_quota::delete(&p, id).unwrap();
    assert_eq!(app_quota::list(&p).unwrap().len(), 0);
}

#[test]
fn app_quota_increment_same_day() {
    let p = pool();
    let _id = app_quota::create(&p, &app_quota::AppQuota {
        id: None,
        app_pattern: "chrome.exe".into(),
        daily_minutes: 10,            // threshold = 600s
        used_seconds: 0,
        used_day: "2026-08-16".into(),
        active: true,
        created_at: 0,
    }).unwrap();

    // 100s -> not exceeded
    let (used, exceeded) = app_quota::increment_used(&p, "chrome.exe", 100, "2026-08-16").unwrap();
    assert_eq!(used, 100);
    assert_eq!(exceeded, false);

    // +500s = 600s -> crosses threshold
    let (used, exceeded) = app_quota::increment_used(&p, "chrome.exe", 500, "2026-08-16").unwrap();
    assert_eq!(used, 600);
    assert_eq!(exceeded, true);

    // +1s above threshold -> already over, crossed=false
    let (used, exceeded) = app_quota::increment_used(&p, "chrome.exe", 1, "2026-08-16").unwrap();
    assert_eq!(used, 601);
    assert_eq!(exceeded, false);

    // row reflects state
    let row = app_quota::list(&p).unwrap().remove(0);
    assert_eq!(row.used_seconds, 601);
    assert_eq!(row.used_day, "2026-08-16");
}

#[test]
fn app_quota_increment_daily_reset() {
    let p = pool();
    let _id = app_quota::create(&p, &app_quota::AppQuota {
        id: None,
        app_pattern: "msedge.exe".into(),
        daily_minutes: 5,
        used_seconds: 400,
        used_day: "2026-08-15".into(),  // yesterday
        active: true,
        created_at: 0,
    }).unwrap();

    // first switch on the new day: reset used_seconds to 0, then add 50
    let (used, exceeded) = app_quota::increment_used(&p, "msedge.exe", 50, "2026-08-16").unwrap();
    assert_eq!(used, 50);
    assert_eq!(exceeded, false);

    let row = app_quota::list(&p).unwrap().remove(0);
    assert_eq!(row.used_seconds, 50);
    assert_eq!(row.used_day, "2026-08-16");
}

#[test]
fn app_quota_increment_no_match_is_noop() {
    let p = pool();
    let (used, exceeded) = app_quota::increment_used(&p, "nope.exe", 100, "2026-08-16").unwrap();
    assert_eq!(used, 0);
    assert_eq!(exceeded, false);
    // nothing inserted/updated
    assert_eq!(app_quota::list(&p).unwrap().len(), 0);
}

#[test]
fn app_quota_inactive_does_not_accumulate() {
    let p = pool();
    let _id = app_quota::create(&p, &app_quota::AppQuota {
        id: None,
        app_pattern: "Slack.exe".into(),
        daily_minutes: 60,
        used_seconds: 0,
        used_day: "2026-08-16".into(),
        active: false,
        created_at: 0,
    }).unwrap();

    // Pattern stored lowercase by `create`; pass the same case to
    // increment_used so the WHERE clause matches. This asserts that
    // the `active=1` filter excludes inactive quotas, not just that
    // the pattern is missing.
    let (used, exceeded) = app_quota::increment_used(&p, "slack.exe", 999, "2026-08-16").unwrap();
    assert_eq!(used, 0);
    assert_eq!(exceeded, false);

    let row = app_quota::list(&p).unwrap().remove(0);
    assert_eq!(row.used_seconds, 0);
}