//! Port forwarding: local (-L), remote (-R), and dynamic SOCKS5 (-D).

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::model::{ForwardInfo, ForwardKind, ForwardSpec};
use crate::session::SshSession;
use crate::{CoreError, Result};

pub struct ActiveForward {
    pub info: ForwardInfo,
    /// Listener task for local/dynamic forwards (aborted on stop).
    task: Option<JoinHandle<()>>,
    /// Set for remote forwards so we can cancel the server-side listener.
    remote_registration: Option<(String, u32)>,
    session: Arc<SshSession>,
}

impl ActiveForward {
    pub async fn start(session: Arc<SshSession>, spec: ForwardSpec) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut info = ForwardInfo {
            id,
            session_id: session.info.id.clone(),
            host_name: session.info.host_name.clone(),
            spec: spec.clone(),
            status: "active".into(),
            error: None,
        };

        match spec.kind {
            ForwardKind::Local => {
                let listener = TcpListener::bind((spec.bind_host.as_str(), spec.bind_port))
                    .await
                    .map_err(|e| {
                        CoreError::other(format!(
                            "couldn't bind {}:{}: {e}",
                            spec.bind_host, spec.bind_port
                        ))
                    })?;
                info.spec.bind_port = listener.local_addr()?.port();
                let task = tokio::spawn(local_forward_loop(session.clone(), listener, spec));
                Ok(Self {
                    info,
                    task: Some(task),
                    remote_registration: None,
                    session,
                })
            }
            ForwardKind::Remote => {
                session.remote_forwards.lock().unwrap().insert(
                    (spec.bind_host.clone(), spec.bind_port as u32),
                    (spec.target_host.clone(), spec.target_port),
                );
                let port = session
                    .handle
                    .tcpip_forward(spec.bind_host.clone(), spec.bind_port as u32)
                    .await
                    .map_err(|e| {
                        session
                            .remote_forwards
                            .lock()
                            .unwrap()
                            .remove(&(spec.bind_host.clone(), spec.bind_port as u32));
                        CoreError::other(format!("server refused remote forward: {e}"))
                    })?;
                // Server may allocate a port when we asked for 0.
                let actual = if spec.bind_port == 0 { port } else { spec.bind_port as u32 };
                if actual != spec.bind_port as u32 {
                    let mut forwards = session.remote_forwards.lock().unwrap();
                    let target = forwards
                        .remove(&(spec.bind_host.clone(), spec.bind_port as u32))
                        .unwrap_or((spec.target_host.clone(), spec.target_port));
                    forwards.insert((spec.bind_host.clone(), actual), target);
                    info.spec.bind_port = actual as u16;
                }
                Ok(Self {
                    info,
                    task: None,
                    remote_registration: Some((spec.bind_host.clone(), actual)),
                    session,
                })
            }
            ForwardKind::Dynamic => {
                let listener = TcpListener::bind((spec.bind_host.as_str(), spec.bind_port))
                    .await
                    .map_err(|e| {
                        CoreError::other(format!(
                            "couldn't bind {}:{}: {e}",
                            spec.bind_host, spec.bind_port
                        ))
                    })?;
                info.spec.bind_port = listener.local_addr()?.port();
                let task = tokio::spawn(socks5_loop(session.clone(), listener));
                Ok(Self {
                    info,
                    task: Some(task),
                    remote_registration: None,
                    session,
                })
            }
        }
    }

    pub async fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        if let Some((addr, port)) = self.remote_registration.take() {
            let _ = self.session.handle.cancel_tcpip_forward(addr.clone(), port).await;
            self.session
                .remote_forwards
                .lock()
                .unwrap()
                .remove(&(addr, port));
        }
    }
}

async fn local_forward_loop(session: Arc<SshSession>, listener: TcpListener, spec: ForwardSpec) {
    loop {
        let Ok((mut stream, peer)) = listener.accept().await else {
            break;
        };
        let session = session.clone();
        let target_host = spec.target_host.clone();
        let target_port = spec.target_port;
        tokio::spawn(async move {
            match session
                .handle
                .channel_open_direct_tcpip(
                    target_host,
                    target_port as u32,
                    peer.ip().to_string(),
                    peer.port() as u32,
                )
                .await
            {
                Ok(channel) => {
                    let mut ch = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut ch).await;
                }
                Err(_) => {
                    let _ = stream.shutdown().await;
                }
            }
        });
    }
}

/// Minimal SOCKS5 (RFC 1928): no auth, CONNECT only.
async fn socks5_loop(session: Arc<SshSession>, listener: TcpListener) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            break;
        };
        let session = session.clone();
        tokio::spawn(async move {
            let _ = socks5_handle(session, stream, peer).await;
        });
    }
}

async fn socks5_handle(
    session: Arc<SshSession>,
    mut stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
) -> Result<()> {
    // Greeting: VER NMETHODS METHODS...
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await?;
    if head[0] != 5 {
        return Err(CoreError::other("not SOCKS5"));
    }
    let mut methods = vec![0u8; head[1] as usize];
    stream.read_exact(&mut methods).await?;
    stream.write_all(&[5, 0]).await?; // NO AUTH

    // Request: VER CMD RSV ATYP ...
    let mut req = [0u8; 4];
    stream.read_exact(&mut req).await?;
    if req[1] != 1 {
        stream.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?; // command not supported
        return Err(CoreError::other("only CONNECT supported"));
    }
    let host = match req[3] {
        1 => {
            let mut a = [0u8; 4];
            stream.read_exact(&mut a).await?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            stream.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        4 => {
            let mut a = [0u8; 16];
            stream.read_exact(&mut a).await?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => {
            stream.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Err(CoreError::other("bad address type"));
        }
    };
    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);

    match session
        .handle
        .channel_open_direct_tcpip(host, port as u32, peer.ip().to_string(), peer.port() as u32)
        .await
    {
        Ok(channel) => {
            stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            let mut ch = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut ch).await;
            Ok(())
        }
        Err(e) => {
            stream.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?; // connection refused
            Err(CoreError::other(format!("tunnel failed: {e}")))
        }
    }
}
