import { useApp } from "../state/store";
import { Segmented } from "../ui/primitives";

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
