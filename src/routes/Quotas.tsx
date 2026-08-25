import { useEffect, useState } from "react";
import * as ipc from "../ipc";
import type { AppQuota, QuotaStatus } from "../types";
import { humanizeError } from "../lib/error";
import "./Quotas.css";

const DEFAULT_DAILY_MINUTES = 60;

type Row = AppQuota & { used_minutes: number; exceeded: boolean };

// `list_quotas` carries id + active; `get_quota_status` carries today's
// usage with the daily reset already applied. Patterns are unique (the
// Rust side rejects duplicates), so joining on app_pattern is safe.
function join(quotas: AppQuota[], status: QuotaStatus[]): Row[] {
  const byPattern = new Map(status.map((s) => [s.app_pattern, s]));
  return quotas.map((q) => {
    const s = byPattern.get(q.app_pattern);
    return {
      ...q,
      used_minutes: s?.used_minutes ?? 0,
      exceeded: s?.exceeded ?? false,
    };
  });
}

export default function Quotas() {
  const [rows, setRows] = useState<Row[]>([]);
  const [pattern, setPattern] = useState("");
  const [dailyMinutes, setDailyMinutes] = useState(DEFAULT_DAILY_MINUTES);
  const [error, setError] = useState<string | null>(null);
  // Inline editor state: which row is currently being edited, which
  // field is being edited (Today or Budget), and the in-flight value
  // (in minutes — that's the unit the UI displays). Nulls when nothing
  // is being edited.
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingField, setEditingField] = useState<"used" | "budget" | null>(null);
  const [editingValue, setEditingValue] = useState<string>("");

  async function refresh() {
    try {
      setError(null);
      const [quotas, status] = await Promise.all([
        ipc.listQuotas(),
        ipc.getQuotaStatus(),
      ]);
      setRows(join(quotas, status));
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!pattern.trim()) {
      setError("App pattern is required");
      return;
    }
    if (!Number.isFinite(dailyMinutes) || dailyMinutes <= 0) {
      setError("Daily minutes must be greater than 0");
      return;
    }
    try {
      setError(null);
      await ipc.createQuota(pattern.trim(), dailyMinutes);
      setPattern("");
      setDailyMinutes(DEFAULT_DAILY_MINUTES);
      await refresh();
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  async function handleToggle(row: Row) {
    if (row.id == null) return;
    try {
      setError(null);
      await ipc.setQuotaActive(row.id, !row.active);
      await refresh();
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  async function handleDelete(row: Row) {
    if (row.id == null) return;
    try {
      setError(null);
      await ipc.deleteQuota(row.id);
      await refresh();
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  function startEdit(row: Row, field: "used" | "budget") {
    if (row.id == null) return;
    setEditingId(row.id);
    setEditingField(field);
    if (field === "used") {
      // Pre-fill with the actual persisted `used_seconds` (not the
      // display-rounded used_minutes) so a 95-second record doesn't
      // get silently coerced down to 1 minute.
      setEditingValue(String(Math.round(row.used_seconds / 60)));
    } else {
      setEditingValue(String(row.daily_minutes));
    }
  }

  function cancelEdit() {
    setEditingId(null);
    setEditingField(null);
    setEditingValue("");
  }

  async function saveEdit(row: Row) {
    if (row.id == null || editingField == null) return;
    const minutes = Number(editingValue);
    if (!Number.isFinite(minutes) || minutes < 0) {
      setError("Time must be a non-negative number");
      return;
    }
    try {
      setError(null);
      if (editingField === "used") {
        // Persist the new used_seconds. The trackers (foreground 10s +
        // background 5s ticks) read used_seconds from the DB on every
        // increment, so the very next tick after this save respects
        // the new baseline — the threshold check in `increment_used`
        // then sees the edited value as the starting point for the
        // remaining daily budget.
        await ipc.updateQuota({
          ...row,
          used_seconds: Math.round(minutes * 60),
        });
      } else {
        // Editing the daily budget. The new limit takes effect
        // immediately — the threshold check inside `increment_used`
        // recomputes against the current row's daily_minutes on
        // every tick.
        await ipc.updateQuota({
          ...row,
          daily_minutes: Math.round(minutes),
        });
      }
      setEditingId(null);
      setEditingField(null);
      setEditingValue("");
      await refresh();
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  async function resetToZero(row: Row) {
    if (row.id == null) return;
    try {
      setError(null);
      await ipc.updateQuota({ ...row, used_seconds: 0 });
      setEditingId(null);
      setEditingField(null);
      setEditingValue("");
      await refresh();
    } catch (e) {
      setError(humanizeError(e));
    }
  }

  return (
    <div className="quotas-page">
      <header className="quotas-header">
        <h1>App quotas</h1>
        <p className="quotas-sub">
          Daily time budget per app, matched on the exact exe file name.
          Time only accumulates while the guard is running, so breaks don't
          count. Exceeding a quota warns you — it doesn't close the app.
          Click "Today" to manually adjust the recorded usage.
        </p>
      </header>

      {error && <div className="quotas-error">{error}</div>}

      <section className="quotas-list">
        {rows.length === 0 ? (
          <p className="quotas-empty">No quotas yet. Add one below.</p>
        ) : (
          <table className="quotas-table">
            <thead>
              <tr>
                <th>App</th>
                <th>Today</th>
                <th>Budget</th>
                <th>Active</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const isEditing = editingId === r.id;
                const editEditor = (field: "used" | "budget", label: string, withReset = false) => (
                  <span className="quotas-edit">
                    <input
                      className="quotas-edit-input"
                      type="number"
                      min={field === "budget" ? 1 : 0}
                      step={1}
                      value={editingValue}
                      autoFocus
                      onChange={(e) => setEditingValue(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") saveEdit(r);
                        else if (e.key === "Escape") cancelEdit();
                      }}
                      aria-label={`${label} for ${r.app_pattern}`}
                    />
                    <span className="quotas-edit-unit">min</span>
                    <button
                      className="btn-ghost quotas-edit-action"
                      onClick={() => saveEdit(r)}
                    >
                      Save
                    </button>
                    <button
                      className="btn-ghost quotas-edit-action"
                      onClick={cancelEdit}
                    >
                      Cancel
                    </button>
                    {withReset && (
                      <button
                        className="btn-ghost quotas-edit-action quotas-edit-reset"
                        onClick={() => resetToZero(r)}
                        title="Reset today's count to 0"
                      >
                        Reset
                      </button>
                    )}
                  </span>
                );
                return (
                  <tr key={r.id} className={r.active ? "" : "quotas-row-inactive"}>
                    <td className="quotas-pattern">{r.app_pattern}</td>
                    <td className={r.exceeded ? "quotas-exceeded" : ""}>
                      {isEditing && editingField === "used" ? (
                        editEditor("used", "Used minutes", true)
                      ) : (
                        <button
                          className="quotas-edit-trigger"
                          onClick={() => startEdit(r, "used")}
                          title="Click to edit today's count"
                        >
                          {r.used_minutes} min{r.exceeded && " — over budget"}
                          <span className="quotas-edit-hint">edit</span>
                        </button>
                      )}
                    </td>
                    <td>
                      {isEditing && editingField === "budget" ? (
                        editEditor("budget", "Daily budget minutes")
                      ) : (
                        <button
                          className="quotas-edit-trigger"
                          onClick={() => startEdit(r, "budget")}
                          title="Click to edit the daily budget"
                        >
                          {r.daily_minutes} min
                          <span className="quotas-edit-hint">edit</span>
                        </button>
                      )}
                    </td>
                    <td>
                      <label className="quotas-toggle">
                        <input
                          type="checkbox"
                          checked={r.active}
                          onChange={() => handleToggle(r)}
                        />
                      </label>
                    </td>
                    <td className="quotas-actions">
                      <button className="btn-danger" onClick={() => handleDelete(r)}>
                        Delete
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      <section className="quotas-form-card">
        <h2>New quota</h2>
        <form onSubmit={handleSubmit} className="quotas-form">
          <label>
            <span>App (exe file name)</span>
            <input
              type="text"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder="Discord.exe"
            />
          </label>

          <label>
            <span>Daily minutes</span>
            <input
              type="number"
              min={1}
              value={dailyMinutes}
              onChange={(e) => {
                const n = parseInt(e.target.value, 10);
                setDailyMinutes(Number.isFinite(n) ? n : 0);
              }}
            />
          </label>

          <div className="quotas-form-actions">
            <button type="submit" className="btn-primary">
              Add quota
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
