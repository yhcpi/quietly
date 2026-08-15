use quietly_lib::engine::guard::{extract_exe_name, matches_active_rule, check_foreground_for_rules};
use quietly_lib::storage::block_rule::BlockRule;

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
fn extract_exe_name_from_full_path() {
    assert_eq!(
        extract_exe_name("C:\\Windows\\System32\\notepad.exe"),
        Some("notepad.exe".to_string())
    );
}

#[test]
fn extract_exe_name_from_just_name() {
    assert_eq!(extract_exe_name("notepad.exe"), Some("notepad.exe".to_string()));
}

#[test]
fn extract_exe_name_from_unix_path() {
    assert_eq!(extract_exe_name("/usr/bin/ls"), Some("ls".to_string()));
}

#[test]
fn extract_exe_name_returns_none_for_empty() {
    assert_eq!(extract_exe_name(""), None);
}

#[test]
fn matches_active_rule_discord_blocked() {
    let rules = vec![
        rule("app_path", "discord", "contains", true),
        rule("app_path", "slack", "contains", false),
    ];
    assert!(matches_active_rule(&rules, "C:\\Users\\me\\AppData\\Local\\Discord\\Update.exe"));
    assert!(!matches_active_rule(&rules, "C:\\Program Files\\Slack\\slack.exe"));
}

#[test]
fn matches_active_rule_ignores_url_kinds() {
    let rules = vec![rule("url", "discord", "contains", true)];
    assert!(!matches_active_rule(&rules, "C:\\Users\\me\\Discord\\Update.exe"));
}

#[test]
fn matches_active_rule_ignores_inactive_rules() {
    let rules = vec![rule("app_path", "discord", "contains", false)];
    assert!(!matches_active_rule(&rules, "C:\\Users\\me\\Discord\\Update.exe"));
}

#[test]
fn matches_active_rule_no_rules() {
    assert!(!matches_active_rule(&[], "C:\\Discord\\Update.exe"));
}

#[test]
fn check_foreground_for_rules_returns_match() {
    let rules = vec![rule("app_path", "discord", "contains", true)];
    let result = check_foreground_for_rules("C:\\Users\\me\\Discord\\Update.exe", &rules);
    assert_eq!(result, Some("C:\\Users\\me\\Discord\\Update.exe".to_string()));
}

#[test]
fn check_foreground_for_rules_returns_none_for_no_match() {
    let rules = vec![rule("app_path", "discord", "contains", true)];
    let result = check_foreground_for_rules("C:\\Program Files\\Slack\\slack.exe", &rules);
    assert_eq!(result, None);
}

#[test]
fn check_foreground_for_rules_returns_none_for_no_rules() {
    let result = check_foreground_for_rules("C:\\Discord\\Update.exe", &[]);
    assert_eq!(result, None);
}
