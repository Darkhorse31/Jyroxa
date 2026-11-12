//! Download queue and scheduler.
//!
//! Runs multiple downloads with a cap on concurrency, honouring per-task
//! priority and an optional "start at" wall-clock time. Every task gets a
//! [`Control`] handle (pause/resume/cancel) and progress events are forwarded
//! tagged with the originating [`TaskId`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tokio::sync::Semaphore;

use crate::download::{Control, DownloadOutcome, DownloadRequest, Downloader, RefreshFn};
use crate::error::Result;
use crate::events::ProgressEvent;

/// Identifier assigned to each queued task.
pub type TaskId = u64;

/// Task priority. Higher priority tasks start first when slots free up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

/// A single unit of scheduled work.
pub struct TaskSpec {
    pub request: DownloadRequest,
    pub priority: TaskPriority,
    /// Do not start before this wall-clock time.
    pub start_at: Option<SystemTime>,
    /// Optional per-task link refresher.
    pub refresher: Option<RefreshFn>,
}

impl TaskSpec {
    pub fn new(request: DownloadRequest) -> Self {
        TaskSpec {
            request,
            priority: TaskPriority::Normal,
            start_at: None,
            refresher: None,
        }
    }
    pub fn priority(mut self, p: TaskPriority) -> Self {
        self.priority = p;
        self
    }
    pub fn start_at(mut self, t: SystemTime) -> Self {
        self.start_at = Some(t);
        self
    }
}

/// Scheduler configuration.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_concurrent: usize,
}

impl Default for QueueConfig {
    fn default() -> Self {
        QueueConfig { max_concurrent: 3 }
    }
}

/// Outcome of a single scheduled task.
pub struct TaskOutcome {
    pub id: TaskId,
    pub result: Result<DownloadOutcome>,
}

/// The scheduler. Shares one [`Downloader`] (and thus its client, global limiter
/// and host-tuning memory) across every task.
pub struct Scheduler {
    downloader: Arc<Downloader>,
    config: QueueConfig,
    next_id: AtomicU64,
    controls: Mutex<HashMap<TaskId, Control>>,
}

/// Decide execution order: highest priority first, stable within a priority.
pub fn order_indices(specs: &[TaskSpec]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..specs.len()).collect();
    idx.sort_by(|&a, &b| specs[b].priority.cmp(&specs[a].priority).then(a.cmp(&b)));
    idx
}

impl Scheduler {
    pub fn new(downloader: Arc<Downloader>, config: QueueConfig) -> Self {
        Scheduler {
            downloader,
            config,
            next_id: AtomicU64::new(1),
            controls: Mutex::new(HashMap::new()),
        }
    }

    /// Fetch the control handle for a task (for pause/resume/cancel from a UI).
    pub fn control(&self, id: TaskId) -> Option<Control> {
        self.controls.lock().unwrap().get(&id).cloned()
    }

    /// Run all tasks to completion, returning per-task outcomes ordered by id.
    ///
    /// `events`, if provided, receives every [`ProgressEvent`] tagged with the
    /// task id that produced it.
    pub async fn run(
        &self,
        specs: Vec<TaskSpec>,
        events: Option<tokio::sync::mpsc::UnboundedSender<(TaskId, ProgressEvent)>>,
    ) -> Vec<TaskOutcome> {
        let order = order_indices(&specs);
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent.max(1)));

        // Assign ids in submission order for stable reporting.
        let ids: Vec<TaskId> = specs
            .iter()
            .map(|_| self.next_id.fetch_add(1, Ordering::Relaxed))
            .collect();

        // Register controls up front so a UI can act on a queued (not-yet-started) task.
        {
            let mut map = self.controls.lock().unwrap();
            for id in &ids {
                map.insert(*id, Control::new());
            }
        }

        let mut specs: Vec<Option<TaskSpec>> = specs.into_iter().map(Some).collect();
        let mut set = tokio::task::JoinSet::new();

        for pos in order {
            let id = ids[pos];
            let spec = specs[pos].take().unwrap();
            let control = self.control(id).unwrap();
            let downloader = self.downloader.clone();
            let sem = semaphore.clone();
            let events = events.clone();

            set.spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore not closed");

                // Honour start-at time.
                if let Some(when) = spec.start_at {
                    if let Ok(delay) = when.duration_since(SystemTime::now()) {
                        tokio::time::sleep(delay).await;
                    }
                }

                // Forward per-task events tagged with the id.
                let per_task_tx = events.as_ref().map(|agg| {
                    let (tx, mut rx) = crate::events::channel();
                    let agg = agg.clone();
                    tokio::spawn(async move {
                        while let Some(ev) = rx.recv().await {
                            let _ = agg.send((id, ev));
                        }
                    });
                    tx
                });

                let result = downloader
                    .download(spec.request, per_task_tx, control, spec.refresher)
                    .await;
                TaskOutcome { id, result }
            });
        }

        let mut outcomes = Vec::new();
        while let Some(joined) = set.join_next().await {
            if let Ok(outcome) = joined {
                outcomes.push(outcome);
            }
        }

        // Clean up controls.
        {
            let mut map = self.controls.lock().unwrap();
            for id in &ids {
                map.remove(id);
            }
        }

        outcomes.sort_by_key(|o| o.id);
        outcomes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(p: TaskPriority) -> TaskSpec {
        TaskSpec::new(DownloadRequest::new("https://x.test/f.bin", "f.bin")).priority(p)
    }

    #[test]
    fn priority_ordering_high_first_stable() {
        let specs = vec![
            spec(TaskPriority::Normal),
            spec(TaskPriority::High),
            spec(TaskPriority::Low),
            spec(TaskPriority::High),
        ];
        // High tasks (indices 1 and 3) first in submission order, then normal, then low.
        assert_eq!(order_indices(&specs), vec![1, 3, 0, 2]);
    }

    #[test]
    fn priority_enum_order() {
        assert!(TaskPriority::High > TaskPriority::Normal);
        assert!(TaskPriority::Normal > TaskPriority::Low);
    }
}
