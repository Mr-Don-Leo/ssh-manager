import { useMemo, useState } from "react";
import { useApp } from "../state/store";
import type { AuthMethod, HostEntry, HostKeyPrompt, SessionInfo } from "../lib/types";
import * as ipc from "../lib/ipc";
import { Checkbox, Dropdown, Field, Modal } from "../ui/primitives";
import { PlugIcon, ServerIcon, TerminalIcon } from "../ui/icons";

const emptyHost = (): HostEntry => ({
  id: "",
  name: "",
  host: "",
  port: 22,
  username: "",
  authMethod: "agent",
  keyPath: null,
  tags: [],
  notes: null,
  healthEnabled: false,
  healthIntervalSecs: 60,
});

const AUTH_OPTIONS: { value: AuthMethod; label: string }[] = [
  { value: "agent", label: "SSH Agent" },
  { value: "key", label: "Private Key File" },
  { value: "password", label: "Password" },
];

function HostEditor({
  initial,
  onDone,
}: {
  initial: HostEntry;
  onDone: (saved: boolean) => void;
}) {
  const [host, setHost] = useState<HostEntry>(initial);
  const [secret, setSecret] = useState("");
  const [tagsText, setTagsText] = useState(initial.tags.join(", "));
  const [saving, setSaving] = useState(false);
  const toast = useApp((s) => s.toast);
  const refreshHosts = useApp((s) => s.refreshHosts);

  const canSave = host.name.trim() && host.host.trim() && host.username.trim() && host.port > 0;

  const save = async () => {
    setSaving(true);
    try {
      const tags = tagsText
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      const saved = await ipc.saveHost({ ...host, tags });
      if (secret) await ipc.setSecret(saved.id, secret);
      await refreshHosts();
      toast(`Saved “${saved.name}”`);
      onDone(true);
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setSaving(false);
    }
  };

  const secretLabel =
    host.authMethod === "password" ? "Password" : "Key Passphrase (optional)";

  return (
    <Modal
      title={initial.id ? "Edit Host" : "New Host"}
      onClose={() => onDone(false)}
      footer={
        <>
          <button className="btn btn-ghost" onClick={() => onDone(false)}>
            Cancel
          </button>
          <button className="btn btn-primary" disabled={!canSave || saving} onClick={save}>
            {saving ? "Saving…" : "Save Host"}
          </button>
        </>
      }
    >
      <Field label="Display Name">
        <input
          className="input"
          value={host.name}
          placeholder="Production Web Server"
          onChange={(e) => setHost({ ...host, name: e.target.value })}
        />
      </Field>
      <div className="form-row">
        <Field label="Hostname / IP">
          <input
            className="input mono"
            value={host.host}
            placeholder="203.0.113.10"
            onChange={(e) => setHost({ ...host, host: e.target.value })}
          />
        </Field>
        <Field label="Port">
          <input
            className="input mono"
            type="text"
            inputMode="numeric"
            value={host.port}
            onChange={(e) => setHost({ ...host, port: Number(e.target.value) || 0 })}
          />
        </Field>
      </div>
      <div className="form-row">
        <Field label="Username">
          <input
            className="input mono"
            value={host.username}
            placeholder="root"
            onChange={(e) => setHost({ ...host, username: e.target.value })}
          />
        </Field>
        <Field label="Authentication">
          <Dropdown
            value={host.authMethod}
            options={AUTH_OPTIONS}
            onChange={(authMethod) => setHost({ ...host, authMethod })}
          />
        </Field>
      </div>
      {host.authMethod === "key" && (
        <Field label="Private Key Path">
          <input
            className="input mono"
            value={host.keyPath ?? ""}
            placeholder="~/.ssh/id_ed25519"
            onChange={(e) => setHost({ ...host, keyPath: e.target.value || null })}
          />
        </Field>
      )}
      {host.authMethod !== "agent" && (
        <Field label={secretLabel}>
          <input
            className="input"
            type="password"
            value={secret}
            placeholder={initial.id ? "Leave blank to keep existing" : "••••••••"}
            onChange={(e) => setSecret(e.target.value)}
          />
        </Field>
      )}
      <Field label="Tags">
        <input
          className="input"
          value={tagsText}
          placeholder="prod, web  (comma-separated)"
          onChange={(e) => setTagsText(e.target.value)}
        />
      </Field>
      <Field label="Notes">
        <textarea
          className="input"
          rows={2}
          value={host.notes ?? ""}
          onChange={(e) => setHost({ ...host, notes: e.target.value || null })}
        />
      </Field>
      <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
        <Checkbox
          checked={host.healthEnabled}
          onChange={(healthEnabled) => setHost({ ...host, healthEnabled })}
          label="Monitor health"
        />
        {host.healthEnabled && (
          <div style={{ display: "flex", alignItems: "center", gap: 7, fontSize: 13 }}>
            every
            <input
              className="input mono"
              style={{ width: 64 }}
              inputMode="numeric"
              value={host.healthIntervalSecs}
              onChange={(e) =>
                setHost({ ...host, healthIntervalSecs: Number(e.target.value) || 60 })
              }
            />
            seconds
          </div>
        )}
      </div>
    </Modal>
  );
}

