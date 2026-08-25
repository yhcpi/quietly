// Verifies each typed wrapper calls `invoke` with the right command
// name and argument shape. Mocks `@tauri-apps/api/core` so no Tauri
// runtime is required.

import { describe, it, expect, vi, beforeEach } from "vitest";

// `vi.hoisted` is required so the mock factory (which vitest hoists
// above this line) can close over a real `vi.fn()`.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import * as ipc from "./ipc";

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("block rules", () => {
  it("listBlockRules → list_block_rules", async () => {
    await ipc.listBlockRules();
    expect(invoke).toHaveBeenCalledWith("list_block_rules");
  });

  it("createBlockRule passes rule with created_at:0 injected", async () => {
    await ipc.createBlockRule({
      kind: "url",
      pattern: "x.com",
      match_type: "contains",
      note: "",
      active: true,
    });
    expect(invoke).toHaveBeenCalledWith("create_block_rule", {
      rule: {
        kind: "url",
        pattern: "x.com",
        match_type: "contains",
        note: "",
        active: true,
        created_at: 0,
      },
    });
  });

  it("deleteBlockRule passes id", async () => {
    await ipc.deleteBlockRule(42);
    expect(invoke).toHaveBeenCalledWith("delete_block_rule", { id: 42 });
  });

  it("setBlockRuleActive passes id+active", async () => {
    await ipc.setBlockRuleActive(7, true);
    expect(invoke).toHaveBeenCalledWith("set_block_rule_active", {
      id: 7,
      active: true,
    });
  });
});

describe("first-run", () => {
  it("needsFirstRunAck takes no args", async () => {
    invoke.mockResolvedValue(true);
    const r = await ipc.needsFirstRunAck();
    expect(r).toBe(true);
    expect(invoke).toHaveBeenCalledWith("needs_first_run_ack");
  });

  it("ackFirstRun takes no args", async () => {
    await ipc.ackFirstRun();
    expect(invoke).toHaveBeenCalledWith("ack_first_run");
  });
});

describe("hits + sessions", () => {
  it("listBlockHits uses camelCase key sinceTs", async () => {
    await ipc.listBlockHits(1_700_000_000, 100);
    expect(invoke).toHaveBeenCalledWith("list_block_hits", {
      sinceTs: 1_700_000_000,
      limit: 100,
    });
  });

  it("logBlockHit uses camelCase key ruleId", async () => {
    await ipc.logBlockHit("url", "twitter.com", 1);
    expect(invoke).toHaveBeenCalledWith("log_block_hit", {
      kind: "url",
      target: "twitter.com",
      ruleId: 1,
    });
  });
});

describe("timer / proxy / guard", () => {
  it("startFocus passes durationMin + locked + autoBlock (Tauri camelCase)", async () => {
    await ipc.startFocus(25, false, true);
    expect(invoke).toHaveBeenCalledWith("start_focus", {
      durationMin: 25,
      locked: false,
      autoBlock: true,
    });
  });

  it("startFocus forwards a locked session", async () => {
    await ipc.startFocus(50, true, true);
    expect(invoke).toHaveBeenCalledWith("start_focus", {
      durationMin: 50,
      locked: true,
      autoBlock: true,
    });
  });

  it("cancelFocus takes no args", async () => {
    await ipc.cancelFocus();
    expect(invoke).toHaveBeenCalledWith("cancel_focus");
  });

  it("isSessionLocked takes no args", async () => {
    await ipc.isSessionLocked();
    expect(invoke).toHaveBeenCalledWith("is_session_locked");
  });

  it("setSessionLocked passes locked", async () => {
    await ipc.setSessionLocked(true);
    expect(invoke).toHaveBeenCalledWith("set_session_locked", { locked: true });
  });

  it("startProxy passes port", async () => {
    await ipc.startProxy(8888);
    expect(invoke).toHaveBeenCalledWith("start_proxy", { port: 8888 });
  });

  it("stopProxy takes no args", async () => {
    await ipc.stopProxy();
    expect(invoke).toHaveBeenCalledWith("stop_proxy");
  });

  it("startGuard takes no args", async () => {
    await ipc.startGuard();
    expect(invoke).toHaveBeenCalledWith("start_guard");
  });
});

describe("whitelist", () => {
  it("listWhitelist → list_whitelist", async () => {
    await ipc.listWhitelist();
    expect(invoke).toHaveBeenCalledWith("list_whitelist");
  });

  it("createWhitelist injects created_at:0", async () => {
    await ipc.createWhitelist({
      kind: "whitelist",
      pattern: "docs.google.com",
      match_type: "contains",
      note: "",
      active: true,
    });
    expect(invoke).toHaveBeenCalledWith("create_whitelist", {
      rule: {
        kind: "whitelist",
        pattern: "docs.google.com",
        match_type: "contains",
        note: "",
        active: true,
        created_at: 0,
      },
    });
  });

  it("deleteWhitelist passes id", async () => {
    await ipc.deleteWhitelist(3);
    expect(invoke).toHaveBeenCalledWith("delete_whitelist", { id: 3 });
  });

  it("toggleWhitelist passes id", async () => {
    await ipc.toggleWhitelist(3);
    expect(invoke).toHaveBeenCalledWith("toggle_whitelist", { id: 3 });
  });
});

describe("app quotas", () => {
  it("listQuotas → list_quotas", async () => {
    await ipc.listQuotas();
    expect(invoke).toHaveBeenCalledWith("list_quotas");
  });

  it("createQuota fills the server-owned fields", async () => {
    await ipc.createQuota("Discord.exe", 60);
    expect(invoke).toHaveBeenCalledWith("create_quota", {
      quota: {
        app_pattern: "Discord.exe",
        daily_minutes: 60,
        used_seconds: 0,
        used_day: "",
        active: true,
        created_at: 0,
      },
    });
  });

  it("updateQuota wraps the row under `quota`", async () => {
    const quota = {
      id: 1,
      app_pattern: "Discord.exe",
      daily_minutes: 30,
      used_seconds: 120,
      used_day: "2026-08-16",
      active: true,
      created_at: 1_700_000_000,
    };
    await ipc.updateQuota(quota);
    expect(invoke).toHaveBeenCalledWith("update_quota", { quota });
  });

  it("deleteQuota passes id", async () => {
    await ipc.deleteQuota(9);
    expect(invoke).toHaveBeenCalledWith("delete_quota", { id: 9 });
  });

  it("setQuotaActive → set_app_quota_active with id+active", async () => {
    await ipc.setQuotaActive(9, false);
    expect(invoke).toHaveBeenCalledWith("set_app_quota_active", {
      id: 9,
      active: false,
    });
  });

  it("getQuotaStatus takes no args", async () => {
    await ipc.getQuotaStatus();
    expect(invoke).toHaveBeenCalledWith("get_quota_status");
  });

  it("getTodayUsage takes no args", async () => {
    await ipc.getTodayUsage();
    expect(invoke).toHaveBeenCalledWith("get_today_usage");
  });
});

describe("usage stats", () => {
  it("getUsageStats passes range", async () => {
    await ipc.getUsageStats("week");
    expect(invoke).toHaveBeenCalledWith("get_usage_stats", { range: "week" });
  });
});

describe("getStats", () => {
  it("defaults range to 'day'", async () => {
    await ipc.getStats();
    expect(invoke).toHaveBeenCalledWith("get_stats", { range: "day" });
  });

  it("passes explicit range", async () => {
    await ipc.getStats("week");
    expect(invoke).toHaveBeenCalledWith("get_stats", { range: "week" });
  });
});