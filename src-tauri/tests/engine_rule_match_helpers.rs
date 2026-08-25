// Edge cases for the helpers inside `engine::rule_match` that are not
// directly reachable from `matches_url` / `matches_app` / `matches_title`.
// The `match_str` and `extract_host` helpers themselves are private to the
// crate; this file exercises their observable behavior through the public
// functions in the spots the integration tests don't cover.
use quietly_lib::engine::rule_match::matches_url;
use quietly_lib::storage::block_rule::BlockRule;

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

// `extract_host` falls back to the input when there is no `://` separator.
// Combined with the host-vs-host fallback in `matches_url`, a scheme-less
// rule pattern like `www.bilibili.com/` should still match requests.
#[test]
fn scheme_less_pattern_is_treated_as_pure_host() {
    let r = rule("url", "www.bilibili.com/", "contains");
    assert!(matches_url(&r, "http://www.bilibili.com/some/path"));
    assert!(matches_url(&r, "https://www.bilibili.com"));
}

#[test]
fn scheme_less_pattern_with_port_in_rule() {
    let r = rule("url", "bilibili.com:8080/", "contains");
    // The rule's host (with port stripped) should still match a request
    // against the bare host.
    assert!(matches_url(&r, "http://bilibili.com/"));
}

// `match_str` is case-insensitive for `contains`. Reach this through a
// kind=url pattern that has no scheme so the host-fallback path doesn't
// accidentally absorb the lowercase comparison.
#[test]
fn contains_match_is_case_insensitive_for_explicit_url_pattern() {
    let r = rule("url", "Twitter.COM/foo", "contains");
    assert!(matches_url(&r, "https://twitter.com/foo"));
    assert!(matches_url(&r, "https://TWITTER.com/foo"));
}

// `extract_host` strips the port from both rule and request when applying
// the host-fallback. A bare-host rule with no scheme should match a
// request against the host on any port.
#[test]
fn scheme_less_host_matches_any_port() {
    let r = rule("url", "bilibili.com", "contains");
    assert!(matches_url(&r, "http://bilibili.com:8443/some/path"));
}