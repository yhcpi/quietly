//! SQLite-backed storage: schema bootstrap, CRUD for rules /
//! sessions / quotas / usage logs, and the proxy recovery marker.
//! All public functions take `&Pool` so callers stay in r2d2 land.

pub mod db;
pub mod first_run;
pub mod block_rule;
pub mod session;
pub mod block_hit;
pub mod proxy_marker;
pub mod app_quota;
pub mod usage_log;