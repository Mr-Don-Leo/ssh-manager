//! SSH client session: connect + authenticate, shared handle for channels.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, Handle};
use russh::keys::ssh_key;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg};
use tokio::io::AsyncWriteExt;

use crate::known_hosts::KnownHostKey;
use crate::model::{now_secs, AuthMethod, HostEntry, SessionInfo};
use crate::{CoreError, Result};

/// Registered remote (-R) forwards: (bind_addr, bind_port) -> (target_host, target_port).
pub type RemoteForwards = Arc<Mutex<HashMap<(String, u32), (String, u16)>>>;

/// The server key actually presented during key exchange.
#[derive(Debug, Clone)]
pub struct HostKeyObservation {
    pub key_type: String,
    pub key_base64: String,
    pub fingerprint: String,
}

/// Host key policy for one connection attempt. The key is accepted when it
/// matches the pinned `expected` key, or when its fingerprint equals
/// `accept_fingerprint` (the fingerprint the user explicitly approved in a
/// trust prompt). Anything else aborts the handshake before authentication.
#[derive(Clone, Default)]
pub struct HostKeyVerifier {
    pub expected: Option<KnownHostKey>,
    pub accept_fingerprint: Option<String>,
    observed: Arc<Mutex<Option<HostKeyObservation>>>,
    accepted_new: Arc<Mutex<bool>>,
}

impl HostKeyVerifier {
    pub fn new(expected: Option<KnownHostKey>, accept_fingerprint: Option<String>) -> Self {
        Self {
            expected,
            accept_fingerprint,
            observed: Arc::new(Mutex::new(None)),
            accepted_new: Arc::new(Mutex::new(false)),
        }
    }

    /// The key the server presented, once key exchange has run.
    pub fn observed(&self) -> Option<HostKeyObservation> {
        self.observed.lock().unwrap().clone()
    }

    /// True when a key not matching the pin was accepted via
    /// `accept_fingerprint` and should now be (re)pinned.
    pub fn accepted_new(&self) -> bool {
        *self.accepted_new.lock().unwrap()
    }
}

fn observe_key(key: &russh::keys::PublicKeyOrCertificate) -> HostKeyObservation {
    let public: ssh_key::PublicKey = match key {
        russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
        russh::keys::PublicKeyOrCertificate::Certificate(cert) => {
            cert.public_key().clone().into()
        }
    };
    let openssh = public.to_openssh().unwrap_or_default();
    let key_base64 = openssh
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    HostKeyObservation {
        key_type: public.algorithm().to_string(),
        key_base64,
        fingerprint: public.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
    }
}

pub struct ClientHandler {
    remote_forwards: RemoteForwards,
    verifier: Option<HostKeyVerifier>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let Some(verifier) = &self.verifier else {
            // No policy (tests/tools): accept anything.
            return Ok(true);
        };
        let observed = observe_key(server_public_key);
        let ok = match &verifier.expected {
            Some(expected) if expected.fingerprint == observed.fingerprint => true,
            _ => {
                // Unknown key, or pinned key changed: only proceed if the user
                // already approved exactly this fingerprint.
                let approved =
                    verifier.accept_fingerprint.as_deref() == Some(observed.fingerprint.as_str());
                if approved {
                    *verifier.accepted_new.lock().unwrap() = true;
                }
                approved
            }
        };
        *verifier.observed.lock().unwrap() = Some(observed);
        Ok(ok)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> std::result::Result<(), Self::Error> {
        let target = {
            let forwards = self.remote_forwards.lock().unwrap();
            forwards
                .get(&(connected_address.to_string(), connected_port))
                .cloned()
                // Fall back to any registration on the same port (server may
                // report a different address string than we bound).
                .or_else(|| {
                    forwards
                        .iter()
                        .find(|((_, p), _)| *p == connected_port)
                        .map(|(_, t)| t.clone())
                })
        };
        let Some((host, port)) = target else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            match tokio::net::TcpStream::connect((host.as_str(), port)).await {
                Ok(mut stream) => {
                    let mut ch = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut ch).await;
                }
                Err(_) => {
                    let _ = channel.close().await;
                }
            }
        });
        Ok(())
    }
}

