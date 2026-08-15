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

// ─── block page response ────────────────────────────────────────────

use quietly_lib::engine::proxy::build_block_response;

#[test]
fn block_response_starts_with_403_status_line() {
    let bytes = build_block_response();
    let text = std::str::from_utf8(&bytes).expect("response must be utf-8");
    assert!(
        text.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "response must start with 403 status line, got: {:?}",
        &text[..text.find('\n').map(|i| i + 1).unwrap_or(text.len()).min(60)]
    );
}

#[test]
fn block_response_contains_required_headers() {
    let bytes = build_block_response();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("Content-Type: text/html; charset=utf-8\r\n"));
    assert!(text.contains("Connection: close\r\n"));
}

#[test]
fn block_response_content_length_matches_body() {
    let bytes = build_block_response();
    let text = std::str::from_utf8(&bytes).unwrap();
    // Find header/body separator
    let sep = "\r\n\r\n".as_bytes();
    let sep_idx = text
        .find("\r\n\r\n")
        .expect("response must have header/body separator");
    let body_bytes = &bytes[sep_idx + sep.len()..];

    // Parse Content-Length header value
    let header_section = &text[..sep_idx];
    let content_length_line = header_section
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .expect("response must have Content-Length header");
    let n: usize = content_length_line
        .split(':')
        .nth(1)
        .expect("Content-Length value present")
        .trim()
        .parse()
        .expect("Content-Length must be a non-negative integer");
    assert_eq!(n, body_bytes.len(), "Content-Length must match body byte length");
}

#[test]
fn block_response_body_branded() {
    let bytes = build_block_response();
    let text = std::str::from_utf8(&bytes).unwrap();
    let sep = "\r\n\r\n";
    let sep_idx = text.find(sep).unwrap();
    let body = &text[sep_idx + sep.len()..];
    assert!(body.contains("Quietly"), "body must carry the Quietly brand");
    assert!(body.contains("This site is blocked"), "body must carry the block-page headline");
    assert!(body.contains("#3D5AFE"), "body must carry the indigo brand color");
    assert!(
        !body.contains("FocusLeaf") && !body.contains("Leaf") && !body.contains("leaf"),
        "body must NOT contain FocusLeaf or any leaf imagery"
    );
}