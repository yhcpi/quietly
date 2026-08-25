import { useEffect, useState } from "react";
import * as ipc from "../ipc";
import "./FirstRunGuide.css";

// Shown once on the first launch after install. Three short steps match
// the "minimum useful session" the user needs to run end-to-end: a rule
// to block, a duration, and a start button. Dismissed forever after —
// the Rust side stores a `first_run_acked` marker file in app_data_dir
// so it survives app restarts.

const STEPS: ReadonlyArray<{ title: string; body: string }> = [
  {
    title: "Add a block rule",
    body: "Open the Rules tab and add a domain like reddit.com. Quietly routes blocked sites through the local proxy during a Focus phase.",
  },
  {
    title: "Set focus duration",
    body: "Back on Timer, pick how long the Focus phase should run. The classic Pomodoro is 25 minutes.",
  },
  {
    title: "Click Start",
    body: "Auto-block is on by default, so the proxy shield comes up the moment the session begins. Quit via the tray icon to stop.",
  },
];

export default function FirstRunGuide() {
  const [show, setShow] = useState(false);

  useEffect(() => {
    let cancelled = false;
    ipc.needsFirstRunAck()
      .then((needs) => { if (!cancelled) setShow(needs); })
      .catch(() => { /* non-fatal: just leave the guide hidden */ });
    return () => { cancelled = true; };
  }, []);

  if (!show) return null;

  async function dismiss() {
    try {
      await ipc.ackFirstRun();
    } catch {
      // Ack failed (e.g. transient filesystem error). Hide the guide
      // anyway — re-showing on the next launch is cheap, and a stuck
      // modal is worse than a slightly early dismissal.
    } finally {
      setShow(false);
    }
  }

  return (
    <div className="first-run-guide" role="dialog" aria-labelledby="first-run-title">
      <h2 id="first-run-title">Welcome to Quietly</h2>
      <ol className="first-run-steps">
        {STEPS.map((s, i) => (
          <li key={i}>
            <span className="first-run-step-num">{i + 1}</span>
            <div>
              <strong>{s.title}</strong>
              <p>{s.body}</p>
            </div>
          </li>
        ))}
      </ol>
      <button className="btn-primary first-run-dismiss" onClick={dismiss}>
        Got it
      </button>
    </div>
  );
}