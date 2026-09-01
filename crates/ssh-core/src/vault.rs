//! Encrypted credential vault.
//!
//! Secrets (passwords / key passphrases) are encrypted with XChaCha20-Poly1305.
//! The cipher key is derived (Argon2id) from a random 32-byte master key stored
//! next to the vault with 0600 permissions. This protects secrets from casual
//! reads and backup leakage of the vault file alone; an attacker with full
//! access to the user's home directory can recover the master key, which is the
//! same trust model as `~/.ssh` private keys.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;

use crate::{CoreError, Result};

const SALT: &[u8] = b"agentmux-ssh-vault-v1";

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct VaultFile {
    /// host id -> base64(nonce || ciphertext)
    secrets: HashMap<String, String>,
}

pub struct Vault {
    path: PathBuf,
    cipher: XChaCha20Poly1305,
    secrets: Mutex<HashMap<String, String>>,
}

fn derive_cipher(master: &[u8]) -> Result<XChaCha20Poly1305> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(master, SALT, &mut key)
        .map_err(|e| CoreError::Vault(e.to_string()))?;
    Ok(XChaCha20Poly1305::new((&key).into()))
}

impl Vault {
    /// Opens the vault in `dir`, creating the master key on first use.
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let key_path = dir.join("vault.key");
        let master = if key_path.exists() {
            std::fs::read(&key_path)?
        } else {
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            write_private(&key_path, &key)?;
            key
        };
        let cipher = derive_cipher(&master)?;

        let path = dir.join("vault.json");
        let secrets = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<VaultFile>(&raw)?.secrets
        } else {
            HashMap::new()
        };
        Ok(Self {
            path,
            cipher,
            secrets: Mutex::new(secrets),
        })
    }

    /// Vault with an explicit key and empty state (used by tests).
    pub fn ephemeral(dir: PathBuf, master: &[u8]) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join("vault.json"),
            cipher: derive_cipher(master)?,
            secrets: Mutex::new(HashMap::new()),
        })
    }

    /// Vault with an explicit key that loads an existing vault file (tests).
    pub fn ephemeral_reload(dir: PathBuf, master: &[u8]) -> Result<Self> {
        let path = dir.join("vault.json");
        let secrets = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<VaultFile>(&raw)?.secrets
        } else {
            HashMap::new()
        };
        Ok(Self {
            path,
            cipher: derive_cipher(master)?,
            secrets: Mutex::new(secrets),
        })
    }

    fn persist(&self, secrets: &HashMap<String, String>) -> Result<()> {
        let raw = serde_json::to_string_pretty(&VaultFile {
            secrets: secrets.clone(),
        })?;
        write_private(&self.path, raw.as_bytes())?;
        Ok(())
    }

    pub fn set(&self, host_id: &str, secret: &str) -> Result<()> {
        let mut nonce = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self
            .cipher
            .encrypt(XNonce::from_slice(&nonce), secret.as_bytes())
            .map_err(|_| CoreError::Vault("encryption failed".into()))?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ct);
        let mut secrets = self.secrets.lock().unwrap();
        secrets.insert(host_id.to_string(), B64.encode(blob));
        self.persist(&secrets)
    }

    pub fn get(&self, host_id: &str) -> Result<Option<String>> {
        let secrets = self.secrets.lock().unwrap();
        let Some(b64) = secrets.get(host_id) else {
            return Ok(None);
        };
        let blob = B64
            .decode(b64)
            .map_err(|_| CoreError::Vault("corrupt vault entry".into()))?;
        if blob.len() < 25 {
            return Err(CoreError::Vault("corrupt vault entry".into()));
        }
        let (nonce, ct) = blob.split_at(24);
        let pt = self
            .cipher
            .decrypt(XNonce::from_slice(nonce), ct)
            .map_err(|_| CoreError::Vault("decryption failed (wrong vault key?)".into()))?;
        Ok(Some(String::from_utf8(pt).map_err(|_| {
            CoreError::Vault("secret is not valid utf-8".into())
        })?))
    }

    pub fn contains(&self, host_id: &str) -> bool {
        self.secrets.lock().unwrap().contains_key(host_id)
    }

    pub fn delete(&self, host_id: &str) -> Result<()> {
        let mut secrets = self.secrets.lock().unwrap();
        secrets.remove(host_id);
        self.persist(&secrets)
    }
}

fn write_private(path: &PathBuf, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    // 0600 on Unix; on Windows the file inherits the user-profile ACL, which
    // is already private to the user.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(data)?;
    Ok(())
}
