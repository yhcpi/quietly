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

/// The PAC directive for the local proxy at the given port. The
/// kernel-assigned port comes from the TcpListener's `local_addr()`
/// after `proxy::start` binds port 0 — see `engine/proxy.rs`.
pub fn proxy_spec_for(port: u16) -> String {
    format!("PROXY 127.0.0.1:{}", port)
}

/// Write the PAC file. Returns the absolute path on success.
///
/// The rendered PAC returns `DIRECT` for any host / URL not matched by
/// the supplied block rules. If `rules` is empty the file is still
/// written — `FindProxyForURL` simply returns `DIRECT` for everything.
pub fn write(app_data_dir: &Path, rules: &[BlockRule], port: u16) -> std::io::Result<PathBuf> {
    let path = app_data_dir.join(PAC_FILENAME);
    let body = render(rules, port);
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Write a PAC file that returns `DIRECT` for everything. This is the
/// "safe default" written on every cleanup path so the user can never
/// be left with a stale file pointing at a dead proxy.
pub fn write_safe_default(app_data_dir: &Path) -> std::io::Result<PathBuf> {
    write(app_data_dir, &[], 0)
}

/// Build the PAC JS body. Pure function — no I/O — so it's trivial to
/// unit-test the rule → JS mapping. `port` is the local proxy port
/// (the kernel-assigned one when called from `start_proxy_inner`).
pub fn render(rules: &[BlockRule], port: u16) -> String {
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
                out.push_str(&format!("    return \"{}\";\n", proxy_spec_for(port)));
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

    // Tests use a fixed test port so we can assert on the rendered
    // PROXY directive. The real port comes from the kernel-assigned
    // listener address at runtime — see `engine/proxy.rs`.
    const TEST_PORT: u16 = 47820;

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
        let s = render(&[], TEST_PORT);
        assert!(s.contains("function FindProxyForURL"));
        assert!(s.contains("return \"DIRECT\""));
        // No rules means no shExpMatch calls, but the function must
        // still parse as JS.
        assert!(!s.contains(proxy_spec_for(TEST_PORT).as_str()));
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
            let s = render(&rules, TEST_PORT);
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
    fn render_domain_contains_translates_to_shexpmatch_with_globs() {
        let s = render(&[r("domain", "reddit", "contains", true)], TEST_PORT);
        assert!(s.contains("shExpMatch(host, \"*reddit*\")"));
        assert!(s.contains(proxy_spec_for(TEST_PORT).as_str()));
    }

    #[test]
    fn render_domain_exact_translates_to_shexpmatch_no_globs() {
        let s = render(&[r("domain", "example.com", "exact", true)], TEST_PORT);
        assert!(s.contains("shExpMatch(host, \"example.com\")"));
    }

    #[test]
    fn render_url_contains_matches_against_url() {
        let s = render(&[r("url", "/login", "contains", true)], TEST_PORT);
        assert!(s.contains("shExpMatch(url, \"*/login*\")"));
    }

    #[test]
    fn render_app_path_skipped_in_pac() {
        let s = render(&[r("app_path", "discord.exe", "exact", true)], TEST_PORT);
        // app_path rules aren't network-relevant — PAC should not
        // include them, so the only PROXY line is absent.
        assert!(!s.contains(proxy_spec_for(TEST_PORT).as_str()));
    }

    #[test]
    fn render_regex_skipped_in_pac() {
        let s = render(&[r("domain", ".*", "regex", true)], TEST_PORT);
        // regex can't be expressed in PAC — silent skip.
        assert!(!s.contains(proxy_spec_for(TEST_PORT).as_str()));
    }

    #[test]
    fn render_whitelist_emits_direct_before_blocks() {
        let rules = vec![
            r("domain", "reddit", "contains", true),       // block
            r("whitelist", "reddit.com", "contains", true), // whitelist override
        ];
        let s = render(&rules, TEST_PORT);
        // Whitelist section appears before the PROXY line.
        let wl_pos = s.find("Whitelist overrides").unwrap();
        let block_pos = s.find(proxy_spec_for(TEST_PORT).as_str()).unwrap();
        assert!(wl_pos < block_pos);
        assert!(s.contains("return \"DIRECT\""));
    }

    #[test]
    fn render_inactive_rules_excluded() {
        let s = render(&[r("domain", "reddit", "contains", false)], TEST_PORT);
        assert!(!s.contains(proxy_spec_for(TEST_PORT).as_str()));
    }

    #[test]
    fn proxy_spec_for_uses_supplied_port() {
        assert_eq!(proxy_spec_for(0), "PROXY 127.0.0.1:0");
        assert_eq!(proxy_spec_for(47820), "PROXY 127.0.0.1:47820");
        assert_eq!(proxy_spec_for(65535), "PROXY 127.0.0.1:65535");
    }

    #[test]
    fn write_safe_default_contains_only_direct() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_safe_default(dir.path()).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("function FindProxyForURL"));
        assert!(body.contains("return \"DIRECT\""));
        assert!(!body.contains("PROXY"));
    }

    /// `write` with a non-empty rule set must (a) produce a file with
    /// the proxy directive for the supplied port and (b) point at the
    /// path the caller asked for, so callers (e.g. start_proxy_inner)
    /// can read it back and use it as AutoConfigURL.
    #[test]
    fn write_with_rules_emits_proxy_directive() {
        let dir = tempfile::tempdir().unwrap();
        let rules = vec![r("domain", "reddit.com", "contains", true)];
        let path = write(dir.path(), &rules, TEST_PORT).unwrap();
        assert_eq!(path, dir.path().join(PAC_FILENAME));
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains(proxy_spec_for(TEST_PORT).as_str()));
        assert!(body.contains("shExpMatch(host, \"*reddit.com*\")"));
    }

    /// Inactive rules must not show up in the rendered PAC, even when
    /// `write` is called with them present.
    #[test]
    fn write_drops_inactive_rules() {
        let dir = tempfile::tempdir().unwrap();
        let rules = vec![r("domain", "reddit.com", "contains", false)];
        let path = write(dir.path(), &rules, TEST_PORT).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(!body.contains("PROXY"));
        assert!(!body.contains("reddit.com"));
    }
}