pub struct SshSession {
    pub info: SessionInfo,
    pub handle: Arc<Handle<ClientHandler>>,
    pub remote_forwards: RemoteForwards,
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

fn describe_failure(result: &AuthResult) -> String {
    match result {
        AuthResult::Success => "success".into(),
        AuthResult::Failure {
            remaining_methods, ..
        } => {
            let methods: Vec<String> =
                remaining_methods.iter().map(|m| format!("{m:?}")).collect();
            if methods.is_empty() {
                "server rejected all methods".into()
            } else {
                format!("server accepts: {}", methods.join(", "))
            }
        }
    }
}

impl SshSession {
    /// Connects to `host` and authenticates. `secret` is the stored password or
    /// key passphrase, if any. With a `verifier`, the server's host key is
    /// checked against the pinned key before authentication; `None` accepts any
    /// key (tests/tools only).
    pub async fn connect(
        host: &HostEntry,
        secret: Option<String>,
        verifier: Option<HostKeyVerifier>,
    ) -> Result<Self> {
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(20)),
            ..Default::default()
        });
        let remote_forwards: RemoteForwards = Arc::new(Mutex::new(HashMap::new()));
        let handler = ClientHandler {
            remote_forwards: remote_forwards.clone(),
            verifier: verifier.clone(),
        };
        let connected = tokio::time::timeout(
            Duration::from_secs(20),
            client::connect(config, (host.host.as_str(), host.port), handler),
        )
        .await
        .map_err(|_| CoreError::other(format!("timed out connecting to {}", host.host)))?;

        let mut handle = match connected {
            Ok(handle) => handle,
            Err(russh::Error::UnknownKey) => {
                // Our check_server_key said no: turn the generic russh error
                // into a rich one carrying what the server presented.
                let observed = verifier.as_ref().and_then(|v| v.observed());
                let Some(observed) = observed else {
                    return Err(CoreError::Ssh(russh::Error::UnknownKey));
                };
                let expected = verifier.as_ref().and_then(|v| v.expected.clone());
                return Err(match expected {
                    Some(expected) => CoreError::HostKeyMismatch {
                        host: format!("{}:{}", host.host, host.port),
                        key_type: observed.key_type,
                        fingerprint: observed.fingerprint,
                        expected: expected.fingerprint,
                    },
                    None => CoreError::HostKeyUnknown {
                        host: format!("{}:{}", host.host, host.port),
                        key_type: observed.key_type,
                        fingerprint: observed.fingerprint,
                    },
                });
            }
            Err(e) => return Err(e.into()),
        };

        let user = host.username.clone();
        let result = match host.auth_method {
            AuthMethod::Password => {
                let password = secret.ok_or(CoreError::MissingCredential)?;
                handle.authenticate_password(user, password).await?
            }
            AuthMethod::Key => {
                let path = host
                    .key_path
                    .clone()
                    .ok_or_else(|| CoreError::other("no key path configured"))?;
                let key = load_secret_key(expand_tilde(&path), secret.as_deref())
                    .map_err(|e| CoreError::AuthFailed(format!("couldn't load key: {e}")))?;
                let hash_alg: Option<HashAlg> =
                    handle.best_supported_rsa_hash().await?.flatten();
                handle
                    .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
                    .await?
            }
            AuthMethod::Agent => Self::auth_agent(&mut handle, &user).await?,
        };

        if !result.success() {
            return Err(CoreError::AuthFailed(describe_failure(&result)));
        }

        Ok(Self {
            info: SessionInfo {
                id: uuid::Uuid::new_v4().to_string(),
                host_id: host.id.clone(),
                host_name: host.name.clone(),
                connected_at: now_secs(),
            },
            handle: Arc::new(handle),
            remote_forwards,
        })
    }

    #[cfg(unix)]
    async fn auth_agent(handle: &mut Handle<ClientHandler>, user: &str) -> Result<AuthResult> {
        let mut agent = russh::keys::agent::client::AgentClient::connect_env()
            .await
            .map_err(|e| CoreError::AuthFailed(format!("ssh-agent unavailable: {e}")))?;
        Self::auth_agent_identities(handle, user, &mut agent).await
    }

    #[cfg(windows)]
    async fn auth_agent(handle: &mut Handle<ClientHandler>, user: &str) -> Result<AuthResult> {
        // The OpenSSH for Windows agent listens on a well-known named pipe.
        let mut agent = russh::keys::agent::client::AgentClient::connect_named_pipe(
            r"\\.\pipe\openssh-ssh-agent",
        )
        .await
        .map_err(|e| CoreError::AuthFailed(format!("ssh-agent unavailable: {e}")))?;
        Self::auth_agent_identities(handle, user, &mut agent).await
    }

    async fn auth_agent_identities<S>(
        handle: &mut Handle<ClientHandler>,
        user: &str,
        agent: &mut russh::keys::agent::client::AgentClient<S>,
    ) -> Result<AuthResult>
    where
        S: russh::keys::agent::client::AgentStream + Unpin + Send,
    {
        let identities = agent
            .request_identities()
            .await
            .map_err(|e| CoreError::AuthFailed(format!("ssh-agent error: {e}")))?;
        if identities.is_empty() {
            return Err(CoreError::AuthFailed("ssh-agent has no identities".into()));
        }
        let mut last = None;
        for identity in identities {
            let key = identity.public_key().into_owned();
            let result = handle
                .authenticate_publickey_with(user, key, None, &mut *agent)
                .await
                .map_err(|e| CoreError::AuthFailed(format!("agent auth error: {e}")))?;
            if result.success() {
                return Ok(result);
            }
            last = Some(result);
        }
        Ok(last.unwrap_or(AuthResult::Failure {
            remaining_methods: russh::MethodSet::empty(),
            partial_success: false,
        }))
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .disconnect(russh::Disconnect::ByApplication, "bye", "en")
            .await;
    }

    /// Runs a command and returns (exit_code, combined stdout).
    pub async fn exec(&self, command: &str) -> Result<(u32, String)> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        let mut out = Vec::new();
        let mut code = 0u32;
        loop {
            let Some(msg) = channel.wait().await else { break };
            match msg {
                russh::ChannelMsg::Data { ref data } => out.extend_from_slice(data),
                russh::ChannelMsg::ExtendedData { ref data, .. } => out.extend_from_slice(data),
                russh::ChannelMsg::ExitStatus { exit_status } => code = exit_status,
                _ => {}
            }
        }
        Ok((code, String::from_utf8_lossy(&out).into_owned()))
    }

    /// Opens an SFTP subsystem session.
    pub async fn open_sftp(&self) -> Result<russh_sftp::client::SftpSession> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream()).await?;
        Ok(sftp)
    }

    /// Writes then flushes raw bytes to a stream (helper for tests/tools).
    pub async fn write_stream<W: tokio::io::AsyncWrite + Unpin>(
        w: &mut W,
        data: &[u8],
    ) -> Result<()> {
        w.write_all(data).await?;
        w.flush().await?;
        Ok(())
    }
}
