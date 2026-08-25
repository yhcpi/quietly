use quietly_lib::storage::usage_log;
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
fn usage_log_append_and_list_recent() {
    let p = pool();

    let id1 = usage_log::append(&p, &usage_log::UsageSegment {
        id: None,
        kind: "app".into(),
        target: "chrome.exe".into(),
        ts: 1_700_000_000,
        duration_seconds: 60,
    }).unwrap();

    let id2 = usage_log::append(&p, &usage_log::UsageSegment {
        id: None,
        kind: "url".into(),
        target: "example.com".into(),
        ts: 1_700_000_500,
        duration_seconds: 1,
    }).unwrap();

    // most-recent-first, bounded by limit
    let recent = usage_log::list_recent(&p, 0, 10).unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].id, Some(id2));
    assert_eq!(recent[1].id, Some(id1));

    // since_ts filter
    let filtered = usage_log::list_recent(&p, 1_700_000_300, 10).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].target, "example.com");
}

#[test]
fn usage_log_aggregate_by_target() {
    let p = pool();

    // chrome 90s across three segments
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: 1_700_000_000, duration_seconds: 30,
    }).unwrap();
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: 1_700_000_100, duration_seconds: 30,
    }).unwrap();
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: 1_700_000_200, duration_seconds: 30,
    }).unwrap();

    // slack 10s
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "Slack.exe".into(),
        ts: 1_700_000_000, duration_seconds: 10,
    }).unwrap();

    // url entry, must NOT be summed when filtering by kind="app"
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "example.com".into(),
        ts: 1_700_000_000, duration_seconds: 1_000,
    }).unwrap();

    let by_target = usage_log::aggregate_by_target(&p, "app", 0).unwrap();
    assert_eq!(by_target.len(), 2);
    assert_eq!(by_target[0].target, "chrome.exe");
    assert_eq!(by_target[0].total_seconds, 90);
    assert_eq!(by_target[1].target, "Slack.exe");
    assert_eq!(by_target[1].total_seconds, 10);

    let by_url = usage_log::aggregate_by_target(&p, "url", 0).unwrap();
    assert_eq!(by_url.len(), 1);
    assert_eq!(by_url[0].target, "example.com");
    assert_eq!(by_url[0].total_seconds, 1_000);

    // since_ts filter — exclude everything before 1_700_000_300 (so chrome 30+30 are kept)
    let filtered = usage_log::aggregate_by_target(&p, "app", 1_700_000_150).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].target, "chrome.exe");
    assert_eq!(filtered[0].total_seconds, 30);
}

#[test]
fn usage_log_aggregate_by_day() {
    let p = pool();

    // 2026-08-15 (unix epoch localtime depends on env, so use clearly distinct days
    // and verify a single day bucket accumulates). We rely on strftime localtime
    // which on most CI runs matches UTC for this kind of check, but we keep
    // timestamps tightly grouped to one day.
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: 1_700_000_000, duration_seconds: 30,
    }).unwrap();
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: 1_700_086_400, duration_seconds: 30,
    }).unwrap();

    let days = usage_log::aggregate_by_day(&p, "app", 0).unwrap();
    // either 1 or 2 day rows depending on tz; ensure sum matches total
    let total: i64 = days.iter().map(|d| d.total_seconds).sum();
    assert_eq!(total, 60);
}

#[test]
fn usage_log_clear_before() {
    let p = pool();
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "a.com".into(),
        ts: 100, duration_seconds: 5,
    }).unwrap();
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "b.com".into(),
        ts: 200, duration_seconds: 5,
    }).unwrap();
    usage_log::append(&p, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "c.com".into(),
        ts: 300, duration_seconds: 5,
    }).unwrap();

    let deleted = usage_log::clear_before(&p, 250).unwrap();
    assert_eq!(deleted, 2);

    let remaining = usage_log::list_recent(&p, 0, 10).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].target, "c.com");
}