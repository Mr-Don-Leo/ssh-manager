import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { useApp, type TermTab } from "../state/store";
import * as ipc from "../lib/ipc";
import { TerminalIcon } from "../ui/icons";
import { b64ToBytes, bytesToB64 } from "../lib/format";

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function TermPane({ tab, visible }: { tab: TermTab; visible: boolean }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const closeTermTab = useApp((s) => s.closeTermTab);
  const toast = useApp((s) => s.toast);
  const encoder = useRef(new TextEncoder());

  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;

    const term = new Terminal({
      fontFamily: cssVar("--font-mono") || "monospace",
      fontSize: 13,
      cursorBlink: true,
      allowProposedApi: true,
      theme: {
        background: cssVar("--terminal-bg"),
        foreground: cssVar("--terminal-fg"),
        cursor: cssVar("--accent"),
        selectionBackground: cssVar("--accent") + "50",
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(el);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    const unlistenData = ipc.onTermData(tab.termId, (b64) => {
      term.write(b64ToBytes(b64));
    });
    const unlistenExit = ipc.onTermExit(tab.termId, () => {
      term.write("\r\n\x1b[2m[session closed]\x1b[0m\r\n");
    });

    const dataDisp = term.onData((data) => {
      void ipc
        .termWrite(tab.termId, bytesToB64(encoder.current.encode(data)))
        .catch((e) => toast(String(e), "error"));
    });
    const resizeDisp = term.onResize(({ cols, rows }) => {
      void ipc.termResize(tab.termId, cols, rows).catch(() => {});
    });

    void ipc.termResize(tab.termId, term.cols, term.rows).catch(() => {});

    const ro = new ResizeObserver(() => {
      if (el.clientWidth > 0) fit.fit();
    });
    ro.observe(el);

    return () => {
      ro.disconnect();
      dataDisp.dispose();
      resizeDisp.dispose();
      void unlistenData.then((u) => u());
      void unlistenExit.then((u) => u());
      term.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.termId]);

  useEffect(() => {
    if (visible) {
      fitRef.current?.fit();
      termRef.current?.focus();
    }
  }, [visible]);

  // closeTermTab referenced to satisfy lint; close handled in strip
  void closeTermTab;

  return (
    <div
      ref={hostRef}
      style={{
        position: "absolute",
        inset: 0,
        padding: "8px 4px 4px 10px",
        display: visible ? "block" : "none",
        background: "var(--terminal-bg)",
      }}
      className="selectable"
    />
  );
}

export default function TerminalView() {
  const termTabs = useApp((s) => s.termTabs);
  const activeTermId = useApp((s) => s.activeTermId);
  const setActiveTerm = useApp((s) => s.setActiveTerm);
  const closeTermTab = useApp((s) => s.closeTermTab);
  const setView = useApp((s) => s.setView);

  const close = async (termId: string) => {
    closeTermTab(termId);
    try {
      await ipc.closeTerminal(termId);
    } catch {
      /* already gone */
    }
  };

  if (termTabs.length === 0) {
    return (
      <div className="view-body">
        <div className="empty-state">
          <div className="big">
            <TerminalIcon />
          </div>
          <h3>No open terminals</h3>
          <p>Open a shell from the Hosts view to start a session.</p>
          <button className="btn btn-primary" onClick={() => setView("hosts")}>
            Go to Hosts
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="view-body no-pad" style={{ display: "flex", flexDirection: "column" }}>
      <div className="tab-strip" style={{ borderBottom: "1px solid var(--border)" }}>
        {termTabs.map((t) => (
          <div
            key={t.termId}
            className={`tab${t.termId === activeTermId ? " active" : ""}`}
            onClick={() => setActiveTerm(t.termId)}
          >
            <TerminalIcon />
            {t.title}
            <button
              className="close"
              onClick={(e) => {
                e.stopPropagation();
                void close(t.termId);
              }}
              aria-label={`Close ${t.title}`}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <div style={{ position: "relative", flex: 1, minHeight: 0, background: "var(--terminal-bg)" }}>
        {termTabs.map((t) => (
          <TermPane key={t.termId} tab={t} visible={t.termId === activeTermId} />
        ))}
      </div>
    </div>
  );
}
