import { useEffect, useState } from "react";
import * as ipc from "../ipc";
import type { Schedule } from "../types";
import {
  blankSchedule,
  buildTimerConfig,
  buildWeeklyConfig,
  validateSchedule,
  SCHEDULE_KINDS,
  type ScheduleDraft,
} from "../lib/schedule";
import "./Schedules.css";

type EditState =
  | { mode: "new" }
  | { mode: "edit"; schedule: Schedule };

function toDraft(s: Schedule): ScheduleDraft {
  return {
    name: s.name,
    kind: s.kind,
    enabled: s.enabled,
    config_json: s.config_json,
    block_rule_ids: s.block_rule_ids,
  };
}

const DAY_LABELS: Array<{ value: number; label: string }> = [
  { value: 1, label: "Mon" },
  { value: 2, label: "Tue" },
  { value: 3, label: "Wed" },
  { value: 4, label: "Thu" },
  { value: 5, label: "Fri" },
  { value: 6, label: "Sat" },
  { value: 7, label: "Sun" },
];

export default function Schedules() {
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [edit, setEdit] = useState<EditState>({ mode: "new" });
  const [draft, setDraft] = useState<ScheduleDraft>(blankSchedule());
  const [error, setError] = useState<string | null>(null);

  // Weekly-config sub-state (mirrored into config_json on change)
  const [weeklyDays, setWeeklyDays] = useState<number[]>([1, 2, 3, 4, 5]);
  const [weeklyStart, setWeeklyStart] = useState("09:00");
  const [weeklyEnd, setWeeklyEnd] = useState("12:00");

  // Timer-config sub-state
  const [timerDuration, setTimerDuration] = useState(25);

  async function refresh() {
    try {
      setError(null);
      setSchedules(await ipc.listSchedules());
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  function applyKindSpecificDefaults(kind: ScheduleDraft["kind"]) {
    if (kind === "timer") {
      setDraft((d) => ({
        ...d,
        kind,
        config_json: buildTimerConfig({ duration_min: timerDuration }),
      }));
    } else {
      setDraft((d) => ({
        ...d,
        kind,
        config_json: buildWeeklyConfig({
          days: weeklyDays,
          start: weeklyStart,
          end: weeklyEnd,
        }),
      }));
    }
  }

  function startNew() {
    setEdit({ mode: "new" });
    setDraft(blankSchedule());
    setTimerDuration(25);
    setWeeklyDays([1, 2, 3, 4, 5]);
    setWeeklyStart("09:00");
    setWeeklyEnd("12:00");
    setError(null);
  }

  function startEdit(s: Schedule) {
    setEdit({ mode: "edit", schedule: s });
    setDraft(toDraft(s));
    if (s.kind === "timer") {
      try {
        const cfg = JSON.parse(s.config_json);
        if (typeof cfg.duration_min === "number") setTimerDuration(cfg.duration_min);
      } catch {
        // keep defaults
      }
    } else {
      try {
        const cfg = JSON.parse(s.config_json);
        if (Array.isArray(cfg.days)) setWeeklyDays(cfg.days);
        if (Array.isArray(cfg.ranges) && cfg.ranges[0]) {
          if (typeof cfg.ranges[0].start === "string") setWeeklyStart(cfg.ranges[0].start);
          if (typeof cfg.ranges[0].end === "string") setWeeklyEnd(cfg.ranges[0].end);
        }
      } catch {
        // keep defaults
      }
    }
    setError(null);
  }

  function onKindChange(kind: ScheduleDraft["kind"]) {
    setDraft((d) => ({ ...d, kind }));
    applyKindSpecificDefaults(kind);
  }

  function onTimerDurationChange(n: number) {
    setTimerDuration(n);
    if (draft.kind === "timer") {
      setDraft((d) => ({
        ...d,
        config_json: buildTimerConfig({ duration_min: n }),
      }));
    }
  }

  function toggleWeeklyDay(day: number) {
    const next = weeklyDays.includes(day)
      ? weeklyDays.filter((d) => d !== day)
      : [...weeklyDays, day].sort((a, b) => a - b);
    setWeeklyDays(next);
    if (draft.kind === "weekly") {
      setDraft((d) => ({
        ...d,
        config_json: buildWeeklyConfig({
          days: next,
          start: weeklyStart,
          end: weeklyEnd,
        }),
      }));
    }
  }

  function onTimeChange(which: "start" | "end", value: string) {
    if (which === "start") setWeeklyStart(value);
    else setWeeklyEnd(value);
    if (draft.kind === "weekly") {
      setDraft((d) => ({
        ...d,
        config_json: buildWeeklyConfig({
          days: weeklyDays,
          start: which === "start" ? value : weeklyStart,
          end: which === "end" ? value : weeklyEnd,
        }),
      }));
    }
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const err = validateSchedule(draft);
    if (err) {
      setError(err);
      return;
    }
    try {
      setError(null);
      if (edit.mode === "new") {
        await ipc.createSchedule(draft);
      } else {
        await ipc.updateSchedule({ ...edit.schedule, ...draft });
      }
      startNew();
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(s: Schedule) {
    if (s.id == null) return;
    try {
      setError(null);
      await ipc.deleteSchedule(s.id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleToggle(s: Schedule) {
    if (s.id == null) return;
    try {
      setError(null);
      await ipc.toggleSchedule(s.id, !s.enabled);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="schedules-page">
      <header className="schedules-header">
        <h1>Schedules</h1>
        <p className="schedules-sub">
          Plan focus time with timer or weekly schedules.
        </p>
      </header>

      {error && <div className="schedules-error">{error}</div>}

      <section className="schedules-list">
        {schedules.length === 0 ? (
          <p className="schedules-empty">No schedules yet. Add one below.</p>
        ) : (
          <table className="schedules-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Kind</th>
                <th>Config</th>
                <th>Enabled</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {schedules.map((s) => (
                <tr key={s.id} className={s.enabled ? "" : "schedules-row-inactive"}>
                  <td>{s.name}</td>
                  <td>{s.kind}</td>
                  <td className="schedules-config">{s.config_json}</td>
                  <td>
                    <label>
                      <input
                        type="checkbox"
                        checked={s.enabled}
                        onChange={() => handleToggle(s)}
                      />
                    </label>
                  </td>
                  <td className="schedules-actions">
                    <button onClick={() => startEdit(s)}>Edit</button>
                    <button
                      className="btn-danger"
                      onClick={() => handleDelete(s)}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="schedules-form-card">
        <h2>{edit.mode === "new" ? "New schedule" : "Edit schedule"}</h2>
        <form onSubmit={handleSubmit} className="schedules-form">
          <label>
            <span>Name</span>
            <input
              type="text"
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            />
          </label>

          <label>
            <span>Kind</span>
            <select
              value={draft.kind}
              onChange={(e) =>
                onKindChange(e.target.value as ScheduleDraft["kind"])
              }
            >
              {SCHEDULE_KINDS.map((k) => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
          </label>

          {draft.kind === "timer" ? (
            <label>
              <span>Duration (minutes)</span>
              <input
                type="number"
                min={1}
                max={120}
                value={timerDuration}
                onChange={(e) => onTimerDurationChange(parseInt(e.target.value, 10) || 25)}
              />
            </label>
          ) : (
            <div className="weekly-config">
              <span className="weekly-label">Days</span>
              <div className="weekly-days">
                {DAY_LABELS.map((d) => (
                  <label key={d.value} className="weekly-day">
                    <input
                      type="checkbox"
                      checked={weeklyDays.includes(d.value)}
                      onChange={() => toggleWeeklyDay(d.value)}
                    />
                    <span>{d.label}</span>
                  </label>
                ))}
              </div>
              <div className="weekly-times">
                <label>
                  <span>Start</span>
                  <input
                    type="time"
                    value={weeklyStart}
                    onChange={(e) => onTimeChange("start", e.target.value)}
                  />
                </label>
                <label>
                  <span>End</span>
                  <input
                    type="time"
                    value={weeklyEnd}
                    onChange={(e) => onTimeChange("end", e.target.value)}
                  />
                </label>
              </div>
            </div>
          )}

          <label>
            <span>Block rule IDs (comma-separated, optional)</span>
            <input
              type="text"
              value={draft.block_rule_ids}
              onChange={(e) =>
                setDraft({ ...draft, block_rule_ids: e.target.value })
              }
              placeholder="1,2,3"
            />
          </label>

          <label className="schedules-active-row">
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
            />
            <span>Enabled</span>
          </label>

          <div className="schedules-form-actions">
            {edit.mode === "edit" && (
              <button type="button" onClick={startNew}>
                Cancel
              </button>
            )}
            <button type="submit" className="btn-primary">
              {edit.mode === "new" ? "Add schedule" : "Save"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}