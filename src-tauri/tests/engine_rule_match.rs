use quietly_lib::storage::block_rule::BlockRule;
use quietly_lib::engine::rule_match::*;

fn rule(kind: &str, pattern: &str, match_type: &str) -> BlockRule {
    BlockRule {
        id: Some(1),
        kind: kind.into(),
        pattern: pattern.into(),
        match_type: match_type.into(),
        note: "".into(),
        active: true,
        created_at: 0,
    }
}

#[test]
fn url_exact_match() {
    let r = rule("url", "https://twitter.com/foo", "exact");
    assert!(matches_url(&r, "https://twitter.com/foo"));
    assert!(!matches_url(&r, "https://twitter.com"));
}

#[test]
fn url_contains_match() {
    let r = rule("url", "twitter.com", "contains");
    assert!(matches_url(&r, "https://twitter.com/foo"));
    assert!(matches_url(&r, "https://twitter.com"));
    assert!(!matches_url(&r, "https://facebook.com"));
}

#[test]
fn url_regex_match() {
    let r = rule("url", r"twitter\.com/.*", "regex");
    assert!(matches_url(&r, "https://twitter.com/foo"));
    assert!(!matches_url(&r, "https://twitter.com"));  // no trailing slash
}

#[test]
fn domain_exact() {
    let r = rule("domain", "twitter.com", "exact");
    assert!(matches_url(&r, "https://twitter.com/foo"));
    assert!(matches_url(&r, "http://twitter.com"));
    assert!(!matches_url(&r, "https://www.twitter.com/foo"));  // www ≠ exact
}

#[test]
fn domain_contains() {
    let r = rule("domain", "twitter", "contains");
    assert!(matches_url(&r, "https://twitter.com/foo"));
    assert!(matches_url(&r, "https://www.twitter.com"));
    assert!(!matches_url(&r, "https://facebook.com"));
}

#[test]
fn domain_strips_port() {
    let r = rule("domain", "twitter.com", "exact");
    assert!(matches_url(&r, "https://twitter.com:443/foo"));
}

#[test]
fn url_rule_does_not_match_app() {
    let r = rule("url", "twitter.com", "contains");
    assert!(!matches_app(&r, "C:\\Windows\\System32\\notepad.exe"));
}

#[test]
fn app_rule_does_not_match_url() {
    let r = rule("app_path", "discord", "contains");
    assert!(!matches_url(&r, "https://discord.com"));
}

#[test]
fn app_exact_match() {
    let r = rule("app_path", "C:\\Windows\\System32\\notepad.exe", "exact");
    assert!(matches_app(&r, "C:\\Windows\\System32\\notepad.exe"));
    assert!(!matches_app(&r, "C:\\Windows\\System32\\calc.exe"));
}

#[test]
fn app_contains_match() {
    let r = rule("app_path", "discord", "contains");
    assert!(matches_app(&r, "C:\\Users\\me\\AppData\\Local\\Discord\\Update.exe"));
    assert!(!matches_app(&r, "C:\\Program Files\\Slack\\slack.exe"));
}

#[test]
fn app_regex_match() {
    let r = rule("app_path", r"Discord\\.*\.exe", "regex");
    assert!(matches_app(&r, "C:\\Users\\me\\AppData\\Local\\Discord\\Update.exe"));
    assert!(!matches_app(&r, "C:\\Windows\\System32\\notepad.exe"));
}

#[test]
fn invalid_regex_yields_false() {
    let r = rule("url", "[invalid(regex", "regex");
    assert!(!matches_url(&r, "https://twitter.com"));
}

#[test]
fn unknown_match_type_yields_false() {
    let r = rule("url", "twitter.com", "frobnicate");
    assert!(!matches_url(&r, "https://twitter.com"));
}

#[test]
fn title_exact_match() {
    let r = rule("title", "YouTube — Music", "exact");
    assert!(matches_title(&r, "YouTube — Music"));
    assert!(!matches_title(&r, "YouTube — Home"));
}

