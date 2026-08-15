import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as ipc from "../ipc";
import type { TimerState } from "../types";
import { formatTime, formatPhase } from "../lib/format";
import "./Timer.css";

const DEFAULT_DURATION_MIN = 25;
const MIN_DURATION_MIN = 1;
const MAX_DURATION_MIN = 120;

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

  function handleStart() {
    return withErrorHandling(() => ipc.startFocus(durationMin));
  }
  function handlePause() {
    return withErrorHandling(() => ipc.pauseFocus());
  }
  function handleResume() {
    return withErrorHandling(() => ipc.resumeFocus());
  }
  function handleCancel() {
    return withErrorHandling(() => ipc.cancelFocus());
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