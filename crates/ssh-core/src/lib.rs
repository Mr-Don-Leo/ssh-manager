//! ssh-core: SSH host management, sessions, terminals, SFTP, port forwarding,
//! health checks, credentials, and async jobs. UI-agnostic; the Tauri shell
//! forwards `CoreEvent`s to the webview.

pub mod error;
pub mod forward;
pub mod health;
pub mod hosts;
pub mod jobs;
pub mod manager;
pub mod model;
pub mod session;
pub mod sftp;
pub mod terminal;
pub mod vault;

pub use error::CoreError;
pub use manager::{CoreEvent, Manager};
pub use model::*;

pub type Result<T> = std::result::Result<T, CoreError>;
