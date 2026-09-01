//! Pinned server host keys, stored as JSON alongside the host inventory.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::model::now_secs;
use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHostKey {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    /// Base64-encoded public key blob (the second field of an OpenSSH
    /// known_hosts line).
    pub key_base64: String,
    /// OpenSSH-style `SHA256:…` fingerprint.
    pub fingerprint: String,
    pub added_at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct KnownHostsFile {
    keys: Vec<KnownHostKey>,
}

pub struct KnownHostsStore {
    path: PathBuf,
    keys: Mutex<Vec<KnownHostKey>>,
}

impl KnownHostsStore {
    /// Opens (or creates) `known_hosts.json` under `dir`.
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("known_hosts.json");
        let keys = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<KnownHostsFile>(&raw)?.keys
        } else {
            Vec::new()
        };
        Ok(Self {
            path,
            keys: Mutex::new(keys),
        })
    }

    fn persist(&self, keys: &[KnownHostKey]) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        let raw = serde_json::to_string_pretty(&KnownHostsFile { keys: keys.to_vec() })?;
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn get(&self, host: &str, port: u16) -> Option<KnownHostKey> {
        self.keys
            .lock()
            .unwrap()
            .iter()
            .find(|k| k.host == host && k.port == port)
            .cloned()
    }

    pub fn list(&self) -> Vec<KnownHostKey> {
        self.keys.lock().unwrap().clone()
    }

    /// Pins (or replaces) the key for `host:port`.
    pub fn pin(
        &self,
        host: &str,
        port: u16,
        key_type: &str,
        key_base64: &str,
        fingerprint: &str,
    ) -> Result<()> {
        let entry = KnownHostKey {
            host: host.to_string(),
            port,
            key_type: key_type.to_string(),
            key_base64: key_base64.to_string(),
            fingerprint: fingerprint.to_string(),
            added_at: now_secs(),
        };
        let mut keys = self.keys.lock().unwrap();
        keys.retain(|k| !(k.host == host && k.port == port));
        keys.push(entry);
        self.persist(&keys)
    }

    /// Removes the pinned key for `host:port`; no-op if absent.
    pub fn forget(&self, host: &str, port: u16) -> Result<()> {
        let mut keys = self.keys.lock().unwrap();
        keys.retain(|k| !(k.host == host && k.port == port));
        self.persist(&keys)
    }
}
