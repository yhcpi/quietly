// Mirrors `src-tauri/src/storage/*.rs` (Serialize/Deserialize shapes).
// Field names are snake_case to match the Rust structs exactly — Tauri
// IPC passes JSON, and the Rust serde rename is the default (no rename).

export type BlockRule = {
  id: number | null;
  // `whitelist` inverts url/domain/app_path blocks; `title` matches the
  // foreground window title. Both are plain `block_rule` rows on the Rust
  // side — only the `kind` value differs.
  kind: "url" | "domain" | "app_path" | "title" | "whitelist";
  pattern: string;
  match_type: "exact" | "contains" | "regex";
  note: string;
  active: boolean;
  created_at: number;
};

export type Session = {
  id: number;
  started_at: number;
  ended_at: number | null;
  duration_seconds: number;
  phase: string;
  completed: boolean;
};

export type BlockHit = {
  id: number;
  ts: number;
  kind: string;
  target: string;
  rule_id: number | null;
};

export type Stats = {
  sessions_today: number;
  completed_sessions: number;
  focus_minutes_today: number;
  blocks_today: number;
};

export type TimerState = {
  status: "Running" | "Paused" | "Completed";
  phase: "Focus" | "ShortBreak" | "LongBreak";
  remaining_seconds: number;
  total_seconds: number;
  cycle: number;
};

// `storage::app_quota::AppQuota`
export type AppQuota = {
  id: number | null;
  app_pattern: string;
  daily_minutes: number;
  used_seconds: number;
  used_day: string;
  active: boolean;
  created_at: number;
};

// `ipc::commands::QuotaStatusEntry` — today's usage with the daily reset
// already applied on the Rust side.
export type QuotaStatus = {
  app_pattern: string;
  used_minutes: number;
  daily_minutes: number;
  exceeded: boolean;
};

// `ipc::commands::UsageEntry`
export type UsageEntry = {
  target: string;
  minutes: number;
  kind: "app" | "url";
};

// Ranges accepted by `get_stats` and `get_usage_stats`.
export type Range = "day" | "week" | "month";