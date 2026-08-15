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