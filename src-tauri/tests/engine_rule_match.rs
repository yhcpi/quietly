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
