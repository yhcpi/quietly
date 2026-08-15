use quietly_lib::engine::timer::*;

#[test]
fn next_phase_default_cycle_of_4() {
    let cfg = TimerConfig::default();
    // Focus #1 completes (cycle=0 -> next cycle=1)
    assert_eq!(next_phase(Phase::Focus, 0, &cfg), Phase::ShortBreak);
    // After ShortBreak we go back to Focus
    assert_eq!(next_phase(Phase::ShortBreak, 1, &cfg), Phase::Focus);
    // Focus #2, #3 complete (cycle=1, 2)
    assert_eq!(next_phase(Phase::Focus, 1, &cfg), Phase::ShortBreak);
    assert_eq!(next_phase(Phase::Focus, 2, &cfg), Phase::ShortBreak);
    // Focus #4 completes (cycle=3, next would be 4 = cycles_before_long)
    assert_eq!(next_phase(Phase::Focus, 3, &cfg), Phase::LongBreak);
    // After LongBreak we restart
    assert_eq!(next_phase(Phase::LongBreak, 4, &cfg), Phase::Focus);
}

#[test]
fn next_phase_cycles_before_long_2() {
    let cfg = TimerConfig { focus_min: 25, short_break_min: 5, long_break_min: 15, cycles_before_long: 2 };
    assert_eq!(next_phase(Phase::Focus, 0, &cfg), Phase::ShortBreak);
    assert_eq!(next_phase(Phase::Focus, 1, &cfg), Phase::LongBreak);
}

#[test]
fn new_engine_initial_state() {
    let cfg = TimerConfig::default();
    let e = TimerEngine::new(cfg);
    let s = e.state();
    assert_eq!(s.phase, Phase::Focus);
    assert_eq!(s.status, TimerStatus::Running);
    assert_eq!(s.total_seconds, 25 * 60);
    assert_eq!(s.remaining_seconds, 25 * 60);
    assert_eq!(s.cycle, 0);
}

#[test]
fn tick_once_decrements_when_running() {
    let cfg = TimerConfig { focus_min: 1, short_break_min: 1, long_break_min: 1, cycles_before_long: 4 };
    let e = TimerEngine::new(cfg);
    let before = e.state().remaining_seconds;
    assert_eq!(e.tick_once(), None);
    assert_eq!(e.state().remaining_seconds, before - 1);
}

#[test]
fn tick_once_transitions_to_short_break() {
    let cfg = TimerConfig { focus_min: 1, short_break_min: 1, long_break_min: 1, cycles_before_long: 4 };
    let e = TimerEngine::new(cfg);
    for _ in 0..60 {
        e.tick_once();
    }
    let s = e.state();
    assert_eq!(s.phase, Phase::ShortBreak);
    assert_eq!(s.cycle, 1);
    assert_eq!(s.remaining_seconds, 60);
    assert_eq!(s.total_seconds, 60);
}

#[test]
fn tick_once_transitions_to_long_break_after_4_focus() {
    let cfg = TimerConfig { focus_min: 1, short_break_min: 1, long_break_min: 1, cycles_before_long: 4 };
    let e = TimerEngine::new(cfg);
    // 4 focus phases x 60 seconds + 3 short breaks x 60 seconds = 240 + 180 = 420 ticks
    for _ in 0..420 {
        e.tick_once();
    }
    let s = e.state();
    assert_eq!(s.phase, Phase::LongBreak);
    assert_eq!(s.cycle, 4);
}

#[test]
fn pause_blocks_tick() {
    let cfg = TimerConfig::default();
    let e = TimerEngine::new(cfg);
    e.pause();
    assert_eq!(e.state().status, TimerStatus::Paused);
    let before = e.state().remaining_seconds;
    assert_eq!(e.tick_once(), None);  // paused, no decrement
    assert_eq!(e.state().remaining_seconds, before);
    e.resume();
    assert_eq!(e.state().status, TimerStatus::Running);
    e.tick_once();
    assert_eq!(e.state().remaining_seconds, before - 1);
}

#[test]
fn cancel_marks_completed() {
    let cfg = TimerConfig::default();
    let e = TimerEngine::new(cfg);
    e.cancel();
    assert_eq!(e.state().status, TimerStatus::Completed);
    // cancel also blocks further ticks
    let before = e.state().remaining_seconds;
    assert_eq!(e.tick_once(), None);
    assert_eq!(e.state().remaining_seconds, before);
}

#[test]
fn phase_duration_seconds_returns_correct_durations() {
    let cfg = TimerConfig::default();
    assert_eq!(phase_duration_seconds(Phase::Focus, &cfg), 25 * 60);
    assert_eq!(phase_duration_seconds(Phase::ShortBreak, &cfg), 5 * 60);
    assert_eq!(phase_duration_seconds(Phase::LongBreak, &cfg), 15 * 60);
}
