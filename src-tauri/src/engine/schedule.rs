use chrono::{DateTime, Datelike, Local, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimerScheduleConfig {
    pub duration_min: u32,
    pub short_break_min: u32,
    pub long_break_min: u32,
    pub cycles_before_long: u32,
}

impl Default for TimerScheduleConfig {
    fn default() -> Self {
        Self {
            duration_min: 25,
            short_break_min: 5,
            long_break_min: 15,
            cycles_before_long: 4,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimeRange {
    pub start: String,
    pub end: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeeklyScheduleConfig {
    pub days: Vec<u32>,
    pub ranges: Vec<TimeRange>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "kind")]
pub enum ScheduleConfig {
    Timer(TimerScheduleConfig),
    Weekly(WeeklyScheduleConfig),
}

pub fn parse(json: &str) -> Result<ScheduleConfig, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn is_active_now(cfg: &ScheduleConfig, now: DateTime<Local>) -> bool {
    match cfg {
        ScheduleConfig::Timer(_) => true,
        ScheduleConfig::Weekly(w) => is_active_weekly(w, now),
    }
}

fn is_active_weekly(cfg: &WeeklyScheduleConfig, now: DateTime<Local>) -> bool {
    let weekday = now.weekday().num_days_from_monday() + 1;
    if !cfg.days.contains(&weekday) {
        return false;
    }
    let current_seconds = now.time().num_seconds_from_midnight();
    for range in &cfg.ranges {
        let start = parse_hh_mm(&range.start).unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let end = parse_hh_mm(&range.end).unwrap_or_else(|| NaiveTime::from_hms_opt(23, 59, 59).unwrap());
        let start_s = start.num_seconds_from_midnight();
        let end_s = end.num_seconds_from_midnight();
        if current_seconds >= start_s && current_seconds < end_s {
            return true;
        }
    }
    false
}

fn parse_hh_mm(s: &str) -> Option<NaiveTime> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    NaiveTime::from_hms_opt(h, m, 0)
}
