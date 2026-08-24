import { create } from "zustand";
import type {
  AppSettings,
  ForwardInfo,
  HealthReport,
  HostEntry,
  JobInfo,
  SessionInfo,
  ViewId,
} from "../lib/types";
import * as ipc from "../lib/ipc";

export interface TermTab {
  termId: string;
  sessionId: string;
  title: string;
}

interface Toast {
  id: number;
  message: string;
  kind: "info" | "error";
}

interface AppState {
  view: ViewId;
  setView: (v: ViewId) => void;

  hosts: HostEntry[];
  sessions: SessionInfo[];
  forwards: ForwardInfo[];
  jobs: JobInfo[];
  healthByHost: Record<string, HealthReport>;

  termTabs: TermTab[];
  activeTermId: string | null;
  addTermTab: (tab: TermTab) => void;
  closeTermTab: (termId: string) => void;
  setActiveTerm: (termId: string) => void;

  settings: AppSettings;
  applySettings: (s: AppSettings, persist?: boolean) => void;

  toasts: Toast[];
  toast: (message: string, kind?: "info" | "error") => void;
  dismissToast: (id: number) => void;

  refreshHosts: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  refreshForwards: () => Promise<void>;
  refreshJobs: () => Promise<void>;
  upsertJob: (job: JobInfo) => void;
  setHealth: (r: HealthReport) => void;
  init: () => Promise<void>;
}

let toastSeq = 1;

function applyDom(settings: AppSettings) {
  const root = document.documentElement;
  root.dataset.skin = settings.skin;
  let theme = settings.theme;
  if (settings.skin === "cyberpunk") theme = "dark";
  else if (settings.skin === "xp") theme = "light";
  else if (theme === "system")
    theme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  root.dataset.theme = theme;
}

export const useApp = create<AppState>((set, get) => ({
  view: "hosts",
  setView: (view) => set({ view }),

  hosts: [],
  sessions: [],
  forwards: [],
  jobs: [],
  healthByHost: {},

  termTabs: [],
  activeTermId: null,
  addTermTab: (tab) =>
    set((s) => ({ termTabs: [...s.termTabs, tab], activeTermId: tab.termId, view: "terminal" })),
  closeTermTab: (termId) =>
    set((s) => {
      const termTabs = s.termTabs.filter((t) => t.termId !== termId);
      const activeTermId =
        s.activeTermId === termId ? (termTabs[termTabs.length - 1]?.termId ?? null) : s.activeTermId;
      return { termTabs, activeTermId };
    }),
  setActiveTerm: (activeTermId) => set({ activeTermId }),

  settings: { theme: "system", skin: "apple" },
  applySettings: (settings, persist = true) => {
    applyDom(settings);
    set({ settings });
    if (persist) void ipc.setSettings(settings).catch(() => {});
  },

  toasts: [],
  toast: (message, kind = "info") => {
    const id = toastSeq++;
    set((s) => ({ toasts: [...s.toasts, { id, message, kind }] }));
    setTimeout(() => get().dismissToast(id), kind === "error" ? 6500 : 3500);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  refreshHosts: async () => set({ hosts: await ipc.listHosts() }),
  refreshSessions: async () => set({ sessions: await ipc.listSessions() }),
  refreshForwards: async () => set({ forwards: await ipc.listForwards() }),
  refreshJobs: async () => set({ jobs: await ipc.listJobs() }),
  upsertJob: (job) =>
    set((s) => {
      const idx = s.jobs.findIndex((j) => j.id === job.id);
      const jobs = idx >= 0 ? s.jobs.map((j) => (j.id === job.id ? job : j)) : [job, ...s.jobs];
      return { jobs };
    }),
  setHealth: (r) => set((s) => ({ healthByHost: { ...s.healthByHost, [r.hostId]: r } })),

  init: async () => {
    try {
      const settings = await ipc.getSettings();
      applyDom(settings);
      set({ settings });
    } catch {
      applyDom(get().settings);
    }
    await Promise.all([
      get().refreshHosts().catch(() => {}),
      get().refreshSessions().catch(() => {}),
      get().refreshForwards().catch(() => {}),
      get().refreshJobs().catch(() => {}),
    ]);

    void ipc.onJobUpdate((job) => get().upsertJob(job));
    void ipc.onHealthUpdate((r) => get().setHealth(r));
    void ipc.onSessionClosed((sessionId) => {
      set((s) => ({
        sessions: s.sessions.filter((x) => x.id !== sessionId),
        forwards: s.forwards.filter((f) => f.sessionId !== sessionId),
      }));
    });
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", () => applyDom(get().settings));
  },
}));
