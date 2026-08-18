//! PAC (Proxy Auto-Config) file generator.
//!
//! Why we use PAC instead of the HKCU `ProxyServer` setting: the latter is
//! a system-wide hard proxy — if the proxy process dies (crash, taskkill,
//! power loss), every HTTP/HTTPS request in the user's session hangs
//! against a dead port and nothing browses until the user (or Quietly on
//! next launch) manually clears the registry.
//!
//! A PAC file is plain text on disk. Windows / Edge / Chrome fetch it
//! on each request. If Quietly dies, the PAC file on disk is still a
//! valid JS function — worst case the user can only reach sites we
//! marked DIRECT. Browsing keeps working.
//!
//! Crash-safety invariant: the PAC file on disk MUST always be a
//! well-formed `FindProxyForURL` that returns `DIRECT` for any host
//! not explicitly listed. `write_safe_default` enforces this: it is
//! called from every cleanup path (focus end, app exit, app start)
//! so a stale file can never leave the user in a "all sites hang" state.

use std::path::{Path, PathBuf};

use crate::storage::block_rule::BlockRule;

pub const PAC_FILENAME: &str = "proxy.pac";
pub const PROXY_PORT: u16 = crate::engine::timer::TimerHandle::AUTO_BLOCK_PROXY_PORT;
const PROXY_SPEC: &str = concat!("PROXY 127.0.0.1:", "47820");

