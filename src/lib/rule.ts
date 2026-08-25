import type { BlockRule } from "../types";

/** Block rule draft — what the form hands to `create_block_rule`. */
export type RuleDraft = Omit<BlockRule, "id" | "created_at">;

/**
 * Kinds selectable in the block-rule form. `whitelist` is deliberately
 * absent — whitelist rules are created from the Rules page's Whitelist
 * tab, which pins the kind.
 */
export const RULE_KINDS: ReadonlyArray<BlockRule["kind"]> = [
  "url",
  "domain",
  "app_path",
  "title",
];

/** Match types supported by the rule engine (exact, substring, regex). */
export const RULE_MATCH_TYPES: ReadonlyArray<BlockRule["match_type"]> = [
  "exact",
  "contains",
  "regex",
];

/** A fresh, empty rule with sane defaults. */
export function blankRule(): RuleDraft {
  return {
    kind: "url",
    pattern: "",
    match_type: "contains",
    note: "",
    active: true,
  };
}

/**
 * Return a human-readable error string if the draft is invalid, or
 * `null` if it can be saved. Validates pattern presence and (for
 * regex match type) that `new RegExp(pattern)` succeeds.
 */
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