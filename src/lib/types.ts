export type AuthMethod = "agent" | "password" | "key";

export interface HostEntry {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  keyPath?: string | null;
  tags: string[];
  notes?: string | null;
  healthEnabled: boolean;
  healthIntervalSecs: number;
}

export interface SessionInfo {
  id: string;
  hostId: string;
  hostName: string;
  connectedAt: number;
}

export interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  modified?: number | null;
  permissions?: string | null;
}

export type ForwardKind = "local" | "remote" | "dynamic";

export interface ForwardSpec {
  kind: ForwardKind;
  bindHost: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
}

export interface ForwardInfo {
  id: string;
  sessionId: string;
  hostName: string;
  spec: ForwardSpec;
  status: "active" | "error";
  error?: string | null;
}

export type JobState = "queued" | "running" | "done" | "failed" | "cancelled";

export interface JobInfo {
  id: string;
  kind: string;
  label: string;
  state: JobState;
  progress?: number | null;
  detail?: string | null;
  error?: string | null;
  createdAt: number;
  finishedAt?: number | null;
}

export interface HealthReport {
  hostId: string;
  timestamp: number;
  reachable: boolean;
  latencyMs?: number | null;
  sshOk: boolean;
  uptime?: string | null;
  loadAvg?: string | null;
  memUsedPct?: number | null;
  diskUsedPct?: number | null;
  error?: string | null;
}

export interface AppSettings {
  theme: "system" | "light" | "dark";
  skin: "apple" | "cyberpunk" | "xp";
}

export type ViewId =
  | "hosts"
  | "terminal"
  | "sftp"
  | "forwards"
  | "health"
  | "jobs"
  | "settings";
