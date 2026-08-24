import { useEffect } from "react";
import { useApp } from "./state/store";
import type { ViewId } from "./lib/types";
import {
  ArrowsIcon,
  FolderIcon,
  GearIcon,
  JobsIcon,
  PulseIcon,
  ServerIcon,
  TerminalIcon,
} from "./ui/icons";
import HostsView from "./views/HostsView";
import TerminalView from "./views/TerminalView";
import SftpView from "./views/SftpView";
import ForwardsView from "./views/ForwardsView";
import HealthView from "./views/HealthView";
import JobsView from "./views/JobsView";
import SettingsView from "./views/SettingsView";

const NAV: { id: ViewId; label: string; icon: JSX.Element; section?: string }[] = [
  { id: "hosts", label: "Hosts", icon: <ServerIcon /> },
  { id: "terminal", label: "Terminal", icon: <TerminalIcon /> },
  { id: "sftp", label: "Files", icon: <FolderIcon /> },
  { id: "forwards", label: "Port Forwarding", icon: <ArrowsIcon /> },
  { id: "health", label: "Health", icon: <PulseIcon /> },
  { id: "jobs", label: "Jobs", icon: <JobsIcon /> },
];

const TITLES: Record<ViewId, string> = {
  hosts: "Hosts",
  terminal: "Terminal",
  sftp: "Files",
  forwards: "Port Forwarding",
  health: "Server Health",
  jobs: "Jobs",
  settings: "Settings",
};

export default function App() {
  const view = useApp((s) => s.view);
  const setView = useApp((s) => s.setView);
  const init = useApp((s) => s.init);
  const sessions = useApp((s) => s.sessions);
  const termTabs = useApp((s) => s.termTabs);
  const jobs = useApp((s) => s.jobs);
  const toasts = useApp((s) => s.toasts);
  const dismissToast = useApp((s) => s.dismissToast);

  useEffect(() => {
    void init();
  }, [init]);

  const runningJobs = jobs.filter((j) => j.state === "running" || j.state === "queued").length;
  const counts: Partial<Record<ViewId, number>> = {
    terminal: termTabs.length,
    jobs: runningJobs,
  };

  return (
    <div className="app-shell">
      <nav className="sidebar">
        <div className="sidebar-brand">
          <span className="logo">⌘</span>
          SSH Manager
        </div>
        {NAV.map((item) => (
          <button
            key={item.id}
            className={`sidebar-item${view === item.id ? " active" : ""}`}
            onClick={() => setView(item.id)}
          >
            <span className="icon">{item.icon}</span>
            <span>{item.label}</span>
            {counts[item.id] ? <span className="count">{counts[item.id]}</span> : null}
          </button>
        ))}
        <div className="sidebar-section">Connections</div>
        {sessions.length === 0 && (
          <div style={{ padding: "2px 10px", fontSize: 12, color: "var(--text-tertiary)" }}>
            Not connected
          </div>
        )}
        {sessions.map((s) => (
          <div key={s.id} className="sidebar-item" style={{ fontSize: 12.5 }}>
            <span className="status-dot ok" />
            <span
              style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
            >
              {s.hostName}
            </span>
          </div>
        ))}
        <div className="sidebar-footer">
          <button
            className={`sidebar-item${view === "settings" ? " active" : ""}`}
            onClick={() => setView("settings")}
          >
            <span className="icon">
              <GearIcon />
            </span>
            <span>Settings</span>
          </button>
        </div>
      </nav>

      <main className="main-pane">
        <header className="app-header">
          <span className="view-title">{TITLES[view]}</span>
        </header>
        {view === "hosts" && <HostsView />}
        {view === "terminal" && <TerminalView />}
        {view === "sftp" && <SftpView />}
        {view === "forwards" && <ForwardsView />}
        {view === "health" && <HealthView />}
        {view === "jobs" && <JobsView />}
        {view === "settings" && <SettingsView />}
      </main>

      <div className="toast-stack">
        {toasts.map((t) => (
          <div key={t.id} className={`toast${t.kind === "error" ? " error" : ""}`}>
            <span>{t.kind === "error" ? "⚠️" : "ℹ️"}</span>
            <span className="toast-msg">{t.message}</span>
            <button className="btn btn-ghost btn-icon btn-sm" onClick={() => dismissToast(t.id)}>
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
