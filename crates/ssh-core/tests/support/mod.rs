//! In-process SSH test server built on russh's server side.
//!
//! Capabilities:
//! - password auth (testuser / sekret)
//! - exec: `echo <x>` and the health probe command (canned output)
//! - pty/shell: echoes all input back (fake shell)
//! - sftp subsystem backed by a real temp directory
//! - direct-tcpip (for -L / SOCKS tests) and tcpip-forward (for -R tests)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use russh::keys::ssh_key;
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File as SftpFile, FileAttributes, Handle as SftpHandle, Name, OpenFlags, Status,
    StatusCode, Version,
};
use tokio::io::AsyncWriteExt;

const USER: &str = "testuser";
const PASSWORD: &str = "sekret";

const PROBE_OUTPUT: &str = " 12:00:00 up 5 days,  3:14,  2 users,  load average: 0.52, 0.58, 0.59\n@@\n0.52 0.58 0.59 1/389 12345\n@@\n16777216 8388608\n@@\n42%\n";

pub struct TestServer {
    pub port: u16,
    pub fs_root: tempfile::TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        let fs_root = tempfile::tempdir().unwrap();
        let root = fs_root.path().to_path_buf();

        let mut config = server::Config::default();
        config.inactivity_timeout = None;
        config.auth_rejection_time = std::time::Duration::from_millis(10);
        config.keys.push(
            russh::keys::PrivateKey::random(
                &mut russh::keys::key::safe_rng(),
                ssh_key::Algorithm::Ed25519,
            )
            .unwrap(),
        );
        let config = Arc::new(config);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let handler = ServerHandler {
                    fs_root: root.clone(),
                    channels: Arc::new(Mutex::new(HashMap::new())),
                    shell_channels: Arc::new(Mutex::new(Vec::new())),
                };
                let config = config.clone();
                tokio::spawn(async move {
                    let _ = server::run_stream(config, socket, handler)
                        .await
                        .map(|s| tokio::spawn(s));
                });
            }
        });

        Self { port, fs_root }
    }
}

struct ServerHandler {
    fs_root: std::path::PathBuf,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    shell_channels: Arc<Mutex<Vec<ChannelId>>>,
}

impl server::Handler for ServerHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == USER && password == PASSWORD {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().unwrap().insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shell_channels.lock().unwrap().push(channel);
        session.channel_success(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Fake shell: echo everything back.
        if self.shell_channels.lock().unwrap().contains(&channel) {
            session.data(channel, data.to_vec())?;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let cmd = String::from_utf8_lossy(data).into_owned();
        session.channel_success(channel)?;
        let output = if cmd.contains("uptime") {
            PROBE_OUTPUT.to_string()
        } else if let Some(rest) = cmd.strip_prefix("echo ") {
            format!("{rest}\n")
        } else {
            String::new()
        };
        session.data(channel, output.into_bytes())?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            let channel = self.channels.lock().unwrap().remove(&channel_id);
            if let Some(channel) = channel {
                session.channel_success(channel_id)?;
                let handler = SftpServer {
                    root: self.fs_root.clone(),
                    handles: HashMap::new(),
                    next_handle: 0,
                };
                russh_sftp::server::run(channel.into_stream(), handler).await;
                return Ok(());
            }
        }
        session.channel_failure(channel_id)?;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        match tokio::net::TcpStream::connect((host_to_connect, port_to_connect as u16)).await {
            Ok(mut stream) => {
                reply.accept().await;
                tokio::spawn(async move {
                    let mut ch = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut stream, &mut ch).await;
                });
            }
            Err(_) => {
                reply
                    .reject(russh::ChannelOpenFailure::ConnectFailed)
                    .await;
            }
        }
        Ok(())
    }

    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let listener = match tokio::net::TcpListener::bind((address, *port as u16)).await {
            Ok(l) => l,
            Err(_) => return Ok(false),
        };
        *port = listener.local_addr().unwrap().port() as u32;
        let bound_addr = address.to_string();
        let bound_port = *port;
        let handle = session.handle();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, peer)) = listener.accept().await else {
                    break;
                };
                let handle = handle.clone();
                let addr = bound_addr.clone();
                tokio::spawn(async move {
                    match handle
                        .channel_open_forwarded_tcpip(
                            addr,
                            bound_port,
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
        });
        Ok(true)
    }
}

// ---------------- SFTP backend over a real directory ----------------

enum FsHandle {
    Dir { entries: Vec<SftpFile>, sent: bool },
    File(std::fs::File),
}

struct SftpServer {
    root: std::path::PathBuf,
    handles: HashMap<String, FsHandle>,
    next_handle: u64,
}

impl SftpServer {
    fn alloc(&mut self, h: FsHandle) -> String {
        self.next_handle += 1;
        let id = format!("h{}", self.next_handle);
        self.handles.insert(id.clone(), h);
        id
    }

    fn resolve(&self, path: &str) -> std::path::PathBuf {
        if path.is_empty() || path == "." {
            self.root.clone()
        } else {
            std::path::PathBuf::from(path)
        }
    }
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".into(),
        language_tag: "en-US".into(),
    }
}

fn io_err(e: std::io::Error) -> StatusCode {
    match e.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

impl russh_sftp::server::Handler for SftpServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let resolved = self.resolve(&path);
        let canonical = std::fs::canonicalize(&resolved).unwrap_or(resolved);
        Ok(Name {
            id,
            files: vec![SftpFile::dummy(canonical.to_string_lossy().into_owned())],
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<SftpHandle, Self::Error> {
        let dir = self.resolve(&path);
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(io_err)? {
            let entry = entry.map_err(io_err)?;
            let meta = entry.metadata().map_err(io_err)?;
            entries.push(SftpFile::new(
                entry.file_name().to_string_lossy().into_owned(),
                FileAttributes::from(&meta),
            ));
        }
        let handle = self.alloc(FsHandle::Dir {
            entries,
            sent: false,
        });
        Ok(SftpHandle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        match self.handles.get_mut(&handle) {
            Some(FsHandle::Dir { entries, sent }) if !*sent => {
                *sent = true;
                Ok(Name {
                    id,
                    files: entries.clone(),
                })
            }
            _ => Err(StatusCode::Eof),
        }
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<SftpHandle, Self::Error> {
        let options: std::fs::OpenOptions = pflags.into();
        let file = options.open(self.resolve(&filename)).map_err(io_err)?;
        let handle = self.alloc(FsHandle::File(file));
        Ok(SftpHandle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        use std::io::{Read, Seek, SeekFrom};
        let Some(FsHandle::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset)).map_err(io_err)?;
        let mut buf = vec![0u8; len as usize];
        let n = file.read(&mut buf).map_err(io_err)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        buf.truncate(n);
        Ok(Data { id, data: buf })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        use std::io::{Seek, SeekFrom, Write};
        let Some(FsHandle::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset)).map_err(io_err)?;
        file.write_all(&data).map_err(io_err)?;
        Ok(ok_status(id))
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(ok_status(id))
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::metadata(self.resolve(&path)).map_err(io_err)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::symlink_metadata(self.resolve(&path)).map_err(io_err)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let Some(FsHandle::File(file)) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let meta = file.metadata().map_err(io_err)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.resolve(&path)).map_err(io_err)?;
        Ok(ok_status(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        std::fs::remove_dir(self.resolve(&path)).map_err(io_err)?;
        Ok(ok_status(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        std::fs::remove_file(self.resolve(&filename)).map_err(io_err)?;
        Ok(ok_status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        std::fs::rename(self.resolve(&oldpath), self.resolve(&newpath)).map_err(io_err)?;
        Ok(ok_status(id))
    }
}
