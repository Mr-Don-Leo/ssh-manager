import { useEffect, useState } from "react";
import { useApp } from "../state/store";
import * as ipc from "../lib/ipc";
import type { KnownHostKey } from "../lib/types";
import { Segmented } from "../ui/primitives";

function KnownHostsCard() {
  const toast = useApp((s) => s.toast);
  const [keys, setKeys] = useState<KnownHostKey[]>([]);

  const refresh = () =>
    ipc
      .listKnownHosts()
      .then(setKeys)
      .catch(() => setKeys([]));

  useEffect(() => {
    refresh();
  }, []);

  const forget = async (key: KnownHostKey) => {
    try {
      await ipc.forgetKnownHost(key.host, key.port);
      await refresh();
      toast(`Forgot key for ${key.host}:${key.port}`);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <div className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <h3 style={{ fontSize: 14 }}>Known Hosts</h3>
      <p style={{ fontSize: 12, color: "var(--text-secondary)" }}>
        Server keys pinned on first connection. Connections are refused if a server's key
        later changes, until you approve the new one. Forget a key to be prompted again.
      </p>
      {keys.length === 0 ? (
        <p style={{ fontSize: 12.5, color: "var(--text-tertiary)" }}>No pinned keys yet.</p>
      ) : (
        keys.map((k) => (
          <div
            key={`${k.host}:${k.port}`}
            style={{ display: "flex", alignItems: "center", gap: 10 }}
          >
            <div style={{ minWidth: 0, flex: 1 }}>
              <div className="mono selectable" style={{ fontSize: 12.5 }}>
                {k.host}:{k.port}
              </div>
              <div
                className="mono selectable"
                style={{
                  fontSize: 11.5,
                  color: "var(--text-secondary)",
                  wordBreak: "break-all",
                }}
              >
                {k.keyType} {k.fingerprint}
              </div>
            </div>
            <button className="btn btn-sm btn-danger" onClick={() => forget(k)}>
              Forget
            </button>
          </div>
        ))
      )}
    </div>
  );
}

export default function SettingsView() {
  const settings = useApp((s) => s.settings);
  const applySettings = useApp((s) => s.applySettings);

  return (
    <div className="view-body">
      <div style={{ display: "flex", flexDirection: "column", gap: 14, maxWidth: 560 }}>
        <div className="card" style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <h3 style={{ fontSize: 14 }}>Appearance</h3>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
            <div>
              <div style={{ fontWeight: 600, fontSize: 13 }}>Theme</div>
              <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                Light/dark applies to the Apple skin; other skins pick their own.
              </div>
            </div>
            <Segmented
              value={settings.theme}
              options={[
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ]}
              onChange={(theme) => applySettings({ ...settings, theme })}
            />
          </div>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
            <div>
              <div style={{ fontWeight: 600, fontSize: 13 }}>Skin</div>
              <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                The whole app restyles through design tokens.
              </div>
            </div>
            <Segmented
              value={settings.skin}
              options={[
                { value: "apple", label: "Apple" },
                { value: "cyberpunk", label: "Cyberpunk" },
                { value: "xp", label: "XP" },
              ]}
              onChange={(skin) => applySettings({ ...settings, skin })}
            />
          </div>
        </div>

        <KnownHostsCard />

        <div className="card">
          <h3 style={{ fontSize: 14, marginBottom: 8 }}>About</h3>
          <p style={{ fontSize: 12.5, color: "var(--text-secondary)" }}>
            SSH Server Manager — store hosts, open terminals, browse files over SFTP, forward
            ports, and watch server health. Credentials are encrypted at rest with a local
            vault key.
          </p>
        </div>
      </div>
    </div>
  );
}
