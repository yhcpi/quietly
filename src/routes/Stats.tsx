import { useEffect, useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import * as ipc from "../ipc";
import type { BlockHit, Stats } from "../types";
import { hitsByHour, formatHourLabel } from "../lib/stats";
import "./Stats.css";

const DAY_SECONDS = 86400;
const RECENT_HIT_LIMIT = 20;

export default function Stats() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [hits, setHits] = useState<BlockHit[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      setError(null);
      const since = Math.floor(Date.now() / 1000) - DAY_SECONDS;
      const [s, h] = await Promise.all([
        ipc.getStats("day"),
        ipc.listBlockHits(since, 1000),
      ]);
      setStats(s);
      setHits(h);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  const buckets = hitsByHour(hits);
  const recent = hits.slice(0, RECENT_HIT_LIMIT);

  return (
    <div className="stats-page">
      <header className="stats-header">
        <h1>Stats</h1>
        <p className="stats-sub">Last 24 hours.</p>
      </header>

      {error && <div className="stats-error">{error}</div>}

      {stats && (
        <section className="stats-cards">
          <StatCard
            label="Focus minutes"
            value={stats.focus_minutes_today}
            unit="min"
          />
          <StatCard
            label="Sessions"
            value={stats.sessions_today}
          />
          <StatCard
            label="Completed"
            value={stats.completed_sessions}
          />
          <StatCard
            label="Blocks"
            value={stats.blocks_today}
          />
        </section>
      )}

      <section className="stats-chart-card">
        <h2>Blocks by hour</h2>
        <div className="stats-chart">
          <ResponsiveContainer width="100%" height={220}>
            <BarChart data={buckets}>
              <CartesianGrid strokeDasharray="3 3" stroke="#eee" />
              <XAxis
                dataKey="hour"
                tickFormatter={formatHourLabel}
                interval={2}
                stroke="#888"
              />
              <YAxis allowDecimals={false} stroke="#888" />
              <Tooltip
                labelFormatter={(h: number) => formatHourLabel(h)}
                formatter={(v: number) => [`${v} block(s)`, "Count"]}
              />
              <Bar dataKey="count" fill="#3D5AFE" radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </section>

      <section className="stats-recent-card">
        <h2>Recent blocks</h2>
        {recent.length === 0 ? (
          <p className="stats-empty">No block events yet.</p>
        ) : (
          <table className="stats-recent-table">
            <thead>
              <tr>
                <th>Time</th>
                <th>Kind</th>
                <th>Target</th>
              </tr>
            </thead>
            <tbody>
              {recent.map((h) => (
                <tr key={h.id}>
                  <td className="stats-time">
                    {new Date(h.ts * 1000).toLocaleTimeString()}
                  </td>
                  <td>{h.kind}</td>
                  <td className="stats-target">{h.target}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}

function StatCard({ label, value, unit }: { label: string; value: number; unit?: string }) {
  return (
    <div className="stat-card">
      <div className="stat-value">
        {value}
        {unit && <span className="stat-unit"> {unit}</span>}
      </div>
      <div className="stat-label">{label}</div>
    </div>
  );
}
