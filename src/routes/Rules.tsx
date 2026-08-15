import { useEffect, useState } from "react";
import * as ipc from "../ipc";
import type { BlockRule } from "../types";
import {
  blankRule,
  validateRule,
  RULE_KINDS,
  RULE_MATCH_TYPES,
  type RuleDraft,
} from "../lib/rule";
import "./Rules.css";

type EditState =
  | { mode: "new" }
  | { mode: "edit"; rule: BlockRule };

function toDraft(rule: BlockRule): RuleDraft {
  return {
    kind: rule.kind,
    pattern: rule.pattern,
    match_type: rule.match_type,
    note: rule.note,
    active: rule.active,
  };
}

export default function Rules() {
  const [rules, setRules] = useState<BlockRule[]>([]);
  const [edit, setEdit] = useState<EditState>({ mode: "new" });
  const [draft, setDraft] = useState<RuleDraft>(blankRule());
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      setError(null);
      setRules(await ipc.listBlockRules());
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  function startNew() {
    setEdit({ mode: "new" });
    setDraft(blankRule());
    setError(null);
  }

  function startEdit(rule: BlockRule) {
    setEdit({ mode: "edit", rule });
    setDraft(toDraft(rule));
    setError(null);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const err = validateRule(draft);
    if (err) {
      setError(err);
      return;
    }
    try {
      setError(null);
      if (edit.mode === "new") {
        await ipc.createBlockRule(draft);
      } else {
        await ipc.updateBlockRule({ ...edit.rule, ...draft });
      }
      startNew();
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(rule: BlockRule) {
    if (rule.id == null) return;
    try {
      setError(null);
      await ipc.deleteBlockRule(rule.id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleToggle(rule: BlockRule) {
    if (rule.id == null) return;
    try {
      setError(null);
      await ipc.setBlockRuleActive(rule.id, !rule.active);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="rules-page">
      <header className="rules-header">
        <h1>Block rules</h1>
        <p className="rules-sub">
          Block distracting URLs, domains, or app paths during focus.
        </p>
      </header>

      {error && <div className="rules-error">{error}</div>}

      <section className="rules-list">
        {rules.length === 0 ? (
          <p className="rules-empty">No rules yet. Add one below.</p>
        ) : (
          <table className="rules-table">
            <thead>
              <tr>
                <th>Pattern</th>
                <th>Kind</th>
                <th>Match</th>
                <th>Note</th>
                <th>Active</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {rules.map((r) => (
                <tr key={r.id} className={r.active ? "" : "rules-row-inactive"}>
                  <td className="rules-pattern">{r.pattern}</td>
                  <td>{r.kind}</td>
                  <td>{r.match_type}</td>
                  <td>{r.note}</td>
                  <td>
                    <label className="rules-toggle">
                      <input
                        type="checkbox"
                        checked={r.active}
                        onChange={() => handleToggle(r)}
                      />
                    </label>
                  </td>
                  <td className="rules-actions">
                    <button onClick={() => startEdit(r)}>Edit</button>
                    <button
                      className="btn-danger"
                      onClick={() => handleDelete(r)}
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

      <section className="rules-form-card">
        <h2>{edit.mode === "new" ? "New rule" : "Edit rule"}</h2>
        <form onSubmit={handleSubmit} className="rules-form">
          <label>
            <span>Kind</span>
            <select
              value={draft.kind}
              onChange={(e) =>
                setDraft({ ...draft, kind: e.target.value as RuleDraft["kind"] })
              }
            >
              {RULE_KINDS.map((k) => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
          </label>

          <label>
            <span>Pattern</span>
            <input
              type="text"
              value={draft.pattern}
              onChange={(e) => setDraft({ ...draft, pattern: e.target.value })}
              placeholder="twitter.com"
            />
          </label>

          <label>
            <span>Match</span>
            <select
              value={draft.match_type}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  match_type: e.target.value as RuleDraft["match_type"],
                })
              }
            >
              {RULE_MATCH_TYPES.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </label>

          <label>
            <span>Note</span>
            <input
              type="text"
              value={draft.note}
              onChange={(e) => setDraft({ ...draft, note: e.target.value })}
            />
          </label>

          <label className="rules-active-row">
            <input
              type="checkbox"
              checked={draft.active}
              onChange={(e) => setDraft({ ...draft, active: e.target.checked })}
            />
            <span>Active</span>
          </label>

          <div className="rules-form-actions">
            {edit.mode === "edit" && (
              <button type="button" onClick={startNew}>
                Cancel
              </button>
            )}
            <button type="submit" className="btn-primary">
              {edit.mode === "new" ? "Add rule" : "Save"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}