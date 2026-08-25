// Typed wrapper around Tauri `invoke`. One function per Rust command
// in `src-tauri/src/ipc/commands.rs`. Pure transport — no caching,
// no retries, no event subscriptions. Higher layers (Tasks 14-18) may
// add their own state management on top.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppQuota,
  BlockRule,
  BlockHit,
  QuotaStatus,
  Range,
  Stats,
  UsageEntry,
} from "./types";

// ─── block rules ────────────────────────────────────────────────
export const listBlockRules = () => invoke<BlockRule[]>("list_block_rules");
export const createBlockRule = (rule: Omit<BlockRule, "id" | "created_at">) =>
  invoke<number>("create_block_rule", { rule: { ...rule, created_at: 0 } });
export const updateBlockRule = (rule: BlockRule) =>
  invoke<void>("update_block_rule", { rule });
export const deleteBlockRule = (id: number) =>
  invoke<void>("delete_block_rule", { id });
export const setBlockRuleActive = (id: number, active: boolean) =>
  invoke<void>("set_block_rule_active", { id, active });

// ─── first-run ack ──────────────────────────────────────────────
export const needsFirstRunAck = () => invoke<boolean>("needs_first_run_ack");
export const ackFirstRun = () => invoke<void>("ack_first_run");

// ─── block hits ─────────────────────────────────────────────────
export const listBlockHits = (sinceTs: number, limit: number) =>
  invoke<BlockHit[]>("list_block_hits", { sinceTs, limit });
export const logBlockHit = (
  kind: string,
  target: string,
  ruleId: number | null,
) =>
  invoke<void>("log_block_hit", {
    kind,
    target,
    ruleId,
  });

// ─── stats ──────────────────────────────────────────────────────
export const getStats = (range: Range = "day") =>
  invoke<Stats>("get_stats", { range });

// ─── focus timer ────────────────────────────────────────────────
// `start_focus` starts the proxy + guard itself when `autoBlock` is true —
// callers must NOT also call startProxy/startGuard. `locked` disables
// cancel for the lifetime of the session.
export const startFocus = (
  durationMin: number,
  locked: boolean,
  autoBlock: boolean,
) => invoke<number>("start_focus", { durationMin, locked, autoBlock });
export const pauseFocus = () => invoke<void>("pause_focus");
export const resumeFocus = () => invoke<void>("resume_focus");
// Rejects with "session is locked — cancel disabled" when locked.
// On success the Rust side also tears down guard + proxy.
export const cancelFocus = () => invoke<void>("cancel_focus");
export const setSessionLocked = (locked: boolean) =>
  invoke<void>("set_session_locked", { locked });
export const isSessionLocked = () => invoke<boolean>("is_session_locked");

// ─── proxy ──────────────────────────────────────────────────────
export const startProxy = (port: number) =>
  invoke<void>("start_proxy", { port });
export const stopProxy = () => invoke<void>("stop_proxy");

// ─── guard ──────────────────────────────────────────────────────
export const startGuard = () => invoke<void>("start_guard");

// ─── whitelist (block_rule rows with kind="whitelist") ──────────
export const listWhitelist = () => invoke<BlockRule[]>("list_whitelist");
// `kind` is forced to "whitelist" by the Rust side, so the draft's own
// kind is irrelevant here.
export const createWhitelist = (rule: Omit<BlockRule, "id" | "created_at">) =>
  invoke<number>("create_whitelist", { rule: { ...rule, created_at: 0 } });
export const deleteWhitelist = (id: number) =>
  invoke<void>("delete_whitelist", { id });
// Flips `active` and returns the new value.
export const toggleWhitelist = (id: number) =>
  invoke<boolean>("toggle_whitelist", { id });

// ─── app quotas ─────────────────────────────────────────────────
export const listQuotas = () => invoke<AppQuota[]>("list_quotas");
export const createQuota = (appPattern: string, dailyMinutes: number) =>
  invoke<number>("create_quota", {
    quota: {
      app_pattern: appPattern,
      daily_minutes: dailyMinutes,
      used_seconds: 0,
      used_day: "",
      active: true,
      created_at: 0,
    },
  });
export const updateQuota = (quota: AppQuota) =>
  invoke<void>("update_quota", { quota });
export const deleteQuota = (id: number) =>
  invoke<void>("delete_quota", { id });
export const setQuotaActive = (id: number, active: boolean) =>
  invoke<void>("set_app_quota_active", { id, active });
export const getQuotaStatus = () =>
  invoke<QuotaStatus[]>("get_quota_status");
export const getTodayUsage = () =>
  invoke<QuotaStatus[]>("get_today_usage");

// ─── usage stats ────────────────────────────────────────────────
export const getUsageStats = (range: Range) =>
  invoke<UsageEntry[]>("get_usage_stats", { range });