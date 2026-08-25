import { useEffect } from "react";
import { useApp } from "../state/store";
import * as ipc from "../lib/ipc";
import { JobsIcon } from "../ui/icons";
import type { JobState } from "../lib/types";

const STATE_PILL: Record<JobState, string> = {
  queued: "neutral",
  running: "",
  done: "success",
  failed: "danger",
  cancelled: "warning",
};

export default function JobsView() {
  const jobs = useApp((s) => s.jobs);
  const refreshJobs = useApp((s) => s.refreshJobs);
  const toast = useApp((s) => s.toast);

  useEffect(() => {
    void refreshJobs();
  }, [refreshJobs]);

  const cancel = async (id: string) => {
    try {
      await ipc.cancelJob(id);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  if (jobs.length === 0) {
    return (
      <div className="view-body">
        <div className="empty-state">
          <div className="big">
            <JobsIcon />
          </div>
          <h3>No jobs yet</h3>
          <p>File transfers and background tasks appear here with live progress.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="view-body">
      <div style={{ display: "flex", flexDirection: "column", gap: 10, maxWidth: 760 }}>
        {jobs.map((j) => (
          <div key={j.id} className="card" style={{ padding: 14 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <strong style={{ fontSize: 13.5 }}>{j.label}</strong>
              <span className={`pill ${STATE_PILL[j.state]}`}>
                {j.state === "running" && (
                  <span className="typing-dots">
                    <i />
                    <i />
                    <i />
                  </span>
                )}
                {j.state}
              </span>
              <div className="spacer" />
              {(j.state === "running" || j.state === "queued") && (
                <button className="btn btn-sm btn-danger" onClick={() => cancel(j.id)}>
                  Cancel
                </button>
              )}
            </div>
            {j.state === "running" && j.progress != null && (
              <div style={{ marginTop: 10 }}>
                <div className="progress-track">
                  <div
                    className="progress-fill"
                    style={{ width: `${Math.round(j.progress * 100)}%` }}
                  />
                </div>
              </div>
            )}
            {(j.detail || j.error) && (
              <div
                className="mono selectable"
                style={{
                  marginTop: 8,
                  fontSize: 11.5,
                  color: j.error ? "var(--danger)" : "var(--text-secondary)",
                }}
              >
                {j.error ?? j.detail}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
