/**
 * Typed wrapper around Tauri `invoke`. One function per Rust command
 * in `src-tauri/src/ipc/commands.rs`. Pure transport — no caching,
 * no retries, no event subscriptions. UI state lives in route
 * components and the React tree, not here.
 */

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
/** All non-whitelist block rules, newest first. */
export const listBlockRules = () => invoke<BlockRule[]>("list_block_rules");
/** Insert a new block rule. Returns the assigned row id. */
export const createBlockRule = (rule: Omit<BlockRule, "id" | "created_at">) =>
  invoke<number>("create_block_rule", { rule: { ...rule, created_at: 0 } });
/** Replace an existing rule by id. */
export const updateBlockRule = (rule: BlockRule) =>
  invoke<void>("update_block_rule", { rule });
/** Delete a rule by id. */
export const deleteBlockRule = (id: number) =>
  invoke<void>("delete_block_rule", { id });
/** Toggle the `active` flag without changing the pattern. */
export const setBlockRuleActive = (id: number, active: boolean) =>
  invoke<void>("set_block_rule_active", { id, active });

// ─── first-run ack ──────────────────────────────────────────────
/** Whether the user still needs to see the first-run guide. */
export const needsFirstRunAck = () => invoke<boolean>("needs_first_run_ack");
/** Mark the first-run guide as seen. Subsequent launches skip it. */
export const ackFirstRun = () => invoke<void>("ack_first_run");

// ─── block hits ─────────────────────────────────────────────────
/** Recent block events, newest first, filtered by `sinceTs` and capped by `limit`. */
export const listBlockHits = (sinceTs: number, limit: number) =>
  invoke<BlockHit[]>("list_block_hits", { sinceTs, limit });
/** Append a block event. `ruleId` is null when the match came from a generic pattern. */
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
/** Aggregate block-hit counts over the requested range. */
export const getStats = (range: Range = "day") =>
  invoke<Stats>("get_stats", { range });

// ─── focus timer ────────────────────────────────────────────────
// `start_focus` starts the proxy + guard itself when `autoBlock` is true —
// callers must NOT also call startProxy/startGuard. `locked` disables
// cancel for the lifetime of the session.
/** Start a focus session. Returns the new session id. */
export const startFocus = (
  durationMin: number,
  locked: boolean,
  autoBlock: boolean,
) => invoke<number>("start_focus", { durationMin, locked, autoBlock });
/** Pause the running timer. No-op if not running. */
export const pauseFocus = () => invoke<void>("pause_focus");
/** Resume a paused timer. No-op if not paused. */
export const resumeFocus = () => invoke<void>("resume_focus");
// Rejects with "session is locked — cancel disabled" when locked.
// On success the Rust side also tears down guard + proxy.
/** Cancel the running session. Rejects when the session is locked. */
export const cancelFocus = () => invoke<void>("cancel_focus");
/** Flip the lock state mid-session. Locked sessions cannot be cancelled. */
export const setSessionLocked = (locked: boolean) =>
  invoke<void>("set_session_locked", { locked });
/** Whether the current session is locked against cancellation. */
export const isSessionLocked = () => invoke<boolean>("is_session_locked");

// ─── proxy ──────────────────────────────────────────────────────
/** Bind the local HTTP proxy on `port` and start serving traffic. */
export const startProxy = (port: number) =>
  invoke<void>("start_proxy", { port });
/** Stop the local proxy and restore the user's previous proxy state. */
export const stopProxy = () => invoke<void>("stop_proxy");

// ─── guard ──────────────────────────────────────────────────────
/** Start the per-foreground-app guard loop (enforces app-path blocks). */
export const startGuard = () => invoke<void>("start_guard");

// ─── whitelist (block_rule rows with kind="whitelist") ──────────
/** All active whitelist rules. */
export const listWhitelist = () => invoke<BlockRule[]>("list_whitelist");
/** Insert a whitelist rule. The Rust side forces `kind="whitelist"`. */
export const createWhitelist = (rule: Omit<BlockRule, "id" | "created_at">) =>
  invoke<number>("create_whitelist", { rule: { ...rule, created_at: 0 } });
/** Delete a whitelist rule by id. */
export const deleteWhitelist = (id: number) =>
  invoke<void>("delete_whitelist", { id });
/** Flip a whitelist rule's `active` flag and return the new value. */
export const toggleWhitelist = (id: number) =>
  invoke<boolean>("toggle_whitelist", { id });

// ─── app quotas ─────────────────────────────────────────────────
/** All configured app quotas. */
export const listQuotas = () => invoke<AppQuota[]>("list_quotas");
/** Create a new quota. Throws if the pattern is empty / already taken. */
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
/** Update an existing quota. */
export const updateQuota = (quota: AppQuota) =>
  invoke<void>("update_quota", { quota });
/** Delete a quota by id. */
export const deleteQuota = (id: number) =>
  invoke<void>("delete_quota", { id });
/** Toggle a quota's `active` flag. */
export const setQuotaActive = (id: number, active: boolean) =>
  invoke<void>("set_app_quota_active", { id, active });
/** Status of every active quota against today's usage. */
export const getQuotaStatus = () =>
  invoke<QuotaStatus[]>("get_quota_status");
/** Today's usage snapshot (used / remaining / breached). */
export const getTodayUsage = () =>
  invoke<QuotaStatus[]>("get_today_usage");

// ─── usage stats ────────────────────────────────────────────────
/** Per-app usage aggregates over the requested range. */
export const getUsageStats = (range: Range) =>
  invoke<UsageEntry[]>("get_usage_stats", { range });