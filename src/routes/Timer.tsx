import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as ipc from "../ipc";
import type { TimerState } from "../types";
import { formatTime, formatPhase } from "../lib/format";
import { humanizeError } from "../lib/error";
import "./Timer.css";

const DEFAULT_DURATION_MIN = 25;
const MIN_DURATION_MIN = 1;
const MAX_DURATION_MIN = 120;

export default function Timer() {
  const [state, setState] = useState<TimerState | null>(null);
  const [durationMin, setDurationMin] = useState<number>(DEFAULT_DURATION_MIN);
  const [error, setError] = useState<string | null>(null);
  // Session options, chosen in the idle view and passed to start_focus.
  const [lockSession, setLockSession] = useState(false);
  const [autoBlock, setAutoBlock] = useState(true);
  // Whether the *running* session is locked. The Rust TimerState does not
  // carry `locked`, so this comes from is_session_locked (on mount, to
  // survive a route change) and from the checkbox at start time.
  const [locked, setLocked] = useState(false);
  // Disable Start while a focus session is being brought up. Without this
  // the button stays clickable for the few hundred ms between click and
  // the first timer-tick, and a double-click fires handleStart twice —
  // the second call hits start_proxy_inner's "proxy already running"
  // check and surfaces as a confusing error.
  const [starting, setStarting] = useState(false);
  // React state is async — a rapid double-click can read `starting` as
  // false on the second event before the first event's setState commits.
  // Use a ref for the synchronous guard; setState remains for the visual
  // disabled state.
  const isStartingRef = useRef(false);

  useEffect(() => {
    const promise = listen<TimerState>("timer-tick", (e) => {
      setState(e.payload);
    });
    // Navigating away and back remounts this component; re-read the lock
    // flag so the Cancel button doesn't reappear on a locked session.
    ipc.isSessionLocked().then(setLocked).catch(() => {});
    return () => {
      promise.then((unlisten) => unlisten());
    };
  }, []);

  async function withErrorHandling(fn: () => Promise<unknown>) {
    try {
      setError(null);
      await fn();
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  async function handleStart() {
    if (isStartingRef.current) return;
    isStartingRef.current = true;
    setStarting(true);
    setLocked(lockSession);
    try {
      await withErrorHandling(async () => {
        // start_focus owns the shield: with autoBlock it brings proxy +
        // guard up synchronously before returning, so the UI must not
        // start them itself.
        await ipc.startFocus(durationMin, lockSession, autoBlock);
      });
    } finally {
      isStartingRef.current = false;
      setStarting(false);
      ipc.isSessionLocked().then(setLocked).catch(() => {});
    }
  }
  function handlePause() {
    return withErrorHandling(() => ipc.pauseFocus());
  }
  function handleResume() {
    return withErrorHandling(() => ipc.resumeFocus());
  }
  async function handleCancel() {
    return withErrorHandling(async () => {
      // cancel_focus tears down guard + proxy on the Rust side. A locked
      // session rejects here — let the error surface instead of swallowing it.
      await ipc.cancelFocus();
      setLocked(false);
      // Stopping the timer engine on the Rust side stops future
      // timer-tick events but does NOT clear the React state we already
      // received. Without this the UI would stay on the ACTIVE view
      // until the user navigates away and back (Timer remount re-runs
      // useState(null)). Reset locally so the flip happens immediately.
      setState(null);
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
            <label className="timer-option">
              <input
                type="checkbox"
                checked={autoBlock}
                onChange={(e) => setAutoBlock(e.target.checked)}
              />
              <span>
                Auto-block on Focus
                <em>Blocks sites and apps while the session runs.</em>
              </span>
            </label>
            <label className="timer-option">
              <input
                type="checkbox"
                checked={lockSession}
                onChange={(e) => setLockSession(e.target.checked)}
              />
              <span>
                Lock this session
                <em>Cancel is disabled until the session ends.</em>
              </span>
            </label>
            <button className="btn-primary" onClick={handleStart} disabled={starting}>
              {starting ? "Starting..." : "Start"}
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
              {locked ? (
                <button
                  className="btn-ghost"
                  disabled
                  title="This session can't be cancelled."
                >
                  Locked
                </button>
              ) : (
                <button className="btn-ghost" onClick={handleCancel}>
                  Cancel
                </button>
              )}
            </div>
          </div>
        )}
        {error && <div className="timer-error">{error}</div>}
      </div>
    </div>
  );
}