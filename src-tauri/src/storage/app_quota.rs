use rusqlite::params;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::db::DbPool;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppQuota {
    pub id: Option<i64>,
    pub app_pattern: String,
    pub daily_minutes: i64,
    pub used_seconds: i64,
    pub used_day: String,
    pub active: bool,
    pub created_at: i64,
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<AppQuota> {
    Ok(AppQuota {
        id: Some(row.get(0)?),
        app_pattern: row.get(1)?,
        daily_minutes: row.get(2)?,
        used_seconds: row.get(3)?,
        used_day: row.get(4)?,
        active: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
    })
}

fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn create(pool: &DbPool, quota: &AppQuota) -> Result<i64> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO app_quota (app_pattern, daily_minutes, used_seconds, used_day, active, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            quota.app_pattern.to_lowercase(),
            quota.daily_minutes,
            quota.used_seconds,
            quota.used_day,
            quota.active as i64,
            if quota.created_at == 0 { now() } else { quota.created_at },
        ])?;
    Ok(conn.last_insert_rowid())
}

pub fn update(pool: &DbPool, quota: &AppQuota) -> Result<()> {
    let id = quota.id.ok_or_else(|| anyhow::anyhow!("update requires id"))?;
    let conn = pool.get()?;
    conn.execute(
        "UPDATE app_quota SET app_pattern=?1, daily_minutes=?2, used_seconds=?3,
         used_day=?4, active=?5 WHERE id=?6",
        params![
            quota.app_pattern.to_lowercase(),
            quota.daily_minutes,
            quota.used_seconds,
            quota.used_day,
            quota.active as i64,
            id,
        ])?;
    Ok(())
}

pub fn delete(pool: &DbPool, id: i64) -> Result<()> {
    let conn = pool.get()?;
    conn.execute("DELETE FROM app_quota WHERE id=?1", params![id])?;
    Ok(())
}

pub fn list(pool: &DbPool) -> Result<Vec<AppQuota>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, app_pattern, daily_minutes, used_seconds, used_day, active, created_at
         FROM app_quota ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn list_active(pool: &DbPool) -> Result<Vec<AppQuota>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, app_pattern, daily_minutes, used_seconds, used_day, active, created_at
         FROM app_quota WHERE active=1 ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], from_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn set_active(pool: &DbPool, id: i64, active: bool) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE app_quota SET active=?1 WHERE id=?2",
        params![active as i64, id])?;
    Ok(())
}

/// Increment `used_seconds` for an active quota matching `pattern` by `delta_seconds`.
/// If `used_day` differs from `today`, reset `used_seconds` to 0 before applying the delta.
/// Returns `(new_used_seconds, exceeded)` where `exceeded` is true iff the new value
/// crosses `daily_minutes * 60` for the first time today (i.e. the pre-increment value
/// was strictly below the threshold and the new value is >= the threshold).
///
/// If no active quota matches `pattern`, returns `Ok((0, false))`.
pub fn increment_used(
    pool: &DbPool,
    pattern: &str,
    delta_seconds: i64,
    today: &str,
) -> Result<(i64, bool)> {
    let conn = pool.get()?;
    // Find the active quota for this pattern (first match wins, per design).
    let mut stmt = conn.prepare(
        "SELECT id, daily_minutes, used_seconds, used_day
         FROM app_quota WHERE app_pattern=?1 AND active=1 LIMIT 1")?;
    let row = stmt.query_row(params![pattern], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    });
    let (id, daily_minutes, mut used_seconds, used_day) = match row {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok((0, false)),
        Err(e) => return Err(e.into()),
    };

    // Daily reset: if used_day differs from today, reset the counter.
    if used_day != today {
        used_seconds = 0;
    }

    let threshold = daily_minutes.saturating_mul(60);
    let was_below = used_seconds < threshold;
    let new_used = used_seconds.saturating_add(delta_seconds);
    let now_exceeded = new_used >= threshold;
    let crossed = was_below && now_exceeded;

    conn.execute(
        "UPDATE app_quota SET used_seconds=?1, used_day=?2 WHERE id=?3",
        params![new_used, today, id])?;
    Ok((new_used, crossed))
}