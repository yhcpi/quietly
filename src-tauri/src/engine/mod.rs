//! Core runtime: timer state machine, HTTP proxy, app-usage tracking,
//! and the Windows accessibility hook. Each sub-module owns one
//! concern and exposes a small `*_inner` API that the IPC layer wraps.

pub mod timer;
pub mod rule_match;
pub mod proxy;
pub mod proxy_state;
pub mod guard;
pub mod app_usage;
pub mod pac;
#[cfg(windows)]
pub mod background_tracker;