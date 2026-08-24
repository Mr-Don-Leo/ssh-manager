//! Top-level orchestrator the UI shell talks to.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh_sftp::client::SftpSession;
use tokio::sync::broadcast;
use tokio::sync::Mutex as AsyncMutex;

use crate::forward::ActiveForward;
use crate::hosts::HostStore;
use crate::jobs::JobManager;
use crate::model::*;
use crate::session::SshSession;
use crate::terminal::{TermEvent, Terminal};
use crate::vault::Vault;
use crate::{health, sftp, CoreError, Result};

const HEALTH_HISTORY_CAP: usize = 120;

#[derive(Debug, Clone)]
pub enum CoreEvent {
    TermData { term_id: String, data: Vec<u8> },
    TermExit { term_id: String },
    Job(JobInfo),
    Health(HealthReport),
    SessionClosed(String),
}

pub struct Manager {
    store: HostStore,
    vault: Vault,
    pub jobs: JobManager,
    sessions: Mutex<HashMap<String, Arc<SshSession>>>,
    sftp_cache: AsyncMutex<HashMap<String, Arc<SftpSession>>>,
    terminals: Mutex<HashMap<String, Arc<Terminal>>>,
    term_sessions: Mutex<HashMap<String, String>>,
    forwards: AsyncMutex<HashMap<String, ActiveForward>>,
    health_history: Mutex<HashMap<String, VecDeque<HealthReport>>>,
    health_tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    events: broadcast::Sender<CoreEvent>,
}

impl Manager {
    pub fn new(data_dir: PathBuf) -> Result<Arc<Self>> {
        let store = HostStore::open(data_dir.clone())?;
        let vault = Vault::open(data_dir)?;
        Self::with_parts(store, vault)
    }

