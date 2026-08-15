use quietly_lib::engine::schedule::*;
use chrono::{Local, Timelike, Datelike};

#[test]
fn parse_timer_config() {
    let json = r#"{"kind":"Timer","duration_min":25,"short_break_min":5,"long_break_min":15,"cycles_before_long":4}"#;
    let cfg = parse(json).expect("parse");
    match cfg {
        ScheduleConfig::Timer(t) => {
            assert_eq!(t.duration_min, 25);
            assert_eq!(t.short_break_min, 5);
            assert_eq!(t.long_break_min, 15);
            assert_eq!(t.cycles_before_long, 4);
        }
        _ => panic!("expected Timer"),
    }
}

#[test]
fn parse_weekly_config() {
    let json = r#"{"kind":"Weekly","days":[1,2,3,4,5],"ranges":[{"start":"09:00","end":"12:00"}]}"#;
    let cfg = parse(json).expect("parse");
    match cfg {
        ScheduleConfig::Weekly(w) => {
            assert_eq!(w.days, vec![1, 2, 3, 4, 5]);
            assert_eq!(w.ranges.len(), 1);
            assert_eq!(w.ranges[0].start, "09:00");
            assert_eq!(w.ranges[0].end, "12:00");
        }
        _ => panic!("expected Weekly"),
    }
}

#[test]
fn parse_invalid_json_returns_err() {
    assert!(parse("not json").is_err());
}

#[test]
fn parse_unknown_kind_returns_err() {
    let json = r#"{"kind":"Monthly","days":[],"ranges":[]}"#;
    assert!(parse(json).is_err());
}

#[test]
fn is_active_now_timer_always_true() {
    let cfg = ScheduleConfig::Timer(TimerScheduleConfig::default());
    let now = Local::now();
    assert!(is_active_now(&cfg, now));
}

#[test]
fn is_active_now_weekly_today_matches() {
    let now = Local::now();
    let weekday = now.weekday().num_days_from_monday() + 1;
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![weekday],
        ranges: vec![TimeRange { start: "00:00".to_string(), end: "23:59".to_string() }],
    });
    assert!(is_active_now(&cfg, now), "expected today to be active at the all-day range");
}

#[test]
fn is_active_now_weekly_no_day_matches() {
    let now = Local::now();
    let weekday = now.weekday().num_days_from_monday() + 1;
    let other_day = if weekday == 1 { 2 } else { 1 };
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![other_day],
        ranges: vec![TimeRange { start: "00:00".to_string(), end: "23:59".to_string() }],
    });
    assert!(!is_active_now(&cfg, now), "expected today to be outside the configured day");
}

#[test]
fn is_active_now_weekly_outside_range() {
    let now = Local::now();
    let weekday = now.weekday().num_days_from_monday() + 1;
    let (start_h, end_h) = if now.hour() >= 12 { (0, 1) } else { (22, 23) };
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![weekday],
        ranges: vec![TimeRange {
            start: format!("{:02}:00", start_h),
            end: format!("{:02}:00", end_h),
        }],
    });
    assert!(!is_active_now(&cfg, now), "expected current time to be outside the configured range");
}

#[test]
fn is_active_now_weekly_at_range_start_inclusive() {
    // Build a "now" at exactly 09:00:00 in the current local day.
    let mut now = Local::now();
    now = now.with_hour(9).unwrap().with_minute(0).unwrap().with_second(0).unwrap();
    let weekday = now.weekday().num_days_from_monday() + 1;
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![weekday],
        ranges: vec![TimeRange { start: "09:00".to_string(), end: "10:00".to_string() }],
    });
    assert!(is_active_now(&cfg, now), "09:00:00 should be inside 09:00-10:00 (inclusive start)");
}

#[test]
fn is_active_now_weekly_at_range_end_exclusive() {
    let mut now = Local::now();
    now = now.with_hour(10).unwrap().with_minute(0).unwrap().with_second(0).unwrap();
    let weekday = now.weekday().num_days_from_monday() + 1;
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![weekday],
        ranges: vec![TimeRange { start: "09:00".to_string(), end: "10:00".to_string() }],
    });
    assert!(!is_active_now(&cfg, now), "10:00:00 should be outside 09:00-10:00 (exclusive end)");
}

#[test]
fn is_active_now_weekly_multiple_ranges() {
    let weekday = Local::now().weekday().num_days_from_monday() + 1;
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![weekday],
        ranges: vec![
            TimeRange { start: "00:00".to_string(), end: "01:00".to_string() },
            TimeRange { start: "12:00".to_string(), end: "23:59".to_string() },
        ],
    });
    if Local::now().hour() >= 12 {
        assert!(is_active_now(&cfg, Local::now()));
    } else {
        assert!(!is_active_now(&cfg, Local::now()));
    }
}

#[test]
fn is_active_now_weekly_invalid_time_string_falls_back_to_allday() {
    let weekday = Local::now().weekday().num_days_from_monday() + 1;
    let cfg = ScheduleConfig::Weekly(WeeklyScheduleConfig {
        days: vec![weekday],
        ranges: vec![TimeRange { start: "bad".to_string(), end: "also bad".to_string() }],
    });
    // leniency: invalid HH:MM defaults to 00:00 / 23:59:59, so any time matches
    assert!(is_active_now(&cfg, Local::now()));
}
