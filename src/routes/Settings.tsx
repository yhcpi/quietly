import { useEffect, useState } from "react";
import * as ipc from "../ipc";
import { formatSettingsJson, parseSettingsJson } from "../lib/settings";
import "./Settings.css";

export default function Settings() {
  const [text, setText] = useState<string>("");
  const [original, setOriginal] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function load() {
    try {
      setError(null);
      const value = await ipc.getSettings();
      const formatted = formatSettingsJson(value);
      setText(formatted);
      setOriginal(formatted);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    load();
  }, []);

  const dirty = text !== original;

  async function handleSave() {
    const parsed = parseSettingsJson(text);
    if (!parsed.ok) {
      setError(parsed.error);
      return;
    }
    try {
      setError(null);
      setSaving(true);
      await ipc.setSettings(parsed.value);
      setOriginal(text);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function handleReset() {
    setText(original);
    setError(null);
  }

  return (
    <div className="settings-page">
      <header className="settings-header">
        <h1>Settings</h1>
        <p className="settings-sub">
          Edit the settings JSON. Stored in the local database.
        </p>
      </header>

      {error && <div className="settings-error">{error}</div>}

      <section className="settings-editor-card">
        <textarea
          className="settings-textarea"
          value={text}
          onChange={(e) => setText(e.target.value)}
          spellCheck={false}
          rows={20}
        />
        <div className="settings-actions">
          <button
            type="button"
            onClick={handleReset}
            disabled={!dirty || saving}
          >
            Reset
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={handleSave}
            disabled={!dirty || saving}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </section>
    </div>
  );
}