    pub fn with_parts(store: HostStore, vault: Vault) -> Result<Arc<Self>> {
        let (events, _) = broadcast::channel(1024);
        let manager = Arc::new(Self {
            store,
            vault,
            jobs: JobManager::new(),
            sessions: Mutex::new(HashMap::new()),
            sftp_cache: AsyncMutex::new(HashMap::new()),
            terminals: Mutex::new(HashMap::new()),
            term_sessions: Mutex::new(HashMap::new()),
            forwards: AsyncMutex::new(HashMap::new()),
            health_history: Mutex::new(HashMap::new()),
            health_tasks: Mutex::new(HashMap::new()),
            events,
        });

        // Forward job updates onto the main event bus.
        {
            let manager = manager.clone();
            let mut rx = manager.jobs.subscribe();
            tokio::spawn(async move {
                while let Ok(job) = rx.recv().await {
                    let _ = manager.events.send(CoreEvent::Job(job));
                }
            });
        }

        // Resume monitors for hosts that have them enabled.
        for host in manager.store.list() {
            if host.health_enabled {
                manager.start_health_monitor(&host);
            }
        }
        Ok(manager)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    // ---- Hosts ----

    pub fn list_hosts(&self) -> Vec<HostEntry> {
        self.store.list()
    }

    pub fn save_host(self: &Arc<Self>, host: HostEntry) -> Result<HostEntry> {
        let saved = self.store.save(host)?;
        self.stop_health_monitor(&saved.id);
        if saved.health_enabled {
            self.start_health_monitor(&saved);
        }
        Ok(saved)
    }

    pub fn delete_host(&self, id: &str) -> Result<()> {
        self.stop_health_monitor(id);
        let _ = self.vault.delete(id);
        self.store.delete(id)
    }

    // ---- Credentials ----

    pub fn set_secret(&self, host_id: &str, secret: &str) -> Result<()> {
        self.store.get(host_id)?;
        self.vault.set(host_id, secret)
    }

    pub fn has_secret(&self, host_id: &str) -> bool {
        self.vault.contains(host_id)
    }

    pub fn delete_secret(&self, host_id: &str) -> Result<()> {
        self.vault.delete(host_id)
    }

    // ---- Sessions ----

    pub async fn connect_host(self: &Arc<Self>, host_id: &str) -> Result<SessionInfo> {
        let host = self.store.get(host_id)?;
        let secret = self.vault.get(host_id)?;
        let session = Arc::new(SshSession::connect(&host, secret).await?);
        let info = session.info.clone();
        self.sessions
            .lock()
            .unwrap()
            .insert(info.id.clone(), session);

        // Watch for the connection dropping out from under us.
        let manager = self.clone();
        let session_id = info.id.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let Some(session) = manager.get_session_opt(&session_id) else {
                    break;
                };
                if session.is_closed() {
                    manager.cleanup_session(&session_id).await;
                    break;
                }
            }
        });
        Ok(info)
    }

    fn get_session_opt(&self, session_id: &str) -> Option<Arc<SshSession>> {
        self.sessions.lock().unwrap().get(session_id).cloned()
    }

    pub fn get_session(&self, session_id: &str) -> Result<Arc<SshSession>> {
        self.get_session_opt(session_id)
            .ok_or_else(|| CoreError::SessionNotFound(session_id.to_string()))
    }

    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut sessions: Vec<SessionInfo> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| s.info.clone())
            .collect();
        sessions.sort_by(|a, b| a.connected_at.cmp(&b.connected_at));
        sessions
    }

    pub async fn disconnect_session(&self, session_id: &str) -> Result<()> {
        let session = self.get_session(session_id)?;
        session.disconnect().await;
        self.cleanup_session(session_id).await;
        Ok(())
    }

    async fn cleanup_session(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
        self.sftp_cache.lock().await.remove(session_id);

        let term_ids: Vec<String> = {
            let map = self.term_sessions.lock().unwrap();
            map.iter()
                .filter(|(_, sid)| sid.as_str() == session_id)
                .map(|(tid, _)| tid.clone())
                .collect()
        };
        for term_id in term_ids {
            self.terminals.lock().unwrap().remove(&term_id);
            self.term_sessions.lock().unwrap().remove(&term_id);
            let _ = self.events.send(CoreEvent::TermExit { term_id });
        }

        let mut forwards = self.forwards.lock().await;
        let ids: Vec<String> = forwards
            .iter()
            .filter(|(_, f)| f.info.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(mut fwd) = forwards.remove(&id) {
                fwd.stop().await;
            }
        }
        drop(forwards);

        let _ = self
            .events
            .send(CoreEvent::SessionClosed(session_id.to_string()));
    }

    /// Session for a host id, if one is currently open.
    pub fn session_for_host(&self, host_id: &str) -> Option<Arc<SshSession>> {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .find(|s| s.info.host_id == host_id)
            .cloned()
    }

    // ---- Terminals ----

    pub async fn open_terminal(
        self: &Arc<Self>,
        session_id: &str,
        cols: u32,
        rows: u32,
    ) -> Result<String> {
        let session = self.get_session(session_id)?;
        let channel = session.handle.channel_open_session().await?;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let terminal = Arc::new(Terminal::open(channel, cols, rows, tx).await?);
        let term_id = terminal.id.clone();

        self.terminals
            .lock()
            .unwrap()
            .insert(term_id.clone(), terminal);
        self.term_sessions
            .lock()
            .unwrap()
            .insert(term_id.clone(), session_id.to_string());

        let manager = self.clone();
        let id = term_id.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    TermEvent::Data(data) => {
                        let _ = manager.events.send(CoreEvent::TermData {
                            term_id: id.clone(),
                            data,
                        });
                    }
                    TermEvent::Exit => {
                        manager.terminals.lock().unwrap().remove(&id);
                        manager.term_sessions.lock().unwrap().remove(&id);
                        let _ = manager
                            .events
                            .send(CoreEvent::TermExit { term_id: id.clone() });
                        break;
                    }
                }
            }
        });
        Ok(term_id)
    }

    fn get_terminal(&self, term_id: &str) -> Result<Arc<Terminal>> {
        self.terminals
            .lock()
            .unwrap()
            .get(term_id)
            .cloned()
            .ok_or_else(|| CoreError::TerminalNotFound(term_id.to_string()))
    }

    pub async fn term_write(&self, term_id: &str, data: &[u8]) -> Result<()> {
        self.get_terminal(term_id)?.write(data).await
    }

    pub async fn term_resize(&self, term_id: &str, cols: u32, rows: u32) -> Result<()> {
        self.get_terminal(term_id)?.resize(cols, rows).await
    }

    pub async fn close_terminal(&self, term_id: &str) -> Result<()> {
        let terminal = {
            let mut terminals = self.terminals.lock().unwrap();
            self.term_sessions.lock().unwrap().remove(term_id);
            terminals
                .remove(term_id)
                .ok_or_else(|| CoreError::TerminalNotFound(term_id.to_string()))?
        };
        terminal.close().await
    }

    // ---- SFTP ----

    async fn sftp_for(&self, session_id: &str) -> Result<Arc<SftpSession>> {
        let mut cache = self.sftp_cache.lock().await;
        if let Some(sftp) = cache.get(session_id) {
            return Ok(sftp.clone());
        }
        let session = self.get_session(session_id)?;
        let sftp = Arc::new(session.open_sftp().await?);
        cache.insert(session_id.to_string(), sftp.clone());
        Ok(sftp)
    }

    pub async fn sftp_home(&self, session_id: &str) -> Result<String> {
        sftp::home(&*self.sftp_for(session_id).await?).await
    }

    pub async fn sftp_list(&self, session_id: &str, path: &str) -> Result<Vec<FileEntry>> {
        sftp::list_dir(&*self.sftp_for(session_id).await?, path).await
    }

    pub async fn sftp_mkdir(&self, session_id: &str, path: &str) -> Result<()> {
        sftp::mkdir(&*self.sftp_for(session_id).await?, path).await
    }

    pub async fn sftp_rename(&self, session_id: &str, from: &str, to: &str) -> Result<()> {
        sftp::rename(&*self.sftp_for(session_id).await?, from, to).await
    }

    pub async fn sftp_delete(&self, session_id: &str, path: &str, is_dir: bool) -> Result<()> {
        sftp::delete(&*self.sftp_for(session_id).await?, path, is_dir).await
    }

    pub fn sftp_download(
        self: &Arc<Self>,
        session_id: &str,
        remote: &str,
        local: &str,
    ) -> Result<JobInfo> {
        let session = self.get_session(session_id)?;
        let remote = remote.to_string();
        let local = local.to_string();
        let label = format!(
            "Download {}",
            remote.rsplit('/').next().unwrap_or(&remote)
        );
        Ok(self.jobs.spawn("sftp-download", &label, move |ctx| async move {
            // Dedicated SFTP session so long transfers don't contend with browsing.
            let sftp = session.open_sftp().await?;
            let bytes = sftp::download(&sftp, &remote, &local, &ctx).await?;
            Ok(Some(format!("{bytes} bytes → {local}")))
        }))
    }

    pub fn sftp_upload(
        self: &Arc<Self>,
        session_id: &str,
        local: &str,
        remote: &str,
    ) -> Result<JobInfo> {
        let session = self.get_session(session_id)?;
        let remote = remote.to_string();
        let local = local.to_string();
        let label = format!("Upload {}", local.rsplit('/').next().unwrap_or(&local));
        Ok(self.jobs.spawn("sftp-upload", &label, move |ctx| async move {
            let sftp = session.open_sftp().await?;
            let bytes = sftp::upload(&sftp, &local, &remote, &ctx).await?;
            Ok(Some(format!("{bytes} bytes → {remote}")))
        }))
    }

    // ---- Port forwarding ----

    pub async fn start_forward(&self, session_id: &str, spec: ForwardSpec) -> Result<ForwardInfo> {
        let session = self.get_session(session_id)?;
        let forward = ActiveForward::start(session, spec).await?;
        let info = forward.info.clone();
        self.forwards.lock().await.insert(info.id.clone(), forward);
        Ok(info)
    }

    pub async fn stop_forward(&self, forward_id: &str) -> Result<()> {
        let mut forwards = self.forwards.lock().await;
        let mut forward = forwards
            .remove(forward_id)
            .ok_or_else(|| CoreError::ForwardNotFound(forward_id.to_string()))?;
        forward.stop().await;
        Ok(())
    }

    pub async fn list_forwards(&self) -> Vec<ForwardInfo> {
        self.forwards
            .lock()
            .await
            .values()
            .map(|f| f.info.clone())
            .collect()
    }

    // ---- Health ----

    pub async fn run_health_check(self: &Arc<Self>, host_id: &str) -> Result<HealthReport> {
        let host = self.store.get(host_id)?;
        let session = self.session_for_host(host_id);
        let report = health::check(&host, session).await;
        self.record_health(report.clone());
        Ok(report)
    }

    fn record_health(&self, report: HealthReport) {
        {
            let mut history = self.health_history.lock().unwrap();
            let entry = history.entry(report.host_id.clone()).or_default();
            entry.push_back(report.clone());
            while entry.len() > HEALTH_HISTORY_CAP {
                entry.pop_front();
            }
        }
        let _ = self.events.send(CoreEvent::Health(report));
    }

    pub fn get_health_history(&self, host_id: &str) -> Vec<HealthReport> {
        self.health_history
            .lock()
            .unwrap()
            .get(host_id)
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn start_health_monitor(self: &Arc<Self>, host: &HostEntry) {
        let manager = self.clone();
        let host_id = host.id.clone();
        let interval = host.health_interval_secs.clamp(5, 24 * 3600);
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let Ok(host) = manager.store.get(&host_id) else {
                    break;
                };
                let session = manager.session_for_host(&host_id);
                let report = health::check(&host, session).await;
                manager.record_health(report);
            }
        });
        if let Some(old) = self
            .health_tasks
            .lock()
            .unwrap()
            .insert(host.id.clone(), task)
        {
            old.abort();
        }
    }

    fn stop_health_monitor(&self, host_id: &str) {
        if let Some(task) = self.health_tasks.lock().unwrap().remove(host_id) {
            task.abort();
        }
    }

    // ---- Settings ----

    pub fn get_settings(&self) -> AppSettings {
        self.store.load_settings()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        self.store.save_settings(settings)
    }
}
