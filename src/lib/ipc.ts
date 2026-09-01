import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  ConnectOutcome,
  FileEntry,
  ForwardInfo,
  ForwardSpec,
  HealthReport,
  HostEntry,
  JobInfo,
  KnownHostKey,
  SessionInfo,
} from "./types";

// Hosts
export const listHosts = () => invoke<HostEntry[]>("list_hosts");
export const saveHost = (host: HostEntry) => invoke<HostEntry>("save_host", { host });
export const deleteHost = (id: string) => invoke<void>("delete_host", { id });

// Credentials
export const setSecret = (hostId: string, secret: string) =>
  invoke<void>("set_secret", { hostId, secret });
export const hasSecret = (hostId: string) => invoke<boolean>("has_secret", { hostId });
export const deleteSecret = (hostId: string) => invoke<void>("delete_secret", { hostId });

// Sessions
export const connectHost = (hostId: string, acceptFingerprint?: string) =>
  invoke<ConnectOutcome>("connect_host", { hostId, acceptFingerprint: acceptFingerprint ?? null });
export const listKnownHosts = () => invoke<KnownHostKey[]>("list_known_hosts");
export const forgetKnownHost = (host: string, port: number) =>
  invoke<void>("forget_known_host", { host, port });
export const disconnectSession = (sessionId: string) =>
  invoke<void>("disconnect_session", { sessionId });
export const listSessions = () => invoke<SessionInfo[]>("list_sessions");

// Terminal
export const openTerminal = (sessionId: string, cols: number, rows: number) =>
  invoke<string>("open_terminal", { sessionId, cols, rows });
export const termWrite = (termId: string, data: string) =>
  invoke<void>("term_write", { termId, data });
export const termResize = (termId: string, cols: number, rows: number) =>
  invoke<void>("term_resize", { termId, cols, rows });
export const closeTerminal = (termId: string) => invoke<void>("close_terminal", { termId });

// SFTP
export const sftpHome = (sessionId: string) => invoke<string>("sftp_home", { sessionId });
export const sftpList = (sessionId: string, path: string) =>
  invoke<FileEntry[]>("sftp_list", { sessionId, path });
export const sftpMkdir = (sessionId: string, path: string) =>
  invoke<void>("sftp_mkdir", { sessionId, path });
export const sftpRename = (sessionId: string, from: string, to: string) =>
  invoke<void>("sftp_rename", { sessionId, from, to });
export const sftpDelete = (sessionId: string, path: string, isDir: boolean) =>
  invoke<void>("sftp_delete", { sessionId, path, isDir });
export const sftpDownload = (sessionId: string, remote: string, local: string) =>
  invoke<string>("sftp_download", { sessionId, remote, local });
export const sftpUpload = (sessionId: string, local: string, remote: string) =>
  invoke<string>("sftp_upload", { sessionId, local, remote });

// Port forwarding
export const startForward = (sessionId: string, spec: ForwardSpec) =>
  invoke<ForwardInfo>("start_forward", { sessionId, spec });
export const stopForward = (forwardId: string) => invoke<void>("stop_forward", { forwardId });
export const listForwards = () => invoke<ForwardInfo[]>("list_forwards");

// Health
export const runHealthCheck = (hostId: string) =>
  invoke<HealthReport>("run_health_check", { hostId });
export const getHealthHistory = (hostId: string) =>
  invoke<HealthReport[]>("get_health_history", { hostId });

// Jobs
export const listJobs = () => invoke<JobInfo[]>("list_jobs");
export const cancelJob = (jobId: string) => invoke<void>("cancel_job", { jobId });

// Settings
export const getSettings = () => invoke<AppSettings>("get_settings");
export const setSettings = (settings: AppSettings) => invoke<void>("set_settings", { settings });

// Events
export const onTermData = (termId: string, cb: (b64: string) => void): Promise<UnlistenFn> =>
  listen<string>(`term-data-${termId}`, (e) => cb(e.payload));
export const onTermExit = (termId: string, cb: () => void): Promise<UnlistenFn> =>
  listen<null>(`term-exit-${termId}`, () => cb());
export const onJobUpdate = (cb: (job: JobInfo) => void): Promise<UnlistenFn> =>
  listen<JobInfo>("job-update", (e) => cb(e.payload));
export const onHealthUpdate = (cb: (r: HealthReport) => void): Promise<UnlistenFn> =>
  listen<HealthReport>("health-update", (e) => cb(e.payload));
export const onSessionClosed = (cb: (sessionId: string) => void): Promise<UnlistenFn> =>
  listen<string>("session-closed", (e) => cb(e.payload));