#[test]
fn title_contains_match() {
    let r = rule("title", "Music", "contains");
    assert!(matches_title(&r, "YouTube — Music"));
    assert!(matches_title(&r, "Spotify - Music for everyone"));
    assert!(!matches_title(&r, "YouTube — Home"));
}

#[test]
fn title_regex_match() {
    let r = rule("title", r"YouTube\s*[-—]\s*Music", "regex");
    assert!(matches_title(&r, "YouTube — Music"));
    assert!(matches_title(&r, "YouTube -  Music"));
    assert!(!matches_title(&r, "YouTube — Home"));
}

#[test]
fn title_rule_does_not_match_url_or_app() {
    let r = rule("title", "Music", "contains");
    assert!(!matches_url(&r, "https://example.com/Music"));
    assert!(!matches_app(&r, "C:\\Path\\Music.exe"));
}

#[test]
fn title_invalid_regex_yields_false() {
    let r = rule("title", "[invalid(regex", "regex");
    assert!(!matches_title(&r, "anything"));
}

#[test]
fn title_unknown_match_type_yields_false() {
    let r = rule("title", "Music", "frobnicate");
    assert!(!matches_title(&r, "YouTube — Music"));
}

// ─── `kind=url` host-vs-host fallback ───────────────────────────────────
//
// A user typing `https://www.bilibili.com/` into a `kind=url` rule
// expects the whole domain blocked. Without the fallback, the literal
// substring matcher only catches requests whose URL string contains
// `https://www.bilibili.com/` — so `http://www.bilibili.com/` slips
// past. The fallback extracts host from both sides and compares.

#[test]
fn url_rule_with_scheme_blocks_http_request() {
    let r = rule("url", "https://www.bilibili.com/", "contains");
    assert!(matches_url(&r, "http://www.bilibili.com/"));
    assert!(matches_url(&r, "http://www.bilibili.com"));
    assert!(matches_url(&r, "https://www.bilibili.com/"));
}

#[test]
fn url_rule_with_scheme_blocks_request_with_path() {
    let r = rule("url", "https://www.bilibili.com/", "contains");
    assert!(matches_url(&r, "https://www.bilibili.com/video/BV1xx"));
}

#[test]
fn url_rule_with_path_does_not_expand_to_whole_host() {
    // Path-specific rule must not silently grow to cover the whole host.
    let r = rule("url", "https://twitter.com/home", "contains");
    assert!(matches_url(&r, "https://twitter.com/home"));
    assert!(!matches_url(&r, "https://twitter.com/explore"));
    assert!(!matches_url(&r, "https://twitter.com/"));
}

#[test]
fn url_rule_with_scheme_exact_blocks_http_request() {
    // The fallback should also apply for exact match — the user typed
    // a URL-with-scheme; they're describing a domain.
    let r = rule("url", "https://www.bilibili.com/", "exact");
    assert!(matches_url(&r, "http://www.bilibili.com/"));
    assert!(!matches_url(&r, "https://other.com/"));
}

#[test]
fn url_rule_with_scheme_regex_does_not_apply_host_fallback() {
    // Regex is precise: a regex explicitly anchored to https:// must NOT
    // silently expand to cover http:// — that would contradict the
    // user's explicit pattern. The fallback only applies to contains/exact.
    let r = rule("url", r"^https://www\.bilibili\.com/?$", "regex");
    assert!(matches_url(&r, "https://www.bilibili.com/"));
    assert!(!matches_url(&r, "http://www.bilibili.com/"));
}

#[test]
fn url_rule_host_fallback_is_case_insensitive() {
    let r = rule("url", "https://WWW.Bilibili.com/", "contains");
    assert!(matches_url(&r, "http://www.bilibili.com/some/path"));
}

#[test]
fn url_rule_host_fallback_strips_port() {
    let r = rule("url", "https://bilibili.com/", "contains");
    assert!(matches_url(&r, "http://bilibili.com:8080/"));
}
