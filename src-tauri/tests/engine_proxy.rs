use quietly_lib::storage::block_rule::BlockRule;
use quietly_lib::engine::proxy::{should_block_url, should_block_host, parse_connect_host, parse_request_url};

fn rule(kind: &str, pattern: &str, match_type: &str, active: bool) -> BlockRule {
    BlockRule {
        id: Some(1),
        kind: kind.into(),
        pattern: pattern.into(),
        match_type: match_type.into(),
        note: "".into(),
        active,
        created_at: 0,
    }
}

#[test]
fn should_block_url_matches_active_rule() {
    let rules = vec![rule("url", "twitter.com", "contains", true)];
    assert!(should_block_url("https://twitter.com/foo", &rules));
    assert!(!should_block_url("https://facebook.com", &rules));
}

#[test]
fn should_block_url_ignores_inactive_rules() {
    let rules = vec![rule("url", "twitter.com", "contains", false)];
    assert!(!should_block_url("https://twitter.com/foo", &rules));
}

#[test]
fn should_block_url_no_rules() {
    assert!(!should_block_url("https://twitter.com", &[]));
}

#[test]
fn should_block_host_with_domain_rule() {
    let rules = vec![rule("domain", "twitter.com", "exact", true)];
    assert!(should_block_host("twitter.com:443", &rules));
    assert!(!should_block_host("facebook.com:443", &rules));
}

#[test]
fn should_block_host_strips_port() {
    let rules = vec![rule("domain", "twitter.com", "exact", true)];
    assert!(should_block_host("twitter.com:443", &rules));
    assert!(should_block_host("twitter.com", &rules));
}

#[test]
fn should_block_host_ignores_inactive_rules() {
    let rules = vec![rule("domain", "twitter.com", "exact", false)];
    assert!(!should_block_host("twitter.com", &rules));
}

#[test]
fn parse_connect_host_extracts_target() {
    assert_eq!(parse_connect_host("CONNECT twitter.com:443 HTTP/1.1"), Some("twitter.com:443".to_string()));
    assert_eq!(parse_connect_host("CONNECT example.com:80 HTTP/1.0"), Some("example.com:80".to_string()));
}

#[test]
fn parse_connect_host_rejects_non_connect() {
    assert_eq!(parse_connect_host("GET / HTTP/1.1"), None);
    assert_eq!(parse_connect_host("POST http://x.com/ HTTP/1.1"), None);
    assert_eq!(parse_connect_host(""), None);
}

#[test]
fn parse_request_url_extracts_absolute_url() {
    assert_eq!(
        parse_request_url("GET http://twitter.com/foo HTTP/1.1"),
        Some("http://twitter.com/foo".to_string())
    );
    assert_eq!(
        parse_request_url("POST https://api.example.com/v1/x HTTP/1.1"),
        Some("https://api.example.com/v1/x".to_string())
    );
}

#[test]
fn parse_request_url_rejects_relative() {
    assert_eq!(parse_request_url("GET /foo HTTP/1.1"), None);
    assert_eq!(parse_request_url("GET / HTTP/1.1"), None);
}

#[test]
fn parse_request_url_rejects_unknown_method() {
    assert_eq!(parse_request_url("BREW /foo HTTP/1.1"), None);
}