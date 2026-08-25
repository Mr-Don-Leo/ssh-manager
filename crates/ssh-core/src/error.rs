use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ssh error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("sftp error: {0}")]
    Sftp(#[from] russh_sftp::client::error::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("host not found: {0}")]
    HostNotFound(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("terminal not found: {0}")]
    TerminalNotFound(String),
    #[error("forward not found: {0}")]
    ForwardNotFound(String),
    #[error("job not found: {0}")]
    JobNotFound(String),
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("credential vault error: {0}")]
    Vault(String),
    #[error("no stored credential for host")]
    MissingCredential,
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn other(msg: impl Into<String>) -> Self {
        CoreError::Other(msg.into())
    }
}
