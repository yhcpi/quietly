use rusqlite::params;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::db::DbPool;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockRule {
    pub id: Option<i64>,
    pub kind: String,
    pub pattern: String,
    pub match_type: String,
    pub note: String,
    pub active: bool,
    pub created_at: i64,
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<BlockRule> {
    Ok(BlockRule {
        id: Some(row.get(0)?),
        kind: row.get(1)?,
        pattern: row.get(2)?,
        match_type: row.get(3)?,
        note: row.get(4)?,
        active: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
    })
}

// Use std::time::SystemTime::now() for created_at — avoid pulling chrono yet.
fn chrono_like_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn create(pool: &DbPool, rule: &BlockRule) -> Result<i64> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO block_rule (kind, pattern, match_type, note, active, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            rule.kind, rule.pattern, rule.match_type, rule.note,
            rule.active as i64,
            if rule.created_at == 0 { chrono_like_now() } else { rule.created_at },
        ])?;
    Ok(conn.last_insert_rowid())
}

pub fn update(pool: &DbPool, rule: &BlockRule) -> Result<()> {
    let id = rule.id.ok_or_else(|| anyhow::anyhow!("update requires id"))?;
    let conn = pool.get()?;
    conn.execute(
        "UPDATE block_rule SET kind=?1, pattern=?2, match_type=?3, note=?4, active=?5 WHERE id=?6",
        params![
            rule.kind, rule.pattern, rule.match_type, rule.note,
            rule.active as i64, id
        ])?;
    Ok(())
}

pub fn delete(pool: &DbPool, id: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM block_rule WHERE id=?1", params![id])?;
    Ok(())
}

pub fn list(pool: &DbPool) -> Result<Vec<BlockRule>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, pattern, match_type, note, active, created_at FROM block_rule ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn set_active(pool: &DbPool, id: i64, active: bool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("UPDATE block_rule SET active=?1 WHERE id=?2",
        params![active as i64, id])?;
    Ok(())
}

pub fn list_active(pool: &DbPool) -> Result<Vec<BlockRule>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, pattern, match_type, note, active, created_at FROM block_rule WHERE active=1 ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}
