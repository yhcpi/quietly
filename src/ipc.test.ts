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

describe("schedules", () => {
  it("toggleSchedule passes id+enabled", async () => {
    await ipc.toggleSchedule(3, false);
    expect(invoke).toHaveBeenCalledWith("toggle_schedule", {
      id: 3,
      enabled: false,
    });
  });
});

describe("settings + first-run", () => {
  it("setSettings wraps value under `settings`", async () => {
    await ipc.setSettings({ theme: "dark" });
    expect(invoke).toHaveBeenCalledWith("set_settings", {
      settings: { theme: "dark" },
    });
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

  it("logBlockHit uses camelCase keys ruleId/scheduleId", async () => {
    await ipc.logBlockHit("url", "twitter.com", 1, null);
    expect(invoke).toHaveBeenCalledWith("log_block_hit", {
      kind: "url",
      target: "twitter.com",
      ruleId: 1,
      scheduleId: null,
    });
  });
});

describe("timer / proxy / guard", () => {
  it("startFocus passes durationMin (Tauri camelCase)", async () => {
    await ipc.startFocus(25);
    expect(invoke).toHaveBeenCalledWith("start_focus", { durationMin: 25 });
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