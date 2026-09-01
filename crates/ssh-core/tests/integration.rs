//! Integration tests: networking, terminals, credentials, SFTP, port
//! forwarding, health checks, and async jobs — all against an in-process
//! SSH server (russh server side), no system sshd required.

mod support;

use std::sync::Arc;
use std::time::Duration;

use ssh_core::hosts::HostStore;
use ssh_core::jobs::JobManager;
use ssh_core::known_hosts::{KnownHostKey, KnownHostsStore};
use ssh_core::model::*;
use ssh_core::session::{HostKeyVerifier, SshSession};
use ssh_core::terminal::{TermEvent, Terminal};
use ssh_core::vault::Vault;
use ssh_core::{health, sftp, CoreEvent, CoreError, Manager};
use support::TestServer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const USER: &str = "testuser";
const PASSWORD: &str = "sekret";

fn host_for(server: &TestServer) -> HostEntry {
    HostEntry {
        id: "test-host".into(),
        name: "Test Server".into(),
        host: "127.0.0.1".into(),
        port: server.port,
        username: USER.into(),
        auth_method: AuthMethod::Password,
        key_path: None,
        tags: vec![],
        notes: None,
        health_enabled: false,
        health_interval_secs: 60,
    }
}

async fn connect(server: &TestServer) -> SshSession {
    SshSession::connect(&host_for(server), Some(PASSWORD.into()), None)
        .await
        .expect("connect + auth should succeed")
}

/// Connects through the manager, trusting the host key on first use the way
/// the UI does (prompt -> retry with the prompted fingerprint).
async fn manager_connect(manager: &Arc<Manager>, host_id: &str) -> SessionInfo {
    match manager.connect_host(host_id, None).await.unwrap() {
        ConnectOutcome::Connected { session } => session,
        ConnectOutcome::HostKeyPrompt { prompt } => {
            match manager
                .connect_host(host_id, Some(prompt.fingerprint))
                .await
                .unwrap()
            {
                ConnectOutcome::Connected { session } => session,
                ConnectOutcome::HostKeyPrompt { .. } => {
                    panic!("still prompted after accepting the host key")
                }
            }
        }
    }
}

// ---------------- networking ----------------

