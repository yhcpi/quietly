use crate::storage::block_rule::BlockRule;

pub fn matches_url(rule: &BlockRule, url: &str) -> bool {
    if rule.kind != "url" && rule.kind != "domain" {
        return false;
    }
    let target = if rule.kind == "domain" {
        extract_host(url)
    } else {
        url.to_string()
    };
    match_str(&target, &rule.pattern, &rule.match_type)
}

pub fn matches_app(rule: &BlockRule, exe_path: &str) -> bool {
    if rule.kind != "app_path" {
        return false;
    }
    match_str(exe_path, &rule.pattern, &rule.match_type)
}

fn match_str(haystack: &str, pattern: &str, match_type: &str) -> bool {
    match match_type {
        "exact" => haystack == pattern,
        "contains" => haystack.to_lowercase().contains(&pattern.to_lowercase()),
        "regex" => regex::Regex::new(pattern)
            .map(|r| r.is_match(haystack))
            .unwrap_or(false),
        _ => false,
    }
}

fn extract_host(url: &str) -> String {
    let after_scheme = url.splitn(2, "://").nth(1).unwrap_or(url);
    let host_end = after_scheme
        .find(|c: char| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_scheme.len());
    let host = &after_scheme[..host_end];
    let host_no_port = host.split(':').next().unwrap_or(host);
    host_no_port.to_string()
}
