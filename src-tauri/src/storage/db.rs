use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;

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