#[tokio::test(flavor = "multi_thread")]
async fn networking_connect_and_exec() {
    let server = TestServer::start().await;
    let session = connect(&server).await;
    assert!(!session.is_closed());

    let (code, out) = session.exec("echo hello").await.unwrap();
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "hello");

    session.disconnect().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn networking_rejects_bad_password() {
    let server = TestServer::start().await;
    let mut host = host_for(&server);
    host.id = "bad".into();
    let Err(err) = SshSession::connect(&host, Some("wrong-password".into()), None).await else {
        panic!("wrong password must fail");
    };
    assert!(matches!(err, CoreError::AuthFailed(_)), "got: {err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn networking_missing_credential() {
    let server = TestServer::start().await;
    let Err(err) = SshSession::connect(&host_for(&server), None, None).await else {
        panic!("password auth without a secret must fail");
    };
    assert!(matches!(err, CoreError::MissingCredential));
}

#[tokio::test(flavor = "multi_thread")]
async fn networking_unreachable_host() {
    // Reserved TEST-NET-1 address: connect should fail or time out, not hang.
    let host = HostEntry {
        host: "127.0.0.1".into(),
        port: 1, // almost certainly closed
        ..host_for(&TestServer::start().await)
    };
    let result = SshSession::connect(&host, Some(PASSWORD.into()), None).await;
    assert!(result.is_err());
}

// ---------------- host key verification ----------------

#[tokio::test(flavor = "multi_thread")]
async fn host_key_tofu_pin_and_reconnect() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let manager = Manager::new(dir.path().to_path_buf()).unwrap();

    let mut host = host_for(&server);
    host.id = String::new();
    let saved = manager.save_host(host).unwrap();
    manager.set_secret(&saved.id, PASSWORD).unwrap();

    // First contact: must prompt, not connect.
    let ConnectOutcome::HostKeyPrompt { prompt } =
        manager.connect_host(&saved.id, None).await.unwrap()
    else {
        panic!("first connection must prompt for the host key");
    };
    assert_eq!(prompt.known_fingerprint, None);
    assert!(prompt.fingerprint.starts_with("SHA256:"), "{}", prompt.fingerprint);
    assert!(manager.list_known_hosts().is_empty());

    // Accepting the prompted fingerprint connects and pins the key.
    let ConnectOutcome::Connected { session } = manager
        .connect_host(&saved.id, Some(prompt.fingerprint.clone()))
        .await
        .unwrap()
    else {
        panic!("accepting the key must connect");
    };
    let pinned = manager.list_known_hosts();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].fingerprint, prompt.fingerprint);
    assert_eq!(pinned[0].port, server.port);
    manager.disconnect_session(&session.id).await.unwrap();

    // Reconnect: pinned key matches, no prompt.
    let ConnectOutcome::Connected { session } =
        manager.connect_host(&saved.id, None).await.unwrap()
    else {
        panic!("pinned host must connect without prompting");
    };
    manager.disconnect_session(&session.id).await.unwrap();

    // Forgetting the key brings the prompt back.
    manager.forget_known_host("127.0.0.1", server.port).unwrap();
    assert!(matches!(
        manager.connect_host(&saved.id, None).await.unwrap(),
        ConnectOutcome::HostKeyPrompt { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn host_key_mismatch_rejected() {
    let server = TestServer::start().await;
    let host = host_for(&server);

    // Learn the server's real fingerprint from the unknown-key rejection.
    let verifier = HostKeyVerifier::new(None, None);
    let Err(CoreError::HostKeyUnknown { fingerprint, .. }) =
        SshSession::connect(&host, Some(PASSWORD.into()), Some(verifier)).await
    else {
        panic!("unverified first contact must fail with HostKeyUnknown");
    };

    // A pinned key that doesn't match must abort before authentication.
    let stale = KnownHostKey {
        host: host.host.clone(),
        port: host.port,
        key_type: "ssh-ed25519".into(),
        key_base64: "AAAA-stale".into(),
        fingerprint: "SHA256:stale-pinned-fingerprint".into(),
        added_at: 0,
    };
    let verifier = HostKeyVerifier::new(Some(stale.clone()), None);
    let Err(CoreError::HostKeyMismatch {
        fingerprint: presented,
        expected,
        ..
    }) = SshSession::connect(&host, Some(PASSWORD.into()), Some(verifier)).await
    else {
        panic!("changed host key must fail with HostKeyMismatch");
    };
    assert_eq!(presented, fingerprint);
    assert_eq!(expected, stale.fingerprint);

    // Explicitly approving the new fingerprint lets the user replace the pin.
    let verifier = HostKeyVerifier::new(Some(stale), Some(fingerprint));
    let session = SshSession::connect(&host, Some(PASSWORD.into()), Some(verifier.clone()))
        .await
        .expect("approving the replacement key must connect");
    assert!(verifier.accepted_new());
    session.disconnect().await;
}

#[test]
fn host_key_store_persists() {
    let dir = tempfile::tempdir().unwrap();
    let store = KnownHostsStore::open(dir.path().to_path_buf()).unwrap();
    store
        .pin("example.com", 22, "ssh-ed25519", "AAAAtest", "SHA256:abc")
        .unwrap();

    let reopened = KnownHostsStore::open(dir.path().to_path_buf()).unwrap();
    let key = reopened.get("example.com", 22).expect("pinned key persists");
    assert_eq!(key.fingerprint, "SHA256:abc");
    assert!(reopened.get("example.com", 2222).is_none());

    // Re-pinning replaces, forgetting removes — both persisted.
    reopened
        .pin("example.com", 22, "ssh-ed25519", "AAAAnew", "SHA256:def")
        .unwrap();
    assert_eq!(reopened.list().len(), 1);
    reopened.forget("example.com", 22).unwrap();
    let after = KnownHostsStore::open(dir.path().to_path_buf()).unwrap();
    assert!(after.get("example.com", 22).is_none());
}

// ---------------- terminals ----------------

#[tokio::test(flavor = "multi_thread")]
async fn terminal_echo_roundtrip() {
    let server = TestServer::start().await;
    let session = connect(&server).await;

    let channel = session.handle.channel_open_session().await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let term = Terminal::open(channel, 80, 24, tx).await.unwrap();

    term.write(b"ls -la\n").await.unwrap();

    // The test server's fake shell echoes input back.
    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while received.len() < 7 {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timed out waiting for terminal echo")
            .expect("terminal event channel closed");
        if let TermEvent::Data(data) = event {
            received.extend_from_slice(&data);
        }
    }
    assert_eq!(&received[..7], b"ls -la\n");

    term.resize(120, 40).await.unwrap();
    term.close().await.unwrap();

    // Closing the channel must surface an Exit event.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timed out waiting for terminal exit");
        match event {
            Some(TermEvent::Exit) | None => break,
            Some(TermEvent::Data(_)) => continue,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_via_manager_events() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let manager = Manager::new(dir.path().to_path_buf()).unwrap();

    let mut host = host_for(&server);
    host.id = String::new();
    let saved = manager.save_host(host).unwrap();
    manager.set_secret(&saved.id, PASSWORD).unwrap();

    let info = manager_connect(&manager, &saved.id).await;
    let mut events = manager.subscribe();
    let term_id = manager.open_terminal(&info.id, 80, 24).await.unwrap();

    manager.term_write(&term_id, b"ping\n").await.unwrap();

    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while received.len() < 5 {
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("timed out waiting for TermData event")
            .expect("event bus closed");
        if let CoreEvent::TermData { term_id: id, data } = event {
            assert_eq!(id, term_id);
            received.extend_from_slice(&data);
        }
    }
    assert_eq!(&received[..5], b"ping\n");

    manager.close_terminal(&term_id).await.unwrap();
    manager.disconnect_session(&info.id).await.unwrap();
    assert!(manager.list_sessions().is_empty());
}

// ---------------- credentials ----------------

#[test]
fn credentials_roundtrip_and_encryption_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path().to_path_buf()).unwrap();

    vault.set("host-1", "hunter2-super-secret").unwrap();
    assert!(vault.contains("host-1"));
    assert_eq!(vault.get("host-1").unwrap().unwrap(), "hunter2-super-secret");
    assert_eq!(vault.get("nope").unwrap(), None);

    // The vault file must never contain the plaintext secret.
    let raw = std::fs::read_to_string(dir.path().join("vault.json")).unwrap();
    assert!(!raw.contains("hunter2"), "plaintext leaked to disk");

    // Master key file must be 0600.
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir.path().join("vault.key"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    // Re-opening with the persisted master key still decrypts.
    let reopened = Vault::open(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        reopened.get("host-1").unwrap().unwrap(),
        "hunter2-super-secret"
    );

    reopened.delete("host-1").unwrap();
    assert!(!reopened.contains("host-1"));
}

#[test]
fn credentials_wrong_key_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let vault_a = Vault::ephemeral(dir.path().to_path_buf(), b"key-a-key-a-key-a").unwrap();
    vault_a.set("h", "topsecret").unwrap();

    // Same ciphertext, different key: must error, not return garbage.
    let dir_b = tempfile::tempdir().unwrap();
    let vault_b = Vault::ephemeral(dir_b.path().to_path_buf(), b"key-b-key-b-key-b").unwrap();
    vault_b.set("h", "other").unwrap();
    std::fs::copy(
        dir.path().join("vault.json"),
        dir_b.path().join("vault.json"),
    )
    .unwrap();
    let vault_b = {
        // reload secrets from the copied file by reopening
        drop(vault_b);
        Vault::ephemeral_reload(dir_b.path().to_path_buf(), b"key-b-key-b-key-b").unwrap()
    };
    assert!(vault_b.get("h").is_err());
}

#[test]
fn credentials_host_store_crud() {
    let dir = tempfile::tempdir().unwrap();
    let store = HostStore::open(dir.path().to_path_buf()).unwrap();

    let saved = store
        .save(HostEntry {
            id: String::new(),
            name: "Box".into(),
            host: "example.com".into(),
            port: 22,
            username: "root".into(),
            auth_method: AuthMethod::Agent,
            key_path: None,
            tags: vec!["prod".into()],
            notes: None,
            health_enabled: false,
            health_interval_secs: 60,
        })
        .unwrap();
    assert!(!saved.id.is_empty());

    // Persistence across re-open.
    let store2 = HostStore::open(dir.path().to_path_buf()).unwrap();
    assert_eq!(store2.list().len(), 1);
    assert_eq!(store2.get(&saved.id).unwrap().name, "Box");

    // Validation.
    assert!(store2
        .save(HostEntry {
            port: 0,
            ..saved.clone()
        })
        .is_err());

    store2.delete(&saved.id).unwrap();
    assert!(store2.list().is_empty());
    assert!(store2.delete(&saved.id).is_err());
}

// ---------------- SFTP ----------------

#[tokio::test(flavor = "multi_thread")]
async fn sftp_browse_and_manage() {
    let server = TestServer::start().await;
    let session = connect(&server).await;
    let root = server.fs_root.path().to_string_lossy().into_owned();

    std::fs::write(format!("{root}/hello.txt"), b"hi there").unwrap();
    std::fs::create_dir(format!("{root}/subdir")).unwrap();

    let sftp_session = session.open_sftp().await.unwrap();
    let entries = sftp::list_dir(&sftp_session, &root).await.unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"hello.txt"));
    assert!(names.contains(&"subdir"));
    let file = entries.iter().find(|e| e.name == "hello.txt").unwrap();
    assert!(!file.is_dir);
    assert_eq!(file.size, 8);
    let dir = entries.iter().find(|e| e.name == "subdir").unwrap();
    assert!(dir.is_dir);

    sftp::mkdir(&sftp_session, &format!("{root}/made")).await.unwrap();
    assert!(std::fs::metadata(format!("{root}/made")).unwrap().is_dir());

    sftp::rename(
        &sftp_session,
        &format!("{root}/hello.txt"),
        &format!("{root}/renamed.txt"),
    )
    .await
    .unwrap();
    assert!(std::fs::metadata(format!("{root}/renamed.txt")).is_ok());

    sftp::delete(&sftp_session, &format!("{root}/renamed.txt"), false)
        .await
        .unwrap();
    assert!(std::fs::metadata(format!("{root}/renamed.txt")).is_err());

    // Recursive directory delete.
    std::fs::create_dir_all(format!("{root}/tree/nested")).unwrap();
    std::fs::write(format!("{root}/tree/a.txt"), b"a").unwrap();
    std::fs::write(format!("{root}/tree/nested/b.txt"), b"b").unwrap();
    sftp::delete(&sftp_session, &format!("{root}/tree"), true)
        .await
        .unwrap();
    assert!(std::fs::metadata(format!("{root}/tree")).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn sftp_transfers_as_jobs_with_progress() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let manager = Manager::new(dir.path().to_path_buf()).unwrap();

    let mut host = host_for(&server);
    host.id = String::new();
    let saved = manager.save_host(host).unwrap();
    manager.set_secret(&saved.id, PASSWORD).unwrap();
    let info = manager_connect(&manager, &saved.id).await;

    let root = server.fs_root.path().to_string_lossy().into_owned();
    let payload = vec![0xabu8; 300 * 1024];
    let local_src = dir.path().join("upload-src.bin");
    std::fs::write(&local_src, &payload).unwrap();

    // Upload.
    let mut events = manager.jobs.subscribe();
    let job = manager
        .sftp_upload(
            &info.id,
            local_src.to_str().unwrap(),
            &format!("{root}/uploaded.bin"),
        )
        .unwrap();
    let mut saw_progress = false;
    loop {
        let update = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("timed out waiting for upload job")
            .unwrap();
        if update.id != job.id {
            continue;
        }
        if update.progress.unwrap_or(0.0) > 0.0 {
            saw_progress = true;
        }
        match update.state {
            JobState::Done => break,
            JobState::Failed | JobState::Cancelled => {
                panic!("upload failed: {:?}", update.error)
            }
            _ => {}
        }
    }
    assert!(saw_progress, "upload job never reported progress");
    assert_eq!(
        std::fs::read(format!("{root}/uploaded.bin")).unwrap(),
        payload
    );

    // Download.
    let local_dst = dir.path().join("downloaded.bin");
    let job = manager
        .sftp_download(
            &info.id,
            &format!("{root}/uploaded.bin"),
            local_dst.to_str().unwrap(),
        )
        .unwrap();
    loop {
        let update = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("timed out waiting for download job")
            .unwrap();
        if update.id != job.id {
            continue;
        }
        match update.state {
            JobState::Done => break,
            JobState::Failed | JobState::Cancelled => {
                panic!("download failed: {:?}", update.error)
            }
            _ => {}
        }
    }
    assert_eq!(std::fs::read(&local_dst).unwrap(), payload);
}

// ---------------- port forwarding ----------------

/// Plain TCP echo server used as a forwarding target.
async fn spawn_echo_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn forward_local_tunnel() {
    let server = TestServer::start().await;
    let echo_port = spawn_echo_server().await;
    let session = Arc::new(connect(&server).await);

    let mut forward = ssh_core::forward::ActiveForward::start(
        session.clone(),
        ForwardSpec {
            kind: ForwardKind::Local,
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
            target_host: "127.0.0.1".into(),
            target_port: echo_port,
        },
    )
    .await
    .unwrap();
    let bound = forward.info.spec.bind_port;
    assert_ne!(bound, 0);

    let mut client = tokio::net::TcpStream::connect(("127.0.0.1", bound))
        .await
        .unwrap();
    client.write_all(b"through the tunnel").await.unwrap();
    let mut buf = [0u8; 18];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut buf))
        .await
        .expect("tunnel echo timed out")
        .unwrap();
    assert_eq!(&buf, b"through the tunnel");

    forward.stop().await;
    // Listener should be gone shortly after stop.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", bound))
            .await
            .is_err(),
        "listener still accepting after stop"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn forward_remote_tunnel() {
    let server = TestServer::start().await;
    let echo_port = spawn_echo_server().await;
    let session = Arc::new(connect(&server).await);

    let mut forward = ssh_core::forward::ActiveForward::start(
        session.clone(),
        ForwardSpec {
            kind: ForwardKind::Remote,
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
            target_host: "127.0.0.1".into(),
            target_port: echo_port,
        },
    )
    .await
    .unwrap();
    let remote_port = forward.info.spec.bind_port;
    assert_ne!(remote_port, 0);

    // Connect to the port the test server bound; data should round-trip
    // through the SSH connection to the local echo server.
    let mut client = tokio::net::TcpStream::connect(("127.0.0.1", remote_port))
        .await
        .unwrap();
    client.write_all(b"reverse!").await.unwrap();
    let mut buf = [0u8; 8];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut buf))
        .await
        .expect("remote tunnel echo timed out")
        .unwrap();
    assert_eq!(&buf, b"reverse!");

    forward.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn forward_dynamic_socks5() {
    let server = TestServer::start().await;
    let echo_port = spawn_echo_server().await;
    let session = Arc::new(connect(&server).await);

    let mut forward = ssh_core::forward::ActiveForward::start(
        session.clone(),
        ForwardSpec {
            kind: ForwardKind::Dynamic,
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
            target_host: String::new(),
            target_port: 0,
        },
    )
    .await
    .unwrap();
    let socks_port = forward.info.spec.bind_port;

    let mut client = tokio::net::TcpStream::connect(("127.0.0.1", socks_port))
        .await
        .unwrap();
    // SOCKS5 greeting, no-auth.
    client.write_all(&[5, 1, 0]).await.unwrap();
    let mut reply = [0u8; 2];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0]);
    // CONNECT 127.0.0.1:echo_port
    let mut req = vec![5, 1, 0, 1, 127, 0, 0, 1];
    req.extend_from_slice(&echo_port.to_be_bytes());
    client.write_all(&req).await.unwrap();
    let mut conn_reply = [0u8; 10];
    client.read_exact(&mut conn_reply).await.unwrap();
    assert_eq!(conn_reply[1], 0, "SOCKS connect failed");

    client.write_all(b"socks says hi").await.unwrap();
    let mut buf = [0u8; 13];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut buf))
        .await
        .expect("socks echo timed out")
        .unwrap();
    assert_eq!(&buf, b"socks says hi");

    forward.stop().await;
}

