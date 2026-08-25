import { useEffect, useMemo, useState } from "react";
import { useApp } from "../state/store";
import type { HealthReport } from "../lib/types";
import * as ipc from "../lib/ipc";
import { PulseIcon } from "../ui/icons";

/* Sparkline: single series — 2px line in the de-emphasis hue, current point
   marked in accent with a surface ring. No legend (title names the series). */
function Sparkline({ points, width = 120, height = 34 }: { points: number[]; width?: number; height?: number }) {
  if (points.length < 2) {
    return (
      <div style={{ height, fontSize: 11, color: "var(--text-tertiary)", display: "flex", alignItems: "center" }}>
        collecting…
      </div>
    );
  }
  const pad = 4;
  const min = Math.min(...points);
  const max = Math.max(...points);
  const span = max - min || 1;
  const step = (width - pad * 2) / (points.length - 1);
  const y = (v: number) => height - pad - ((v - min) / span) * (height - pad * 2);
  const d = points.map((v, i) => `${i === 0 ? "M" : "L"}${(pad + i * step).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const lastX = pad + (points.length - 1) * step;
  const lastY = y(points[points.length - 1]);
  return (
    <svg width={width} height={height} aria-hidden="true">
      <path d={d} fill="none" stroke="var(--text-tertiary)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx={lastX} cy={lastY} r="4" fill="var(--accent)" stroke="var(--bg-elevated)" strokeWidth="2" />
    </svg>
  );
}

/* Meter: fill carries severity; track is a lighter step of the same hue. */
function Meter({ pct }: { pct: number }) {
  const color = pct >= 90 ? "var(--danger)" : pct >= 75 ? "var(--warning)" : "var(--accent)";
  return (
    <div className="progress-track" style={{ background: "var(--accent-soft)" }}>
      <div className="progress-fill" style={{ width: `${Math.min(100, pct)}%`, background: color }} />
    </div>
  );
}

function StatTile({
  label,
  value,
  trend,
  meterPct,
}: {
  label: string;
  value: string;
  trend?: number[];
  meterPct?: number | null;
}) {
  return (
    <div className="card" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
      <span style={{ fontSize: 12, color: "var(--text-secondary)", fontWeight: 500 }}>{label}</span>
      <span style={{ fontSize: 22, fontWeight: 600, letterSpacing: "-0.4px" }}>{value}</span>
      {meterPct != null && <Meter pct={meterPct} />}
      {trend && <Sparkline points={trend.slice(-12)} />}
    </div>
  );
}

export default function HealthView() {
  const hosts = useApp((s) => s.hosts);
  const healthByHost = useApp((s) => s.healthByHost);
  const toast = useApp((s) => s.toast);
  const setView = useApp((s) => s.setView);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [history, setHistory] = useState<HealthReport[]>([]);
  const [checking, setChecking] = useState(false);

  const selected = hosts.find((h) => h.id === selectedId) ?? null;
  const latest = selectedId ? (healthByHost[selectedId] ?? history[history.length - 1]) : undefined;

  useEffect(() => {
    if (!selectedId && hosts.length > 0) setSelectedId(hosts[0].id);
  }, [hosts, selectedId]);

  useEffect(() => {
    if (!selectedId) return;
    void ipc
      .getHealthHistory(selectedId)
      .then(setHistory)
      .catch(() => setHistory([]));
  }, [selectedId, healthByHost]);

  const latencySeries = useMemo(
    () => history.filter((r) => r.latencyMs != null).map((r) => r.latencyMs as number),
    [history],
  );
  const memSeries = useMemo(
    () => history.filter((r) => r.memUsedPct != null).map((r) => r.memUsedPct as number),
    [history],
  );

  const checkNow = async () => {
    if (!selected) return;
    setChecking(true);
    try {
      await ipc.runHealthCheck(selected.id);
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setChecking(false);
    }
  };

  if (hosts.length === 0) {
    return (
      <div className="view-body">
        <div className="empty-state">
          <div className="big">
            <PulseIcon />
          </div>
          <h3>Nothing to monitor</h3>
          <p>Add a host and enable health monitoring to see reachability and load here.</p>
          <button className="btn btn-primary" onClick={() => setView("hosts")}>
            Go to Hosts
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="view-body no-pad" style={{ display: "flex", minHeight: 0 }}>
      <div
        style={{
          width: 240,
          borderRight: "1px solid var(--border)",
          overflowY: "auto",
          padding: 10,
          flexShrink: 0,
        }}
      >
        {hosts.map((h) => {
          const r = healthByHost[h.id];
          return (
            <button
              key={h.id}
              className={`sidebar-item${h.id === selectedId ? " active" : ""}`}
              onClick={() => setSelectedId(h.id)}
            >
              <span
                className={`status-dot ${r ? (r.reachable ? "ok" : "bad") : "unknown"}`}
              />
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {h.name}
              </span>
              {h.healthEnabled && <span className="count">auto</span>}
            </button>
          );
        })}
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: 18 }}>
        {selected && (
          <>
            <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 14 }}>
              <h2 style={{ fontSize: 16 }}>{selected.name}</h2>
              {latest &&
                (latest.reachable ? (
                  <span className="pill success">✓ reachable</span>
                ) : (
                  <span className="pill danger">✕ unreachable</span>
                ))}
              {latest?.sshOk && <span className="pill">ssh ok</span>}
              <div className="spacer" />
              <button className="btn btn-sm" disabled={checking} onClick={checkNow}>
                {checking ? "Checking…" : "Check Now"}
              </button>
            </div>

            {latest?.error && (
              <div
                className="card selectable"
                style={{ borderColor: "var(--danger)", marginBottom: 14, padding: 12, fontSize: 12.5, color: "var(--danger)" }}
              >
                {latest.error}
              </div>
            )}

            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))",
                gap: 12,
                marginBottom: 16,
              }}
            >
              <StatTile
                label="Latency"
                value={latest?.latencyMs != null ? `${latest.latencyMs} ms` : "—"}
                trend={latencySeries}
              />
              <StatTile
                label="Load average"
                value={latest?.loadAvg ?? "—"}
              />
              <StatTile
                label="Memory used"
                value={latest?.memUsedPct != null ? `${latest.memUsedPct.toFixed(0)}%` : "—"}
                meterPct={latest?.memUsedPct}
                trend={memSeries}
              />
              <StatTile
                label="Disk used"
                value={latest?.diskUsedPct != null ? `${latest.diskUsedPct.toFixed(0)}%` : "—"}
                meterPct={latest?.diskUsedPct}
              />
            </div>

            {latest?.uptime && (
              <div className="card" style={{ padding: 14 }}>
                <span style={{ fontSize: 12, color: "var(--text-secondary)", fontWeight: 500 }}>
                  Uptime
                </span>
                <div className="mono selectable" style={{ fontSize: 12.5, marginTop: 6 }}>
                  {latest.uptime}
                </div>
              </div>
            )}

            {history.length > 0 && (
              <div style={{ marginTop: 16, fontSize: 11.5, color: "var(--text-tertiary)" }}>
                {history.length} checks recorded · last{" "}
                {latest ? new Date(latest.timestamp * 1000).toLocaleTimeString() : "—"}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
