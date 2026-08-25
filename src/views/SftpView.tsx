import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "../state/store";
import type { FileEntry } from "../lib/types";
import * as ipc from "../lib/ipc";
import { Dropdown, Field, Modal } from "../ui/primitives";
import { FileIcon, FolderIcon } from "../ui/icons";
import { fmtSize, fmtTime, parentOf } from "../lib/format";

export default function SftpView() {
  const sessions = useApp((s) => s.sessions);
  const toast = useApp((s) => s.toast);
  const setView = useApp((s) => s.setView);

  const [sessionId, setSessionId] = useState<string | null>(null);
  const [path, setPath] = useState<string>("/");
  const [pathInput, setPathInput] = useState<string>("/");
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [mkdirOpen, setMkdirOpen] = useState(false);
  const [mkdirName, setMkdirName] = useState("");
  const [renaming, setRenaming] = useState<FileEntry | null>(null);
  const [renameTo, setRenameTo] = useState("");
  const [deleting, setDeleting] = useState<FileEntry | null>(null);

  const session = sessions.find((s) => s.id === sessionId) ?? null;

  useEffect(() => {
    if (!sessionId && sessions.length > 0) setSessionId(sessions[0].id);
    if (sessionId && !sessions.some((s) => s.id === sessionId))
      setSessionId(sessions[0]?.id ?? null);
  }, [sessions, sessionId]);

  const load = useCallback(
    async (sid: string, p: string) => {
      setLoading(true);
      try {
        const list = await ipc.sftpList(sid, p);
        setEntries(list);
        setPath(p);
        setPathInput(p);
        setSelected(null);
      } catch (e) {
        toast(`Couldn’t open ${p}: ${e}`, "error");
      } finally {
        setLoading(false);
      }
    },
    [toast],
  );

  useEffect(() => {
    if (!sessionId) return;
    void (async () => {
      try {
        const home = await ipc.sftpHome(sessionId);
        await load(sessionId, home);
      } catch {
        await load(sessionId, "/");
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  const sorted = useMemo(
    () =>
      [...entries].sort((a, b) =>
        a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1,
      ),
    [entries],
  );

  const download = async (f: FileEntry) => {
    if (!sessionId) return;
    const local = await saveDialog({ defaultPath: f.name });
    if (!local) return;
    try {
      await ipc.sftpDownload(sessionId, f.path, local);
      toast(`Downloading ${f.name} — see Jobs`);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const upload = async () => {
    if (!sessionId) return;
    const local = await openDialog({ multiple: false });
    if (!local || typeof local !== "string") return;
    const name = local.split("/").pop() ?? "upload";
    const remote = path.replace(/\/+$/, "") + "/" + name;
    try {
      await ipc.sftpUpload(sessionId, local, remote);
      toast(`Uploading ${name} — see Jobs`);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  if (sessions.length === 0) {
    return (
      <div className="view-body">
        <div className="empty-state">
          <div className="big">
            <FolderIcon />
          </div>
          <h3>Not connected</h3>
          <p>Connect to a host to browse its files over SFTP.</p>
          <button className="btn btn-primary" onClick={() => setView("hosts")}>
            Go to Hosts
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="view-body no-pad" style={{ display: "flex", flexDirection: "column" }}>
      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "12px 16px",
          borderBottom: "1px solid var(--border)",
          alignItems: "center",
        }}
      >
        <div style={{ width: 220 }}>
          <Dropdown
            value={sessionId}
            options={sessions.map((s) => ({ value: s.id, label: s.hostName }))}
            onChange={setSessionId}
            placeholder="Choose connection…"
          />
        </div>
        <button
          className="btn btn-sm"
          disabled={!sessionId || path === "/"}
          onClick={() => sessionId && void load(sessionId, parentOf(path))}
          title="Up one level"
        >
          ↑ Up
        </button>
        <input
          className="input mono"
          style={{ flex: 1 }}
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && sessionId) void load(sessionId, pathInput.trim() || "/");
          }}
        />
        <button className="btn btn-sm" onClick={() => setMkdirOpen(true)} disabled={!sessionId}>
          New Folder
        </button>
        <button className="btn btn-sm btn-primary" onClick={upload} disabled={!sessionId}>
          Upload
        </button>
      </div>

      <div style={{ flex: 1, overflowY: "auto" }}>
        {loading ? (
          <div style={{ padding: 24, color: "var(--text-secondary)", fontSize: 13 }}>
            Loading{" "}
            <span className="typing-dots">
              <i />
              <i />
              <i />
            </span>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th style={{ width: "45%" }}>Name</th>
                <th>Size</th>
                <th>Modified</th>
                <th>Mode</th>
                <th style={{ width: 170 }}></th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((f) => (
                <tr
                  key={f.path}
                  onClick={() => setSelected(f.path)}
                  onDoubleClick={() => {
                    if (f.isDir && sessionId) void load(sessionId, f.path);
                  }}
                  style={
                    selected === f.path ? { background: "var(--accent-soft)" } : undefined
                  }
                >
                  <td>
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                      <span style={{ color: f.isDir ? "var(--accent)" : "var(--text-tertiary)" }}>
                        {f.isDir ? <FolderIcon /> : <FileIcon />}
                      </span>
                      <span className="selectable">{f.name}</span>
                      {f.isSymlink && <span className="pill neutral">link</span>}
                    </span>
                  </td>
                  <td className="mono" style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                    {f.isDir ? "—" : fmtSize(f.size)}
                  </td>
                  <td style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                    {fmtTime(f.modified)}
                  </td>
                  <td className="mono" style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                    {f.permissions ?? "—"}
                  </td>
                  <td>
                    <div style={{ display: "flex", gap: 4, justifyContent: "flex-end" }}>
                      {!f.isDir && (
                        <button className="btn btn-sm btn-ghost" onClick={() => download(f)}>
                          Download
                        </button>
                      )}
                      <button
                        className="btn btn-sm btn-ghost"
                        onClick={() => {
                          setRenaming(f);
                          setRenameTo(f.name);
                        }}
                      >
                        Rename
                      </button>
                      <button className="btn btn-sm btn-danger" onClick={() => setDeleting(f)}>
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
              {sorted.length === 0 && (
                <tr>
                  <td colSpan={5} style={{ color: "var(--text-tertiary)", padding: 20 }}>
                    Empty directory
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        )}
      </div>

      {mkdirOpen && session && (
        <Modal
          title="New Folder"
          onClose={() => setMkdirOpen(false)}
          footer={
            <>
              <button className="btn btn-ghost" onClick={() => setMkdirOpen(false)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={!mkdirName.trim()}
                onClick={async () => {
                  try {
                    await ipc.sftpMkdir(
                      session.id,
                      path.replace(/\/+$/, "") + "/" + mkdirName.trim(),
                    );
                    setMkdirOpen(false);
                    setMkdirName("");
                    void load(session.id, path);
                  } catch (e) {
                    toast(String(e), "error");
                  }
                }}
              >
                Create
              </button>
            </>
          }
        >
          <Field label="Folder Name">
            <input
              className="input"
              autoFocus
              value={mkdirName}
              onChange={(e) => setMkdirName(e.target.value)}
            />
          </Field>
        </Modal>
      )}

      {renaming && session && (
        <Modal
          title={`Rename “${renaming.name}”`}
          onClose={() => setRenaming(null)}
          footer={
            <>
              <button className="btn btn-ghost" onClick={() => setRenaming(null)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={!renameTo.trim()}
                onClick={async () => {
                  try {
                    await ipc.sftpRename(
                      session.id,
                      renaming.path,
                      parentOf(renaming.path).replace(/\/$/, "") + "/" + renameTo.trim(),
                    );
                    setRenaming(null);
                    void load(session.id, path);
                  } catch (e) {
                    toast(String(e), "error");
                  }
                }}
              >
                Rename
              </button>
            </>
          }
        >
          <Field label="New Name">
            <input
              className="input"
              autoFocus
              value={renameTo}
              onChange={(e) => setRenameTo(e.target.value)}
            />
          </Field>
        </Modal>
      )}

      {deleting && session && (
        <Modal
          title={deleting.isDir ? "Delete Folder" : "Delete File"}
          onClose={() => setDeleting(null)}
          footer={
            <>
              <button className="btn btn-ghost" onClick={() => setDeleting(null)}>
                Cancel
              </button>
              <button
                className="btn"
                style={{ background: "var(--danger)", color: "#fff" }}
                onClick={async () => {
                  try {
                    await ipc.sftpDelete(session.id, deleting.path, deleting.isDir);
                    setDeleting(null);
                    void load(session.id, path);
                  } catch (e) {
                    toast(String(e), "error");
                  }
                }}
              >
                Delete
              </button>
            </>
          }
        >
          <p style={{ fontSize: 13.5 }}>
            Delete <strong className="mono">{deleting.path}</strong>
            {deleting.isDir ? " and its contents?" : "?"} This can’t be undone.
          </p>
        </Modal>
      )}
    </div>
  );
}