// ---------------- health ----------------

#[tokio::test(flavor = "multi_thread")]
async fn health_check_with_session_metrics() {
    let server = TestServer::start().await;
    let session = Arc::new(connect(&server).await);
    let host = host_for(&server);

    let report = health::check(&host, Some(session)).await;
    assert!(report.reachable);
    assert!(report.ssh_ok);
    assert!(report.latency_ms.is_some());
    assert_eq!(report.load_avg.as_deref(), Some("0.52 0.58 0.59"));
    let mem = report.mem_used_pct.unwrap();
    assert!((mem - 50.0).abs() < 1.0, "mem {mem}");
    assert_eq!(report.disk_used_pct, Some(42.0));
    assert!(report.uptime.unwrap().contains("up 5 days"));
}

#[tokio::test(flavor = "multi_thread")]
async fn health_check_unreachable() {
    let mut host = host_for(&TestServer::start().await);
    host.port = 1;
    let report = health::check_tcp(&host).await;
    assert!(!report.reachable);
    assert!(report.error.is_some());
}

#[test]
fn health_parse_probe_defensively() {
    let mut report = HealthReport {
        host_id: "h".into(),
        timestamp: 0,
        reachable: true,
        latency_ms: None,
        ssh_ok: true,
        uptime: None,
        load_avg: None,
        mem_used_pct: None,
        disk_used_pct: None,
        error: None,
    };
    // Garbage output should not panic or produce values.
    health::parse_probe("", &mut report);
    assert!(report.load_avg.is_none());
    health::parse_probe("@@@@@@", &mut report);
    assert!(report.mem_used_pct.is_none());
    health::parse_probe("up @@ x y @@ 0 0 @@ nope", &mut report);
    assert!(report.disk_used_pct.is_none());
}

