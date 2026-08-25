//! Interactive PTY terminals over SSH channels.

use std::sync::Arc;

use russh::client::Msg;
use russh::{Channel, ChannelMsg, ChannelWriteHalf};
use tokio::sync::{mpsc, Notify};

use crate::Result;

#[derive(Debug, Clone)]
pub enum TermEvent {
    Data(Vec<u8>),
    Exit,
}

pub struct Terminal {
    pub id: String,
    write: ChannelWriteHalf<Msg>,
    shutdown: Arc<Notify>,
}

impl Terminal {
    /// Requests a PTY + shell on `channel` and streams output into `events`.
    pub async fn open(
        channel: Channel<Msg>,
        cols: u32,
        rows: u32,
        events: mpsc::UnboundedSender<TermEvent>,
    ) -> Result<Self> {
        channel
            .request_pty(true, "xterm-256color", cols, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;

        let id = uuid::Uuid::new_v4().to_string();
        let (mut read, write) = channel.split();
        let shutdown = Arc::new(Notify::new());
        let shutdown_rx = shutdown.clone();

        tokio::spawn(async move {
            loop {
                let msg = tokio::select! {
                    msg = read.wait() => msg,
                    // A locally-initiated close is not echoed back as a
                    // channel message, so close() nudges us explicitly.
                    _ = shutdown_rx.notified() => None,
                };
                let Some(msg) = msg else {
                    let _ = events.send(TermEvent::Exit);
                    break;
                };
                match msg {
                    ChannelMsg::Data { ref data } => {
                        let _ = events.send(TermEvent::Data(data.to_vec()));
                    }
                    ChannelMsg::ExtendedData { ref data, .. } => {
                        let _ = events.send(TermEvent::Data(data.to_vec()));
                    }
                    ChannelMsg::Close | ChannelMsg::Eof => {
                        let _ = events.send(TermEvent::Exit);
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            id,
            write,
            shutdown,
        })
    }

    pub async fn write(&self, data: &[u8]) -> Result<()> {
        self.write.data(data).await?;
        Ok(())
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        self.write.window_change(cols, rows, 0, 0).await?;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        let _ = self.write.eof().await;
        let result = self.write.close().await;
        self.shutdown.notify_waiters();
        result?;
        Ok(())
    }
}
