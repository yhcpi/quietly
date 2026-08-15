// Mirrors `src-tauri/src/storage/*.rs` (Serialize/Deserialize shapes).
// Field names are snake_case to match the Rust structs exactly — Tauri
// IPC passes JSON, and the Rust serde rename is the default (no rename).

export type BlockRule = {
  id: number | null;
  kind: "url" | "domain" | "app_path";
  pattern: string;
  match_type: "exact" | "contains" | "regex";
  note: string;
  active: boolean;
  created_at: number;
};

export type Schedule = {
  id: number | null;
  name: string;
  kind: "timer" | "weekly";
  enabled: boolean;
  config_json: string;
  block_rule_ids: string;
  created_at: number;
};

export type Session = {
  id: number;
  schedule_id: number | null;
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
  schedule_id: number | null;
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