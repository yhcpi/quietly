import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as ipc from "../ipc";
import type { TimerState } from "../types";
import { formatTime, formatPhase } from "../lib/format";
import "./Timer.css";

const DEFAULT_DURATION_MIN = 25;
const MIN_DURATION_MIN = 1;
const MAX_DURATION_MIN = 120;

// Local proxy port — must match the port Windows system proxy is pointed
// at during a focus session. 47820 is the MVP default; future releases may
// surface it via Settings.
const PROXY_PORT = 47820;

export default function Timer() {
  const [state, setState] = useState<TimerState | null>(null);
  const [durationMin, setDurationMin] = useState<number>(DEFAULT_DURATION_MIN);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const promise = listen<TimerState>("timer-tick", (e) => {
      setState(e.payload);
    });
    return () => {
      promise.then((unlisten) => unlisten());
    };
  }, []);

  async function withErrorHandling(fn: () => Promise<unknown>) {
    try {
      setError(null);
      await fn();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleStart() {
    return withErrorHandling(async () => {
      // Order matters: proxy must be listening before the timer starts so
      // Windows system proxy is already pointing at it. If proxy or guard
      // fail to come up, do not start the timer (focus session without
      // shield is worse than no session).
      await ipc.startProxy(PROXY_PORT);
      await ipc.startGuard();
      await ipc.startFocus(durationMin);
    });
  }
  function handlePause() {
    return withErrorHandling(() => ipc.pauseFocus());
  }
  function handleResume() {
    return withErrorHandling(() => ipc.resumeFocus());
  }
  async function handleCancel() {
    return withErrorHandling(async () => {
      // Stop in reverse: timer first (releases session state), then guard,
      // then proxy (which also restores the original Windows system proxy).
      // Best-effort: if one step fails, still try the others so we don't
      // leak a stuck shield.
      try { await ipc.cancelFocus(); } catch { /* no active timer */ }
      try { await ipc.stopGuard(); } catch { /* guard not running */ }
      try { await ipc.stopProxy(); } catch { /* proxy not running */ }
    });
  }

  const isRunning = state?.status === "Running";
  const isPaused = state?.status === "Paused";
  const isIdle = !state;

  return (
    <div className="timer-page">
      <div className="timer-card">
        {isIdle ? (
          <div className="timer-idle">
            <h1>Ready to focus?</h1>
            <label className="timer-duration">
              <span>Duration (minutes)</span>
              <input
                type="number"
                min={MIN_DURATION_MIN}
                max={MAX_DURATION_MIN}
                value={durationMin}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  setDurationMin(Number.isFinite(n) ? n : DEFAULT_DURATION_MIN);
                }}
              />
            </label>
            <button className="btn-primary" onClick={handleStart}>
              Start
            </button>
          </div>
        ) : (
          <div className="timer-active">
            <div className="timer-phase">{formatPhase(state.phase)}</div>
            <div className="timer-clock">{formatTime(state.remaining_seconds)}</div>
            <div className="timer-cycle">Cycle {state.cycle + 1}</div>
            <div className="timer-status">{state.status}</div>
            <div className="timer-actions">
              {isRunning && (
                <button onClick={handlePause}>Pause</button>
              )}
              {isPaused && (
                <button onClick={handleResume}>Resume</button>
              )}
              <button className="btn-ghost" onClick={handleCancel}>
                Cancel
              </button>
            </div>
          </div>
        )}
        {error && <div className="timer-error">{error}</div>}
      </div>
    </div>
  );
}