// ---------------- async jobs ----------------

#[tokio::test(flavor = "multi_thread")]
async fn jobs_lifecycle_success_and_failure() {
    let jobs = JobManager::new();
    let mut events = jobs.subscribe();

    let job = jobs.spawn("test", "does things", |ctx| async move {
        for i in 1..=4 {
            ctx.progress(i as f64 / 4.0, Some(format!("step {i}")));
        }
        Ok(Some("all done".into()))
    });
    assert_eq!(job.state, JobState::Queued);

    let mut states = Vec::new();
    let mut max_progress = 0.0f64;
    loop {
        let update = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("job event timeout")
            .unwrap();
        if update.id != job.id {
            continue;
        }
        if let Some(p) = update.progress {
            max_progress = max_progress.max(p);
        }
        if !states.last().is_some_and(|s| *s == update.state) {
            states.push(update.state);
        }
        if update.state == JobState::Done {
            assert_eq!(update.detail.as_deref(), Some("all done"));
            assert!(update.finished_at.is_some());
            break;
        }
    }
    assert!(states.contains(&JobState::Running));
    assert!((max_progress - 1.0).abs() < f64::EPSILON);

    // Failure path.
    let failing = jobs.spawn("test", "explodes", |_ctx| async move {
        Err(ssh_core::CoreError::other("boom"))
    });
    loop {
        let update = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if update.id != failing.id {
            continue;
        }
        if update.state == JobState::Failed {
            assert!(update.error.unwrap().contains("boom"));
            break;
        }
    }

    let listed = jobs.list();
    assert_eq!(listed.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn jobs_cancellation() {
    let jobs = JobManager::new();
    let mut events = jobs.subscribe();

    let job = jobs.spawn("test", "sleeps forever", |ctx| async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if ctx.is_cancelled() {
                return Err(ssh_core::CoreError::other("cancelled"));
            }
        }
    });

    // Wait until running, then cancel.
    loop {
        let update = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if update.id == job.id && update.state == JobState::Running {
            break;
        }
    }
    jobs.cancel(&job.id).unwrap();

    loop {
        let update = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("cancel event timeout")
            .unwrap();
        if update.id == job.id && update.state == JobState::Cancelled {
            break;
        }
    }
    assert_eq!(jobs.get(&job.id).unwrap().state, JobState::Cancelled);
    assert!(jobs.cancel("no-such-job").is_err());
}