export default function HostsView() {
  const hosts = useApp((s) => s.hosts);
  const sessions = useApp((s) => s.sessions);
  const healthByHost = useApp((s) => s.healthByHost);
  const toast = useApp((s) => s.toast);
  const refreshHosts = useApp((s) => s.refreshHosts);
  const refreshSessions = useApp((s) => s.refreshSessions);
  const addTermTab = useApp((s) => s.addTermTab);

  const [editing, setEditing] = useState<HostEntry | null>(null);
  const [query, setQuery] = useState("");
  const [connecting, setConnecting] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<HostEntry | null>(null);
  const [keyPrompt, setKeyPrompt] = useState<{
    host: HostEntry;
    prompt: HostKeyPrompt;
    openShellAfter: boolean;
  } | null>(null);
  const [trusting, setTrusting] = useState(false);

  const filtered = useMemo(() => {
    const q = query.toLowerCase();
    return hosts.filter(
      (h) =>
        !q ||
        h.name.toLowerCase().includes(q) ||
        h.host.toLowerCase().includes(q) ||
        h.tags.some((t) => t.toLowerCase().includes(q)),
    );
  }, [hosts, query]);

  const sessionFor = (hostId: string) => sessions.find((s) => s.hostId === hostId);

  /** Connects, or surfaces the host-key trust prompt and returns null. */
  const establish = async (
    h: HostEntry,
    openShellAfter: boolean,
    acceptFingerprint?: string,
  ): Promise<SessionInfo | null> => {
    const outcome = await ipc.connectHost(h.id, acceptFingerprint);
    if (outcome.status === "hostKeyPrompt") {
      setKeyPrompt({ host: h, prompt: outcome.prompt, openShellAfter });
      return null;
    }
    await refreshSessions();
    return outcome.session;
  };

  const openTerminalFor = async (h: HostEntry, session: SessionInfo) => {
    try {
      const termId = await ipc.openTerminal(session.id, 80, 24);
      addTermTab({ termId, sessionId: session.id, title: h.name });
    } catch (e) {
      toast(`Terminal failed: ${e}`, "error");
    }
  };

  const connect = async (h: HostEntry) => {
    setConnecting(h.id);
    try {
      const session = await establish(h, false);
      if (session) toast(`Connected to ${h.name}`);
    } catch (e) {
      toast(`Connection failed: ${e}`, "error");
    } finally {
      setConnecting(null);
    }
  };

  const openShell = async (h: HostEntry) => {
    let session = sessionFor(h.id);
    if (!session) {
      setConnecting(h.id);
      try {
        session = (await establish(h, true)) ?? undefined;
      } catch (e) {
        toast(`Connection failed: ${e}`, "error");
      } finally {
        setConnecting(null);
      }
      if (!session) return; // trust prompt shown, or connect failed
    }
    await openTerminalFor(h, session);
  };

  const trustHostKey = async () => {
    if (!keyPrompt) return;
    const { host, prompt, openShellAfter } = keyPrompt;
    setTrusting(true);
    try {
      const session = await establish(host, openShellAfter, prompt.fingerprint);
      if (session) {
        setKeyPrompt(null);
        toast(`Connected to ${host.name}`);
        if (openShellAfter) await openTerminalFor(host, session);
      }
      // else: the server presented yet another key — establish() already
      // replaced the prompt with the new fingerprint.
    } catch (e) {
      setKeyPrompt(null);
      toast(`Connection failed: ${e}`, "error");
    } finally {
      setTrusting(false);
    }
  };

  const disconnect = async (hostId: string) => {
    const session = sessionFor(hostId);
    if (!session) return;
    try {
      await ipc.disconnectSession(session.id);
      await refreshSessions();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const doDelete = async (h: HostEntry) => {
    try {
      await ipc.deleteHost(h.id);
      await refreshHosts();
      toast(`Deleted “${h.name}”`);
    } catch (e) {
      toast(String(e), "error");
    }
    setConfirmDelete(null);
  };

  return (
    <>
      <div className="view-body">
        <div style={{ display: "flex", gap: 10, marginBottom: 16 }}>
          <input
            className="input"
            style={{ maxWidth: 320 }}
            placeholder="Search hosts, addresses, tags…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <div className="spacer" />
          <button className="btn btn-primary" onClick={() => setEditing(emptyHost())}>
            + Add Host
          </button>
        </div>

        {filtered.length === 0 ? (
          <div className="empty-state" style={{ height: "60%" }}>
            <div className="big">
              <ServerIcon />
            </div>
            <h3>{hosts.length === 0 ? "No hosts yet" : "No matches"}</h3>
            <p>
              {hosts.length === 0
                ? "Add your first server to connect, browse files, and forward ports."
                : "Try a different search."}
            </p>
            {hosts.length === 0 && (
              <button className="btn btn-primary" onClick={() => setEditing(emptyHost())}>
                + Add Host
              </button>
            )}
          </div>
        ) : (
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(300px, 1fr))",
              gap: 14,
            }}
          >
            {filtered.map((h) => {
              const session = sessionFor(h.id);
              const health = healthByHost[h.id];
              return (
                <div key={h.id} className="card hoverable" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 9 }}>
                    <span
                      className={`status-dot ${
                        session ? "ok" : health ? (health.reachable ? "ok" : "bad") : "unknown"
                      }`}
                    />
                    <strong style={{ fontSize: 14.5, letterSpacing: "-0.2px" }}>{h.name}</strong>
                    <div className="spacer" />
                    {session && <span className="pill success">connected</span>}
                  </div>
                  <div className="mono selectable" style={{ fontSize: 12.5, color: "var(--text-secondary)" }}>
                    {h.username}@{h.host}:{h.port}
                  </div>
                  {h.tags.length > 0 && (
                    <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
                      {h.tags.map((t) => (
                        <span key={t} className="pill neutral">
                          {t}
                        </span>
                      ))}
                    </div>
                  )}
                  <div style={{ display: "flex", gap: 6, marginTop: "auto" }}>
                    <button
                      className="btn btn-sm"
                      disabled={connecting === h.id}
                      onClick={() => openShell(h)}
                      title="Open terminal"
                    >
                      <TerminalIcon /> Shell
                    </button>
                    {session ? (
                      <button className="btn btn-sm btn-danger" onClick={() => disconnect(h.id)}>
                        Disconnect
                      </button>
                    ) : (
                      <button
                        className="btn btn-sm"
                        disabled={connecting === h.id}
                        onClick={() => connect(h)}
                      >
                        <PlugIcon /> {connecting === h.id ? "Connecting…" : "Connect"}
                      </button>
                    )}
                    <div className="spacer" />
                    <button className="btn btn-sm btn-ghost" onClick={() => setEditing(h)}>
                      Edit
                    </button>
                    <button
                      className="btn btn-sm btn-danger"
                      onClick={() => setConfirmDelete(h)}
                    >
                      Delete
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {editing && <HostEditor initial={editing} onDone={() => setEditing(null)} />}
      {keyPrompt && (
        <Modal
          title={keyPrompt.prompt.knownFingerprint ? "Host Key Changed" : "Verify Host Key"}
          onClose={() => setKeyPrompt(null)}
          footer={
            <>
              <button className="btn btn-ghost" onClick={() => setKeyPrompt(null)}>
                Cancel
              </button>
              <button
                className="btn"
                style={
                  keyPrompt.prompt.knownFingerprint
                    ? { background: "var(--danger)", color: "#fff" }
                    : undefined
                }
                disabled={trusting}
                onClick={trustHostKey}
              >
                {trusting
                  ? "Connecting…"
                  : keyPrompt.prompt.knownFingerprint
                    ? "Replace Key & Connect"
                    : "Trust & Connect"}
              </button>
            </>
          }
        >
          {keyPrompt.prompt.knownFingerprint ? (
            <p style={{ fontSize: 13.5, color: "var(--danger)", fontWeight: 600 }}>
              The key presented by {keyPrompt.prompt.host}:{keyPrompt.prompt.port} does not
              match the one pinned for this server. This can mean the server was reinstalled —
              or that the connection is being intercepted.
            </p>
          ) : (
            <p style={{ fontSize: 13.5 }}>
              First connection to{" "}
              <strong>
                {keyPrompt.prompt.host}:{keyPrompt.prompt.port}
              </strong>
              . Verify the key fingerprint against the one shown on the server (
              <span className="mono">ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub</span>) before
              trusting it.
            </p>
          )}
          <div
            className="mono selectable"
            style={{
              marginTop: 10,
              padding: "10px 12px",
              background: "var(--bg-input)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-sm)",
              fontSize: 12.5,
              wordBreak: "break-all",
            }}
          >
            {keyPrompt.prompt.keyType} {keyPrompt.prompt.fingerprint}
          </div>
          {keyPrompt.prompt.knownFingerprint && (
            <div
              style={{ marginTop: 8, fontSize: 12, color: "var(--text-secondary)" }}
              className="selectable"
            >
              Previously pinned:{" "}
              <span className="mono">{keyPrompt.prompt.knownFingerprint}</span>
            </div>
          )}
        </Modal>
      )}
      {confirmDelete && (
        <Modal
          title="Delete Host"
          onClose={() => setConfirmDelete(null)}
          footer={
            <>
              <button className="btn btn-ghost" onClick={() => setConfirmDelete(null)}>
                Cancel
              </button>
              <button
                className="btn"
                style={{ background: "var(--danger)", color: "#fff" }}
                onClick={() => doDelete(confirmDelete)}
              >
                Delete
              </button>
            </>
          }
        >
          <p style={{ fontSize: 13.5 }}>
            Delete <strong>{confirmDelete.name}</strong>? Its stored credentials will also be
            removed. This can’t be undone.
          </p>
        </Modal>
      )}
    </>
  );
}
