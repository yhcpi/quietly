use rusqlite::params;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::db::DbPool;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_seconds: i64,
    pub phase: String,
    pub completed: bool,
}

fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn start(pool: &DbPool, phase: &str, duration: i64) -> Result<i64> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO session (started_at, duration_seconds, phase, completed)
         VALUES (?1,?2,?3,0)",
        params![now(), duration, phase])?;
    Ok(conn.last_insert_rowid())
}

pub fn end(pool: &DbPool, id: i64, completed: bool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE session SET ended_at=?1, completed=?2 WHERE id=?3",
        params![now(), completed as i64, id])?;
    Ok(())
}

pub fn list_recent(pool: &DbPool, since_ts: i64) -> Result<Vec<Session>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, started_at, ended_at, duration_seconds, phase, completed
         FROM session WHERE started_at >= ?1 ORDER BY started_at DESC")?;
    let rows = stmt.query_map(params![since_ts], |row| {
        Ok(Session {
            id: row.get(0)?,
            started_at: row.get(1)?,
            ended_at: row.get(2)?,
            duration_seconds: row.get(3)?,
            phase: row.get(4)?,
            completed: row.get::<_, i64>(5)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}
