// Typed wrapper around Tauri `invoke`. One function per Rust command
// in `src-tauri/src/ipc/commands.rs`. Pure transport — no caching,
// no retries, no event subscriptions. Higher layers (Tasks 14-18) may
// add their own state management on top.

import { invoke } from "@tauri-apps/api/core";
import type {
  BlockRule,
  BlockHit,
  Schedule,
  Session,
  Stats,
} from "./types";

// ─── block rules ────────────────────────────────────────────────
export const listBlockRules = () => invoke<BlockRule[]>("list_block_rules");
export const createBlockRule = (rule: Omit<BlockRule, "id" | "created_at">) =>
  invoke<number>("create_block_rule", { rule });
export const updateBlockRule = (rule: BlockRule) =>
  invoke<void>("update_block_rule", { rule });
export const deleteBlockRule = (id: number) =>
  invoke<void>("delete_block_rule", { id });
export const setBlockRuleActive = (id: number, active: boolean) =>
  invoke<void>("set_block_rule_active", { id, active });

// ─── schedules ──────────────────────────────────────────────────
export const listSchedules = () => invoke<Schedule[]>("list_schedules");
export const createSchedule = (s: Omit<Schedule, "id" | "created_at">) =>
  invoke<number>("create_schedule", { schedule: s });
export const updateSchedule = (s: Schedule) =>
  invoke<void>("update_schedule", { schedule: s });
export const deleteSchedule = (id: number) =>
  invoke<void>("delete_schedule", { id });
export const toggleSchedule = (id: number, enabled: boolean) =>
  invoke<void>("toggle_schedule", { id, enabled });

// ─── settings ───────────────────────────────────────────────────
export const getSettings = () =>
  invoke<Record<string, unknown>>("get_settings");
export const setSettings = (value: Record<string, unknown>) =>
  invoke<void>("set_settings", { settings: value });

// ─── first-run ack ──────────────────────────────────────────────
export const needsFirstRunAck = () => invoke<boolean>("needs_first_run_ack");
export const ackFirstRun = () => invoke<void>("ack_first_run");

// ─── block hits ─────────────────────────────────────────────────
export const listBlockHits = (sinceTs: number, limit: number) =>
  invoke<BlockHit[]>("list_block_hits", { since_ts: sinceTs, limit });
export const logBlockHit = (
  kind: string,
  target: string,
  ruleId: number | null,
  scheduleId: number | null,
) =>
  invoke<void>("log_block_hit", {
    kind,
    target,
    rule_id: ruleId,
    schedule_id: scheduleId,
  });

// ─── sessions ───────────────────────────────────────────────────
export const listSessions = (sinceTs: number) =>
  invoke<Session[]>("list_sessions", { since_ts: sinceTs });

// ─── stats ──────────────────────────────────────────────────────
export const getStats = (range: "day" | "week" | "month" = "day") =>
  invoke<Stats>("get_stats", { range });

// ─── focus timer ────────────────────────────────────────────────
export const startFocus = (durationMin: number) =>
  invoke<number>("start_focus", { duration_min: durationMin });
export const pauseFocus = () => invoke<void>("pause_focus");
export const resumeFocus = () => invoke<void>("resume_focus");
export const cancelFocus = () => invoke<void>("cancel_focus");

// ─── proxy ──────────────────────────────────────────────────────
export const startProxy = (port: number) =>
  invoke<void>("start_proxy", { port });
export const stopProxy = () => invoke<void>("stop_proxy");

// ─── guard ──────────────────────────────────────────────────────
export const startGuard = () => invoke<void>("start_guard");
export const stopGuard = () => invoke<void>("stop_guard");