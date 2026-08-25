//! Persistent host inventory + app settings, stored as JSON files.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::model::{AppSettings, HostEntry};
use crate::{CoreError, Result};

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct HostsFile {
    hosts: Vec<HostEntry>,
}

pub struct HostStore {
    dir: PathBuf,
    hosts: Mutex<Vec<HostEntry>>,
}

impl HostStore {
    /// Opens (or creates) the store rooted at `dir`.
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("hosts.json");
        let hosts = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<HostsFile>(&raw)?.hosts
        } else {
            Vec::new()
        };
        Ok(Self {
            dir,
            hosts: Mutex::new(hosts),
        })
    }

    fn persist(&self, hosts: &[HostEntry]) -> Result<()> {
        let path = self.dir.join("hosts.json");
        let tmp = self.dir.join("hosts.json.tmp");
        let raw = serde_json::to_string_pretty(&HostsFile {
            hosts: hosts.to_vec(),
        })?;
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<HostEntry> {
        self.hosts.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Result<HostEntry> {
        self.hosts
            .lock()
            .unwrap()
            .iter()
            .find(|h| h.id == id)
            .cloned()
            .ok_or_else(|| CoreError::HostNotFound(id.to_string()))
    }

    /// Inserts or updates; assigns an id to new entries. Returns the saved entry.
    pub fn save(&self, mut host: HostEntry) -> Result<HostEntry> {
        if host.name.trim().is_empty() || host.host.trim().is_empty() {
            return Err(CoreError::other("name and host are required"));
        }
        if host.port == 0 {
            return Err(CoreError::other("port must be between 1 and 65535"));
        }
        let mut hosts = self.hosts.lock().unwrap();
        if host.id.is_empty() {
            host.id = uuid::Uuid::new_v4().to_string();
            hosts.push(host.clone());
        } else if let Some(existing) = hosts.iter_mut().find(|h| h.id == host.id) {
            *existing = host.clone();
        } else {
            return Err(CoreError::HostNotFound(host.id));
        }
        self.persist(&hosts)?;
        Ok(host)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut hosts = self.hosts.lock().unwrap();
        let before = hosts.len();
        hosts.retain(|h| h.id != id);
        if hosts.len() == before {
            return Err(CoreError::HostNotFound(id.to_string()));
        }
        self.persist(&hosts)
    }

    pub fn load_settings(&self) -> AppSettings {
        let path = self.dir.join("settings.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let path = self.dir.join("settings.json");
        std::fs::write(path, serde_json::to_string_pretty(settings)?)?;
        Ok(())
    }
}
