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
import type { BlockHit, Range, Stats, UsageEntry } from "../types";
import { hitsByHour, formatHourLabel } from "../lib/stats";
import { humanizeError } from "../lib/error";
import "./Stats.css";

const DAY_SECONDS = 86400;
const RECENT_HIT_LIMIT = 20;
const TOP_N = 10;

const USAGE_RANGES: ReadonlyArray<{ value: Range; label: string }> = [
  { value: "day", label: "24h" },
  { value: "week", label: "7d" },
  { value: "month", label: "30d" },
];

export default function Stats() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [hits, setHits] = useState<BlockHit[]>([]);
  const [usage, setUsage] = useState<UsageEntry[]>([]);
  const [usageRange, setUsageRange] = useState<Range>("week");
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
      setError(humanizeError(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    ipc
      .getUsageStats(usageRange)
      .then(setUsage)
      .catch((e) => setError(humanizeError(e)));
  }, [usageRange]);

  const buckets = hitsByHour(hits);
  const recent = hits.slice(0, RECENT_HIT_LIMIT);
  // `get_usage_stats` returns both kinds, already sorted by seconds DESC.
  const topApps = usage.filter((u) => u.kind === "app").slice(0, TOP_N);
  const topSites = usage.filter((u) => u.kind === "url").slice(0, TOP_N);

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

      <section className="stats-chart-card">
        <div className="stats-usage-head">
          <h2>Usage</h2>
          <div className="stats-range">
            {USAGE_RANGES.map((r) => (
              <button
                key={r.value}
                className={
                  "stats-range-btn" +
                  (usageRange === r.value ? " stats-range-btn-active" : "")
                }
                onClick={() => setUsageRange(r.value)}
              >
                {r.label}
              </button>
            ))}
          </div>
        </div>
        <p className="stats-sub">
          Foreground app time and website activity. Nothing is recorded during
          breaks, when the guard and proxy are off. Website minutes are
          approximate — each request counts as ~1s.
        </p>
        <UsageChart title="Top apps" data={topApps} />
        <UsageChart title="Top websites" data={topSites} />
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

function UsageChart({ title, data }: { title: string; data: UsageEntry[] }) {
  return (
    <div className="stats-usage-chart">
      <h3>{title}</h3>
      {data.length === 0 ? (
        <p className="stats-empty">No usage recorded yet.</p>
      ) : (
        <ResponsiveContainer width="100%" height={Math.max(120, data.length * 28)}>
          <BarChart data={data} layout="vertical" margin={{ left: 8, right: 16 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="#eee" horizontal={false} />
            <XAxis type="number" allowDecimals={false} stroke="#888" />
            <YAxis
              type="category"
              dataKey="target"
              width={150}
              stroke="#888"
              tick={{ fontSize: 12 }}
            />
            <Tooltip formatter={(v: number) => [`${v} min`, "Time"]} />
            <Bar dataKey="minutes" fill="#3D5AFE" radius={[0, 4, 4, 0]} />
          </BarChart>
        </ResponsiveContainer>
      )}
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
