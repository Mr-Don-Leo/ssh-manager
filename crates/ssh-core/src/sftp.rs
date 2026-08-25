//! SFTP browsing and transfers (transfers report progress via `JobCtx`).

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::jobs::JobCtx;
use crate::model::FileEntry;
use crate::{CoreError, Result};

const CHUNK: usize = 64 * 1024;

fn join_path(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), name)
    }
}

pub async fn home(sftp: &SftpSession) -> Result<String> {
    Ok(sftp.canonicalize(".").await?)
}

pub async fn list_dir(sftp: &SftpSession, path: &str) -> Result<Vec<FileEntry>> {
    let canonical = sftp.canonicalize(path).await?;
    let entries = sftp.read_dir(&canonical).await?;
    let mut out = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        let meta = entry.metadata();
        let is_symlink = meta.is_symlink();
        // For symlinks, stat the target so directories-behind-links browse correctly.
        let (is_dir, size) = if is_symlink {
            match sftp.metadata(join_path(&canonical, &name)).await {
                Ok(m) => (m.is_dir(), m.len()),
                Err(_) => (false, 0),
            }
        } else {
            (meta.is_dir(), meta.len())
        };
        out.push(FileEntry {
            path: join_path(&canonical, &name),
            name,
            is_dir,
            is_symlink,
            size,
            modified: meta.mtime.map(|t| t as u64),
            permissions: Some(meta.permissions().to_string()),
        });
    }
    Ok(out)
}

pub async fn mkdir(sftp: &SftpSession, path: &str) -> Result<()> {
    Ok(sftp.create_dir(path).await?)
}

pub async fn rename(sftp: &SftpSession, from: &str, to: &str) -> Result<()> {
    Ok(sftp.rename(from, to).await?)
}

pub async fn delete(sftp: &SftpSession, path: &str, is_dir: bool) -> Result<()> {
    if is_dir {
        remove_dir_recursive(sftp, path).await
    } else {
        Ok(sftp.remove_file(path).await?)
    }
}

async fn remove_dir_recursive(sftp: &SftpSession, path: &str) -> Result<()> {
    // Iterative DFS to avoid async recursion boxing.
    let mut stack = vec![path.to_string()];
    let mut dirs = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in sftp.read_dir(&dir).await? {
            let child = join_path(&dir, &entry.file_name());
            if entry.metadata().is_dir() {
                stack.push(child);
            } else {
                sftp.remove_file(child).await?;
            }
        }
        dirs.push(dir);
    }
    for dir in dirs.into_iter().rev() {
        sftp.remove_dir(dir).await?;
    }
    Ok(())
}

/// Downloads `remote` to `local`, reporting progress on `ctx`.
pub async fn download(sftp: &SftpSession, remote: &str, local: &str, ctx: &JobCtx) -> Result<u64> {
    let total = sftp.metadata(remote).await?.size.unwrap_or(0);
    let mut src = sftp.open_with_flags(remote, OpenFlags::READ).await?;
    let mut dst = tokio::fs::File::create(local).await?;
    let mut buf = vec![0u8; CHUNK];
    let mut done: u64 = 0;
    loop {
        if ctx.is_cancelled() {
            return Err(CoreError::other("cancelled"));
        }
        let n = src.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await?;
        done += n as u64;
        if total > 0 {
            ctx.progress(done as f64 / total as f64, None);
        }
    }
    dst.flush().await?;
    Ok(done)
}

/// Uploads `local` to `remote`, reporting progress on `ctx`.
pub async fn upload(sftp: &SftpSession, local: &str, remote: &str, ctx: &JobCtx) -> Result<u64> {
    let mut src = tokio::fs::File::open(local).await?;
    let total = src.metadata().await?.len();
    let mut dst = sftp
        .open_with_flags(
            remote,
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
        )
        .await?;
    let mut buf = vec![0u8; CHUNK];
    let mut done: u64 = 0;
    loop {
        if ctx.is_cancelled() {
            return Err(CoreError::other("cancelled"));
        }
        let n = src.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await?;
        done += n as u64;
        if total > 0 {
            ctx.progress(done as f64 / total as f64, None);
        }
    }
    dst.flush().await?;
    dst.shutdown().await?;
    Ok(done)
}
