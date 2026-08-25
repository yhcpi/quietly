use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::storage::block_rule::BlockRule;

/// Hard cap on user-supplied regex patterns. A 4 KiB regex on a hot
/// request path is a DoS surface even with caching (compile can stall
/// the calling thread for seconds); reject up front.
const MAX_REGEX_PATTERN: usize = 4 * 1024;

/// Cap on the regex compile cache. Rules are user-supplied and the set
/// is bounded in practice (the rule list is small), but defensively
/// limit growth: once full, evict everything. This keeps the cache
/// bounded while still amortizing recompilation across requests.
const REGEX_CACHE_LIMIT: usize = 64;

type CompiledRegex = Result<regex::Regex, regex::Error>;

fn regex_cache() -> &'static Mutex<HashMap<String, CompiledRegex>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CompiledRegex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn matches_url(rule: &BlockRule, url: &str) -> bool {
    if rule.kind != "url" && rule.kind != "domain" {
        return false;
    }
    let target = if rule.kind == "domain" {
        extract_host(url)
    } else {
        url.to_string()
    };
    if match_str(&target, &rule.pattern, &rule.match_type) {
        return true;
    }
    // Fallback for `kind=url` rules whose pattern is effectively just a
    // domain (e.g. user typed `https://www.bilibili.com/`): also compare
    // the request URL's host against the rule pattern's host. Without
    // this, the literal-substring matcher silently lets `http://...` slip
    // past a rule written with `https://...`, which is the common
    // mismatch when a user copies a URL out of their address bar.
    //
    // Gated on:
    //   - known match_type (unknown match types already fail closed — the
    //     rule is broken, don't rescue it with a fallback that silently
    //     changes semantics).
    //   - contains / exact only — for `regex`, the user wrote a precise
    //     pattern; expanding it would contradict their explicit intent.
    //   - rule pattern has no meaningful path (`https://twitter.com/home`
    //     is path-specific and must NOT grow to cover all of twitter.com).
    if matches!(rule.match_type.as_str(), "contains" | "exact")
        && rule.kind == "url"
        && rule_pattern_is_pure_host(&rule.pattern)
    {
        let rule_host = extract_host(&rule.pattern);
        if !rule_host.is_empty() {
            let req_host = extract_host(url);
            if !req_host.is_empty() && req_host.eq_ignore_ascii_case(&rule_host) {
                return true;
            }
        }
    }
    false
}

pub fn matches_app(rule: &BlockRule, exe_path: &str) -> bool {
    if rule.kind != "app_path" {
        return false;
    }
    match_str(exe_path, &rule.pattern, &rule.match_type)
}

pub fn matches_title(rule: &BlockRule, title: &str) -> bool {
    if rule.kind != "title" {
        return false;
    }
    match_str(title, &rule.pattern, &rule.match_type)
}

pub(crate) fn match_str(haystack: &str, pattern: &str, match_type: &str) -> bool {
    match match_type {
        "exact" => haystack == pattern,
        "contains" => haystack.to_lowercase().contains(&pattern.to_lowercase()),
        "regex" => {
            if pattern.len() > MAX_REGEX_PATTERN {
                return false;
            }
            let mut cache = regex_cache().lock().expect("regex cache poisoned");
            if cache.len() >= REGEX_CACHE_LIMIT && !cache.contains_key(pattern) {
                // Cache full and this pattern isn't present — flush to
                // make room. Cheap eviction; the rule list is small so
                // the recompile cost is bounded.
                cache.clear();
            }
            let entry = cache
                .entry(pattern.to_string())
                .or_insert_with(|| regex::Regex::new(pattern));
            entry.as_ref().map(|r| r.is_match(haystack)).unwrap_or(false)
        }
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

/// True if `pattern` is `scheme://host[:port]` with at most a single
/// trailing `/` — i.e. it describes a host, not a specific URL path.
/// Used to decide whether to apply the host-vs-host fallback in
/// `matches_url` for `kind=url` rules.
fn rule_pattern_is_pure_host(pattern: &str) -> bool {
    let after_scheme = pattern.splitn(2, "://").nth(1).unwrap_or(pattern);
    match after_scheme.find('/') {
        None => true,
        Some(idx) => after_scheme[idx + 1..].is_empty(),
    }
}