/// Write the PAC file. Returns the absolute path on success.
///
/// The rendered PAC returns `DIRECT` for any host / URL not matched by
/// the supplied block rules. If `rules` is empty the file is still
/// written — `FindProxyForURL` simply returns `DIRECT` for everything.
pub fn write(app_data_dir: &Path, rules: &[BlockRule]) -> std::io::Result<PathBuf> {
    let path = app_data_dir.join(PAC_FILENAME);
    let body = render(rules);
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Write a PAC file that returns `DIRECT` for everything. This is the
/// "safe default" written on every cleanup path so the user can never
/// be left with a stale file pointing at a dead proxy.
pub fn write_safe_default(app_data_dir: &Path) -> std::io::Result<PathBuf> {
    write(app_data_dir, &[])
}

/// Build the PAC JS body. Pure function — no I/O — so it's trivial to
/// unit-test the rule → JS mapping.
pub fn render(rules: &[BlockRule]) -> String {
    let mut out = String::new();
    // PAC must be valid JS. We wrap shExpMatch calls so PACs without
    // any matching rules still parse (some PAC engines reject files
    // with empty bodies).
    out.push_str("// Quietly PAC. Last regenerated: ");
    out.push_str(&timestamp_now());
    out.push_str("\n// Returns DIRECT for any host / URL not matched by the rules below.\n");
    out.push_str("// Safe to leave on disk even when Quietly is not running.\n\n");
    out.push_str("function FindProxyForURL(url, host) {\n");

    // Whitelist first: any whitelist match short-circuits to DIRECT so a
    // block rule that overlaps with a whitelist never fires on the
    // network proxy. App-path / title whitelist entries don't apply to
    // network traffic (they're process- and window-level) so we skip
    // them with a comment for visibility.
    let whitelists: Vec<&BlockRule> = rules.iter().filter(|r| r.active && r.kind == "whitelist").collect();
    let blocks: Vec<&BlockRule> = rules.iter().filter(|r| r.active && r.kind != "whitelist").collect();

    if !whitelists.is_empty() {
        out.push_str("  // Whitelist overrides — return DIRECT if any match.\n");
        for w in &whitelists {
            if let Some(cond) = rule_to_pac_condition(w, /*for_whitelist=*/ true) {
                for line in cond.lines() {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str("    return \"DIRECT\";\n");
                out.push_str("  }\n");
            }
        }
    }

    out.push_str("  // Block rules — matched entries route through the local proxy.\n");
    if blocks.is_empty() {
        out.push_str("  // (none active)\n");
    } else {
        for b in &blocks {
            if let Some(cond) = rule_to_pac_condition(b, /*for_whitelist=*/ false) {
                for line in cond.lines() {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&format!("    return \"{PROXY_SPEC}\";\n"));
                out.push_str("  }\n");
            }
        }
    }

    out.push_str("  return \"DIRECT\";\n");
    out.push_str("}\n");
    out
}

/// Convert a single rule into one or more PAC `if (...)` statements
/// (without the leading `if` — caller wraps). Returns None when the
/// rule can't be expressed in PAC (regex match types, app_path /
/// title kinds which are non-network).
fn rule_to_pac_condition(rule: &BlockRule, for_whitelist: bool) -> Option<String> {
    // Only url / domain rules map to network PAC. app_path / title are
    // enforced by the local proxy via the win-event hook, not by PAC.
    let target = match rule.kind.as_str() {
        "url" => "url",
        "domain" => "host",
        "whitelist" => {
            // Whitelist rules with app_path / title kinds don't apply
            // to the network layer; the win-event hook handles them.
            // For url / domain we mirror block matching.
            match_whitelist_target(rule)?
        }
        _ => return None,
    };

    // shExpMatch is case-insensitive and supports shell-glob wildcards
    // (* and ?). It does NOT support arbitrary regex. We translate:
    //   exact    → shExpMatch(t, pattern)
    //   contains → shExpMatch(t, *pattern*)
    //   regex    → skipped (PAC has no regex; fall through to DIRECT)
    let pattern = &rule.pattern;
    if pattern.is_empty() {
        return None;
    }
    let pac_pattern = match rule.match_type.as_str() {
        "exact" => pattern.clone(),
        "contains" => format!("*{}*", pattern),
        "regex" => {
            // Can't express regex in PAC. Return DIRECT for this rule
            // so we don't accidentally proxy it via a too-loose
            // fallback.
            if for_whitelist {
                return None;
            } else {
                // For block rules we conservatively skip — better to
                // not block (logged below) than to silently misclassify.
                return None;
            }
        }
        _ => return None,
    };

    // PAC requires double-quoted strings; escape any double quotes
    // inside the pattern.
    let escaped = pac_pattern.replace('"', "\\\"");
    Some(format!("if (shExpMatch({}, \"{}\")) {{", target, escaped))
}

fn match_whitelist_target(rule: &BlockRule) -> Option<&'static str> {
    // The whitelist rules we look at here are *url* / *domain* type
    // (BlockRule carries kind="whitelist" for the block-rule row, but
    // for PAC we need to know whether it's a host match or a URL
    // match). We reuse the rule's `kind` field via the pattern type
    // by inspecting a side-channel: the storage module doesn't carry
    // a separate "match kind" for whitelist, so we look at the
    // pattern itself — if it contains a '/' it's a URL, otherwise a
    // host. This is best-effort and adequate for the common case.
    if rule.pattern.contains('/') {
        Some("url")
    } else {
        Some("host")
    }
}

fn timestamp_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch={}", secs)
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::block_rule::BlockRule;

    fn r(kind: &str, pattern: &str, match_type: &str, active: bool) -> BlockRule {
        BlockRule {
            id: None,
            kind: kind.into(),
            pattern: pattern.into(),
            match_type: match_type.into(),
            note: String::new(),
            active,
            created_at: 0,
        }
    }

    #[test]
    fn render_empty_returns_valid_pac() {
        let s = render(&[]);
        assert!(s.contains("function FindProxyForURL"));
        assert!(s.contains("return \"DIRECT\""));
        // No rules means no shExpMatch calls, but the function must
        // still parse as JS.
        assert!(!s.contains("PROXY 127.0.0.1:47820"));
    }

    /// Brace-balance invariant: every `if (...)` opening `{` MUST have
    /// a matching `}` before the next sibling statement. The original
    /// implementation omitted the closing brace, producing PAC files
    /// that browsers rejected at fetch time — silent blocking failure.
    /// This test would have caught that bug at PR time.
    #[test]
    fn render_braces_are_balanced() {
        for n in 0..4 {
            let rules: Vec<BlockRule> = (0..n)
                .map(|i| {
                    r(if i % 2 == 0 { "domain" } else { "url" },
                      format!("example{}.com", i).as_str(),
                      "contains",
                      true)
                })
                .collect();
            let s = render(&rules);
            // Strip line comments so they don't inflate the count.
            let code: String = s
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let opens = code.matches('{').count();
            let closes = code.matches('}').count();
            assert_eq!(
                opens, closes,
                "PAC has unbalanced braces ({} open vs {} close) for {} rules.\n---\n{}\n---",
                opens, closes, n, s
            );
        }
    }

    #[test]
    fn render_domain_contains_translates_to_shExpMatch_with_globs() {
        let s = render(&[r("domain", "reddit", "contains", true)]);
        assert!(s.contains("shExpMatch(host, \"*reddit*\")"));
        assert!(s.contains("PROXY 127.0.0.1:47820"));
    }

    #[test]
    fn render_domain_exact_translates_to_shExpMatch_no_globs() {
        let s = render(&[r("domain", "example.com", "exact", true)]);
        assert!(s.contains("shExpMatch(host, \"example.com\")"));
    }

    #[test]
    fn render_url_contains_matches_against_url() {
        let s = render(&[r("url", "/login", "contains", true)]);
        assert!(s.contains("shExpMatch(url, \"*/login*\")"));
    }

    #[test]
    fn render_app_path_skipped_in_pac() {
        let s = render(&[r("app_path", "discord.exe", "exact", true)]);
        // app_path rules aren't network-relevant — PAC should not
        // include them, so the only PROXY line is absent.
        assert!(!s.contains("PROXY 127.0.0.1:47820"));
    }

    #[test]
    fn render_regex_skipped_in_pac() {
        let s = render(&[r("domain", ".*", "regex", true)]);
        // regex can't be expressed in PAC — silent skip.
        assert!(!s.contains("PROXY 127.0.0.1:47820"));
    }

    #[test]
    fn render_whitelist_emits_direct_before_blocks() {
        let rules = vec![
            r("domain", "reddit", "contains", true),       // block
            r("whitelist", "reddit.com", "contains", true), // whitelist override
        ];
        let s = render(&rules);
        // Whitelist section appears before the PROXY line.
        let wl_pos = s.find("Whitelist overrides").unwrap();
        let block_pos = s.find("PROXY 127.0.0.1:47820").unwrap();
        assert!(wl_pos < block_pos);
        assert!(s.contains("return \"DIRECT\""));
    }

    #[test]
    fn render_inactive_rules_excluded() {
        let s = render(&[r("domain", "reddit", "contains", false)]);
        assert!(!s.contains("PROXY 127.0.0.1:47820"));
    }

    #[test]
    fn write_safe_default_contains_only_direct() {
        let dir = tempdir_like();
        let path = write_safe_default(&dir).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("function FindProxyForURL"));
        assert!(body.contains("return \"DIRECT\""));
        assert!(!body.contains("PROXY"));
    }

    /// Minimal temp dir helper that avoids pulling in a `tempfile`
    /// dependency. Uses the process PID to namespace.
    fn tempdir_like() -> std::path::PathBuf {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("quietly_pac_test_{}", pid));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
