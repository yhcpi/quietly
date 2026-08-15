import { describe, it, expect } from "vitest";
import { parseSettingsJson, formatSettingsJson } from "./settings";

describe("parseSettingsJson", () => {
  it("parses a simple object", () => {
    const r = parseSettingsJson('{"theme": "dark"}');
    expect(r).toEqual({ ok: true, value: { theme: "dark" } });
  });

  it("parses an empty object", () => {
    expect(parseSettingsJson("{}")).toEqual({ ok: true, value: {} });
  });

  it("returns error on invalid JSON", () => {
    const r = parseSettingsJson("{not json");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(typeof r.error).toBe("string");
  });

  it("returns error when top-level is an array", () => {
    const r = parseSettingsJson("[1,2,3]");
    expect(r).toEqual({ ok: false, error: "Settings must be a JSON object" });
  });

  it("returns error when top-level is a string", () => {
    const r = parseSettingsJson('"hello"');
    expect(r).toEqual({ ok: false, error: "Settings must be a JSON object" });
  });

  it("returns error when top-level is null", () => {
    const r = parseSettingsJson("null");
    expect(r).toEqual({ ok: false, error: "Settings must be a JSON object" });
  });
});

describe("formatSettingsJson", () => {
  it("pretty-prints with 2-space indent", () => {
    const out = formatSettingsJson({ a: 1, b: { c: 2 } });
    expect(out).toBe('{\n  "a": 1,\n  "b": {\n    "c": 2\n  }\n}');
  });

  it("round-trips with parseSettingsJson", () => {
    const value = { theme: "light", volume: 0.5, list: [1, 2, 3] };
    const text = formatSettingsJson(value);
    const r = parseSettingsJson(text);
    expect(r).toEqual({ ok: true, value });
  });
});