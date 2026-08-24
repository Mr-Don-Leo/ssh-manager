//! Async job manager: spawns cancellable tokio tasks and broadcasts progress.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tokio_util_lite::CancellationToken;

use crate::model::{now_secs, JobInfo, JobState};
use crate::{CoreError, Result};

/// Minimal cancellation token (avoids a tokio-util dependency).
pub mod tokio_util_lite {
    use std::sync::{Arc, Mutex};
    use tokio::sync::Notify;

    #[derive(Clone, Default)]
    pub struct CancellationToken {
        inner: Arc<Inner>,
    }

    #[derive(Default)]
    struct Inner {
        cancelled: Mutex<bool>,
        notify: Notify,
    }

    impl CancellationToken {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn cancel(&self) {
            *self.inner.cancelled.lock().unwrap() = true;
            self.inner.notify.notify_waiters();
        }

        pub fn is_cancelled(&self) -> bool {
            *self.inner.cancelled.lock().unwrap()
        }

        pub async fn cancelled(&self) {
            loop {
                if self.is_cancelled() {
                    return;
                }
                let notified = self.inner.notify.notified();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        }
    }
}

struct JobRecord {
    info: JobInfo,
    cancel: CancellationToken,
}

#[derive(Clone)]
pub struct JobManager {
    jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
    events: broadcast::Sender<JobInfo>,
}

/// Handle passed into job bodies for progress reporting and cancellation.
#[derive(Clone)]
pub struct JobCtx {
    id: String,
    jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
    events: broadcast::Sender<JobInfo>,
    pub cancel: CancellationToken,
}

impl JobCtx {
    pub fn progress(&self, fraction: f64, detail: Option<String>) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(rec) = jobs.get_mut(&self.id) {
            rec.info.progress = Some(fraction.clamp(0.0, 1.0));
            if detail.is_some() {
                rec.info.detail = detail;
            }
            let _ = self.events.send(rec.info.clone());
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl JobManager {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobInfo> {
        self.events.subscribe()
    }

    pub fn list(&self) -> Vec<JobInfo> {
        let mut jobs: Vec<JobInfo> = self
            .jobs
            .lock()
            .unwrap()
            .values()
            .map(|r| r.info.clone())
            .collect();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
        jobs
    }

    pub fn get(&self, id: &str) -> Result<JobInfo> {
        self.jobs
            .lock()
            .unwrap()
            .get(id)
            .map(|r| r.info.clone())
            .ok_or_else(|| CoreError::JobNotFound(id.to_string()))
    }

    pub fn cancel(&self, id: &str) -> Result<()> {
        let jobs = self.jobs.lock().unwrap();
        let rec = jobs
            .get(id)
            .ok_or_else(|| CoreError::JobNotFound(id.to_string()))?;
        rec.cancel.cancel();
        Ok(())
    }

    /// Spawns a job. `body` receives a `JobCtx` for progress/cancellation and
    /// resolves to Ok(detail) or Err. State transitions are broadcast.
    pub fn spawn<F, Fut>(&self, kind: &str, label: &str, body: F) -> JobInfo
    where
        F: FnOnce(JobCtx) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Option<String>>> + Send + 'static,
    {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        let info = JobInfo {
            id: id.clone(),
            kind: kind.to_string(),
            label: label.to_string(),
            state: JobState::Queued,
            progress: None,
            detail: None,
            error: None,
            created_at: now_secs(),
            finished_at: None,
        };
        self.jobs.lock().unwrap().insert(
            id.clone(),
            JobRecord {
                info: info.clone(),
                cancel: cancel.clone(),
            },
        );
        let _ = self.events.send(info.clone());

        let ctx = JobCtx {
            id: id.clone(),
            jobs: self.jobs.clone(),
            events: self.events.clone(),
            cancel: cancel.clone(),
        };
        let jobs = self.jobs.clone();
        let events = self.events.clone();

        tokio::spawn(async move {
            set_state(&jobs, &events, &id, JobState::Running, None);
            let result = tokio::select! {
                r = body(ctx) => r,
                _ = cancel.cancelled() => Err(CoreError::other("cancelled")),
            };
            match result {
                Ok(detail) => set_state(&jobs, &events, &id, JobState::Done, detail),
                Err(_) if cancel.is_cancelled() => {
                    set_state(&jobs, &events, &id, JobState::Cancelled, None)
                }
                Err(e) => {
                    let mut js = jobs.lock().unwrap();
                    if let Some(rec) = js.get_mut(&id) {
                        rec.info.state = JobState::Failed;
                        rec.info.error = Some(e.to_string());
                        rec.info.finished_at = Some(now_secs());
                        let _ = events.send(rec.info.clone());
                    }
                }
            }
        });
        info
    }
}

fn set_state(
    jobs: &Arc<Mutex<HashMap<String, JobRecord>>>,
    events: &broadcast::Sender<JobInfo>,
    id: &str,
    state: JobState,
    detail: Option<String>,
) {
    let mut js = jobs.lock().unwrap();
    if let Some(rec) = js.get_mut(id) {
        rec.info.state = state;
        if detail.is_some() {
            rec.info.detail = detail;
        }
        if matches!(
            state,
            JobState::Done | JobState::Failed | JobState::Cancelled
        ) {
            rec.info.finished_at = Some(now_secs());
            rec.info.progress = None;
        }
        let _ = events.send(rec.info.clone());
    }
}
