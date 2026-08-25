import { useEffect, useState } from "react";
import { useApp } from "../state/store";
import type { ForwardKind, ForwardSpec } from "../lib/types";
import * as ipc from "../lib/ipc";
import { Dropdown, Field, Modal } from "../ui/primitives";
import { ArrowsIcon } from "../ui/icons";

const KIND_OPTIONS: { value: ForwardKind; label: string }[] = [
  { value: "local", label: "Local → Remote  (-L)" },
  { value: "remote", label: "Remote → Local  (-R)" },
  { value: "dynamic", label: "Dynamic SOCKS5  (-D)" },
];

const KIND_HELP: Record<ForwardKind, string> = {
  local:
    "Listens on your machine and tunnels connections to a host reachable from the server.",
  remote:
    "Listens on the server and tunnels connections back to a host reachable from your machine.",
  dynamic: "Runs a SOCKS5 proxy on your machine that routes traffic through the server.",
};

export default function ForwardsView() {
  const sessions = useApp((s) => s.sessions);
  const forwards = useApp((s) => s.forwards);
  const refreshForwards = useApp((s) => s.refreshForwards);
  const toast = useApp((s) => s.toast);
  const setView = useApp((s) => s.setView);

  const [adding, setAdding] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [spec, setSpec] = useState<ForwardSpec>({
    kind: "local",
    bindHost: "127.0.0.1",
    bindPort: 8080,
    targetHost: "127.0.0.1",
    targetPort: 80,
  });
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    void refreshForwards();
  }, [refreshForwards]);

  useEffect(() => {
    if (!sessionId && sessions.length > 0) setSessionId(sessions[0].id);
  }, [sessions, sessionId]);

  const start = async () => {
    if (!sessionId) return;
    setStarting(true);
    try {
      await ipc.startForward(sessionId, spec);
      await refreshForwards();
      setAdding(false);
      toast("Forward started");
    } catch (e) {
      toast(`Forward failed: ${e}`, "error");
    } finally {
      setStarting(false);
    }
  };

  const stop = async (id: string) => {
    try {
      await ipc.stopForward(id);
      await refreshForwards();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const describe = (s: ForwardSpec): string => {
    if (s.kind === "dynamic") return `SOCKS5 on ${s.bindHost}:${s.bindPort}`;
    if (s.kind === "local")
      return `${s.bindHost}:${s.bindPort} → ${s.targetHost}:${s.targetPort}`;
    return `server ${s.bindHost}:${s.bindPort} → ${s.targetHost}:${s.targetPort}`;
  };

  return (
    <div className="view-body">
      <div style={{ display: "flex", marginBottom: 16 }}>
        <div className="spacer" />
        <button
          className="btn btn-primary"
          disabled={sessions.length === 0}
          onClick={() => setAdding(true)}
        >
          + New Forward
        </button>
      </div>

      {forwards.length === 0 ? (
        <div className="empty-state" style={{ height: "60%" }}>
          <div className="big">
            <ArrowsIcon />
          </div>
          <h3>No active forwards</h3>
          <p>
            {sessions.length === 0
              ? "Connect to a host first, then create local, remote, or SOCKS5 tunnels."
              : "Create a tunnel to expose remote services locally or vice versa."}
          </p>
          {sessions.length === 0 && (
            <button className="btn btn-primary" onClick={() => setView("hosts")}>
              Go to Hosts
            </button>
          )}
        </div>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>Type</th>
              <th>Route</th>
              <th>Connection</th>
              <th>Status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {forwards.map((f) => (
              <tr key={f.id}>
                <td>
                  <span className="pill">{f.spec.kind}</span>
                </td>
                <td className="mono selectable" style={{ fontSize: 12.5 }}>
                  {describe(f.spec)}
                </td>
                <td style={{ color: "var(--text-secondary)" }}>{f.hostName}</td>
                <td>
                  {f.status === "active" ? (
                    <span className="pill success">active</span>
                  ) : (
                    <span className="pill danger" title={f.error ?? undefined}>
                      error
                    </span>
                  )}
                </td>
                <td style={{ textAlign: "right" }}>
                  <button className="btn btn-sm btn-danger" onClick={() => stop(f.id)}>
                    Stop
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {adding && (
        <Modal
          title="New Port Forward"
          onClose={() => setAdding(false)}
          footer={
            <>
              <button className="btn btn-ghost" onClick={() => setAdding(false)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={!sessionId || starting || spec.bindPort <= 0}
                onClick={start}
              >
                {starting ? "Starting…" : "Start Forward"}
              </button>
            </>
          }
        >
          <Field label="Connection">
            <Dropdown
              value={sessionId}
              options={sessions.map((s) => ({ value: s.id, label: s.hostName }))}
              onChange={setSessionId}
            />
          </Field>
          <Field label="Type">
            <Dropdown
              value={spec.kind}
              options={KIND_OPTIONS}
              onChange={(kind) => setSpec({ ...spec, kind })}
            />
          </Field>
          <p style={{ fontSize: 12, color: "var(--text-secondary)" }}>{KIND_HELP[spec.kind]}</p>
          <div className="form-row">
            <Field label={spec.kind === "remote" ? "Server Bind Address" : "Local Bind Address"}>
              <input
                className="input mono"
                value={spec.bindHost}
                onChange={(e) => setSpec({ ...spec, bindHost: e.target.value })}
              />
            </Field>
            <Field label={spec.kind === "remote" ? "Server Port" : "Local Port"}>
              <input
                className="input mono"
                inputMode="numeric"
                value={spec.bindPort}
                onChange={(e) => setSpec({ ...spec, bindPort: Number(e.target.value) || 0 })}
              />
            </Field>
          </div>
          {spec.kind !== "dynamic" && (
            <div className="form-row">
              <Field label="Target Host">
                <input
                  className="input mono"
                  value={spec.targetHost}
                  onChange={(e) => setSpec({ ...spec, targetHost: e.target.value })}
                />
              </Field>
              <Field label="Target Port">
                <input
                  className="input mono"
                  inputMode="numeric"
                  value={spec.targetPort}
                  onChange={(e) =>
                    setSpec({ ...spec, targetPort: Number(e.target.value) || 0 })
                  }
                />
              </Field>
            </div>
          )}
        </Modal>
      )}
    </div>
  );
}
