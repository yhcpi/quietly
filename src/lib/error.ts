/**
 * Convert an unknown thrown value into a user-displayable string.
 *
 * Tauri's IPC layer rejects with the Rust error string verbatim, which
 * often includes a leading `"` (from `serde_json` round-tripping a plain
 * `&str`) and internal phrasing. Strip the JSON noise; leave the rest
 * alone for the user.
 */
export function humanizeError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return stripJsonQuotes(e);
  return String(e);
}

function stripJsonQuotes(s: string): string {
  // serde_json serializes a Rust `&str` with surrounding quotes. If the
  // payload is itself a JSON-quoted string we get `"text"`; strip the
  // outer pair so the user sees the message, not the framing.
  if (s.length >= 2 && s.startsWith('"') && s.endsWith('"')) {
    return s.slice(1, -1).replace(/\\"/g, '"');
  }
  return s;
}