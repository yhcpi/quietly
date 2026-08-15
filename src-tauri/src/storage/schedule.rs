use rusqlite::params;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::db::DbPool;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Schedule {
    pub id: Option<i64>,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub config_json: String,
    pub block_rule_ids: String,
    pub created_at: i64,
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Schedule> {
    Ok(Schedule {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        kind: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        config_json: row.get(4)?,
        block_rule_ids: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn create(pool: &DbPool, s: &Schedule) -> Result<i64> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO schedule (name, kind, enabled, config_json, block_rule_ids, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            s.name, s.kind, s.enabled as i64,
            s.config_json, s.block_rule_ids,
            if s.created_at == 0 { now() } else { s.created_at },
        ])?;
    Ok(conn.last_insert_rowid())
}

pub fn update(pool: &DbPool, s: &Schedule) -> Result<()> {
    let id = s.id.ok_or_else(|| anyhow::anyhow!("update requires id"))?;
    let conn = pool.get()?;
    conn.execute(
        "UPDATE schedule SET name=?1, kind=?2, enabled=?3, config_json=?4, block_rule_ids=?5 WHERE id=?6",
        params![
            s.name, s.kind, s.enabled as i64,
            s.config_json, s.block_rule_ids, id,
        ])?;
    Ok(())
}

pub fn delete(pool: &DbPool, id: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM schedule WHERE id=?1", params![id])?;
    Ok(())
}

pub fn list(pool: &DbPool) -> Result<Vec<Schedule>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, enabled, config_json, block_rule_ids, created_at
         FROM schedule ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn list_enabled(pool: &DbPool) -> Result<Vec<Schedule>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, enabled, config_json, block_rule_ids, created_at
         FROM schedule WHERE enabled=1 ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn toggle(pool: &DbPool, id: i64, enabled: bool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("UPDATE schedule SET enabled=?1 WHERE id=?2",
        params![enabled as i64, id])?;
    Ok(())
}
