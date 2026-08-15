use rusqlite::params;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::db::DbPool;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockHit {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub target: String,
    pub rule_id: Option<i64>,
    pub schedule_id: Option<i64>,
}

fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn log(pool: &DbPool, kind: &str, target: &str, rule_id: Option<i64>, schedule_id: Option<i64>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO block_hit (ts, kind, target, rule_id, schedule_id) VALUES (?1,?2,?3,?4,?5)",
        params![now(), kind, target, rule_id, schedule_id])?;
    Ok(())
}

pub fn list_recent(pool: &DbPool, since_ts: i64, limit: i64) -> Result<Vec<BlockHit>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, ts, kind, target, rule_id, schedule_id
         FROM block_hit WHERE ts >= ?1 ORDER BY ts DESC LIMIT ?2")?;
    let rows = stmt.query_map(params![since_ts, limit], |row| {
        Ok(BlockHit {
            id: row.get(0)?,
            ts: row.get(1)?,
            kind: row.get(2)?,
            target: row.get(3)?,
            rule_id: row.get(4)?,
            schedule_id: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn count_by_day(pool: &DbPool, since_ts: i64) -> Result<Vec<(String, i64)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%d', ts, 'unixepoch', 'localtime') AS day, COUNT(*)
         FROM block_hit WHERE ts >= ?1 GROUP BY day ORDER BY day")?;
    let rows = stmt.query_map(params![since_ts], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}
