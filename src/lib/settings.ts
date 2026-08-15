export type ParsedSettings =
  | { ok: true; value: Record<string, unknown> }
  | { ok: false; error: string };

export function parseSettingsJson(text: string): ParsedSettings {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, error: "Settings must be a JSON object" };
  }
  return { ok: true, value: parsed as Record<string, unknown> };
}

export function formatSettingsJson(value: Record<string, unknown>): string {
  return JSON.stringify(value, null, 2);
}