import type { BlockRule } from "../types";

export type RuleDraft = Omit<BlockRule, "id" | "created_at">;

export const RULE_KINDS: ReadonlyArray<BlockRule["kind"]> = [
  "url",
  "domain",
  "app_path",
];

export const RULE_MATCH_TYPES: ReadonlyArray<BlockRule["match_type"]> = [
  "exact",
  "contains",
  "regex",
];

export function blankRule(): RuleDraft {
  return {
    kind: "url",
    pattern: "",
    match_type: "contains",
    note: "",
    active: true,
  };
}

export function validateRule(rule: RuleDraft): string | null {
  if (!rule.pattern.trim()) {
    return "Pattern is required";
  }
  if (rule.match_type === "regex") {
    try {
      new RegExp(rule.pattern);
    } catch {
      return "Invalid regex pattern";
    }
  }
  return null;
}