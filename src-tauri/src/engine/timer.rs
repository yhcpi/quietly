use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use serde::{Serialize, Deserialize};
use tauri::{AppHandle, Emitter};
use tokio::time::interval;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimerConfig {
    pub focus_min: u32,
    pub short_break_min: u32,
    pub long_break_min: u32,
    pub cycles_before_long: u32,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            focus_min: 25,
            short_break_min: 5,
            long_break_min: 15,
            cycles_before_long: 4,
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub enum TimerStatus {
    Running,
    Paused,
    Completed,
}

#[derive(Serialize, Clone, Debug)]
pub struct TimerState {
    pub status: TimerStatus,
    pub phase: Phase,
    pub remaining_seconds: u32,
    pub total_seconds: u32,
    pub cycle: u32,
}

pub fn phase_duration_seconds(phase: Phase, cfg: &TimerConfig) -> u32 {
    match phase {
        Phase::Focus => cfg.focus_min * 60,
        Phase::ShortBreak => cfg.short_break_min * 60,
        Phase::LongBreak => cfg.long_break_min * 60,
    }
}

pub fn next_phase(current: Phase, cycle: u32, cfg: &TimerConfig) -> Phase {
    match current {
        Phase::Focus => {
            if cycle + 1 >= cfg.cycles_before_long {
                Phase::LongBreak
            } else {
                Phase::ShortBreak
            }
        }
        Phase::ShortBreak => Phase::Focus,
        Phase::LongBreak => Phase::Focus,
    }
}

pub struct TimerEngine {
    cfg: TimerConfig,
    state: Arc<Mutex<TimerState>>,
    pause_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
}

impl TimerEngine {
    pub fn new(cfg: TimerConfig) -> Self {
        let total = phase_duration_seconds(Phase::Focus, &cfg);
        Self {
            cfg,
            state: Arc::new(Mutex::new(TimerState {
                status: TimerStatus::Running,
                phase: Phase::Focus,
                remaining_seconds: total,
                total_seconds: total,
                cycle: 0,
            })),
            pause_flag: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn state(&self) -> TimerState {
        self.state.lock().unwrap().clone()
    }

    pub fn pause(&self) {
        let mut s = self.state.lock().unwrap();
        if s.status == TimerStatus::Running {
            s.status = TimerStatus::Paused;
            self.pause_flag.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume(&self) {
        let mut s = self.state.lock().unwrap();
        if s.status == TimerStatus::Paused {
            s.status = TimerStatus::Running;
            self.pause_flag.store(false, Ordering::SeqCst);
        }
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        let mut s = self.state.lock().unwrap();
        s.status = TimerStatus::Completed;
    }

    /// Decrement timer by 1 second. If the phase just drained, advance to next phase.
    /// Returns Some(old_phase) when a phase transition occurred (caller's cue to emit/log).
    pub fn tick_once(&self) -> Option<Phase> {
        let mut s = self.state.lock().unwrap();
        if s.status != TimerStatus::Running {
            return None;
        }
        if s.remaining_seconds > 0 {
            s.remaining_seconds -= 1;
        }
        if s.remaining_seconds == 0 {
            let old_phase = s.phase;
            let new_phase = next_phase(old_phase, s.cycle, &self.cfg);
            if old_phase == Phase::Focus {
                if new_phase == Phase::LongBreak {
                    s.cycle = self.cfg.cycles_before_long;
                } else {
                    s.cycle += 1;
                }
            } else if old_phase == Phase::LongBreak {
                s.cycle = 0;
            }
            s.phase = new_phase;
            s.total_seconds = phase_duration_seconds(new_phase, &self.cfg);
            s.remaining_seconds = s.total_seconds;
            s.status = TimerStatus::Running;
            return Some(old_phase);
        }
        None
    }

    pub async fn run_loop(
        self: Arc<Self>,
        on_tick: impl Fn(TimerState) + Send + 'static,
        on_phase_change: impl Fn(Phase, Phase) + Send + 'static,
    ) {
        let mut ticker = interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            if self.cancel_flag.load(Ordering::SeqCst) {
                break;
            }
            while self.pause_flag.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if self.cancel_flag.load(Ordering::SeqCst) {
                    break;
                }
            }
            if self.cancel_flag.load(Ordering::SeqCst) {
                break;
            }
            let phase_change = self.tick_once();
            on_tick(self.state());
            if let Some(old) = phase_change {
                on_phase_change(old, self.state().phase);
            }
        }
    }
}

pub struct TimerHandle {
    engine: Arc<TimerEngine>,
    _task: tokio::task::JoinHandle<()>,
}

impl TimerHandle {
    pub fn start(app: AppHandle, cfg: TimerConfig) -> Self {
        let engine = Arc::new(TimerEngine::new(cfg));
        let engine_for_task = engine.clone();
        let task = tokio::spawn(async move {
            engine_for_task
                .run_loop(
                    {
                        let app = app.clone();
                        move |state| {
                            let _ = app.emit("timer-tick", state);
                        }
                    },
                    {
                        let app = app.clone();
                        move |old, new| {
                            let _ = app.emit("timer-phase-changed", (old, new));
                        }
                    },
                )
                .await;
        });
        Self { engine, _task: task }
    }

    pub fn state(&self) -> TimerState {
        self.engine.state()
    }

    pub fn pause(&self) {
        self.engine.pause();
    }

    pub fn resume(&self) {
        self.engine.resume();
    }

    pub fn cancel(&self) {
        self.engine.cancel();
    }
}
