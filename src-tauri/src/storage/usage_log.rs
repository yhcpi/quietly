use rusqlite::params;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::db::DbPool;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UsageSegment {
    pub id: Option<i64>,
    pub kind: String,            // "app" | "url"
    pub target: String,          // exe file name or host
    pub ts: i64,                 // unix seconds (segment start)
    pub duration_seconds: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UsageTarget {
    pub target: String,
    pub total_seconds: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UsageDay {
    pub day: String,             // 'YYYY-MM-DD' local
    pub total_seconds: i64,
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<UsageSegment> {
    Ok(UsageSegment {
        id: Some(row.get(0)?),
        kind: row.get(1)?,
        target: row.get(2)?,
        ts: row.get(3)?,
        duration_seconds: row.get(4)?,
    })
}

/// Insert a new usage segment. `id` on the returned struct is populated.
pub fn append(pool: &DbPool, segment: &UsageSegment) -> Result<i64> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO usage_log (kind, target, ts, duration_seconds) VALUES (?1,?2,?3,?4)",
        params![
            segment.kind,
            segment.target,
            segment.ts,
            segment.duration_seconds,
        ])?;
    Ok(conn.last_insert_rowid())
}

pub fn list_recent(pool: &DbPool, since_ts: i64, limit: i64) -> Result<Vec<UsageSegment>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, target, ts, duration_seconds
         FROM usage_log WHERE ts >= ?1 ORDER BY ts DESC LIMIT ?2")?;
    let rows = stmt.query_map(params![since_ts, limit], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

/// Sum `duration_seconds` for each `target` of the given `kind` whose `ts >= since_ts`.
/// Ordered by `total_seconds DESC`.
pub fn aggregate_by_target(
    pool: &DbPool,
    kind: &str,
    since_ts: i64,
) -> Result<Vec<UsageTarget>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT target, SUM(duration_seconds)
         FROM usage_log WHERE kind=?1 AND ts >= ?2
         GROUP BY target ORDER BY SUM(duration_seconds) DESC")?;
    let rows = stmt.query_map(params![kind, since_ts], |row| {
        Ok(UsageTarget {
            target: row.get(0)?,
            total_seconds: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

/// Sum `duration_seconds` per local-day bucket for the given `kind` since `since_ts`.
pub fn aggregate_by_day(
    pool: &DbPool,
    kind: &str,
    since_ts: i64,
) -> Result<Vec<UsageDay>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%d', ts, 'unixepoch', 'localtime') AS day, SUM(duration_seconds)
         FROM usage_log WHERE kind=?1 AND ts >= ?2
         GROUP BY day ORDER BY day")?;
    let rows = stmt.query_map(params![kind, since_ts], |row| {
        Ok(UsageDay {
            day: row.get(0)?,
            total_seconds: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

/// Delete rows with `ts < before_ts`. Returns the number of rows deleted.
pub fn clear_before(pool: &DbPool, before_ts: i64) -> Result<usize> {
    let conn = pool.get()?;
    let n = conn.execute(
        "DELETE FROM usage_log WHERE ts < ?1",
        params![before_ts])?;
    Ok(n)
}