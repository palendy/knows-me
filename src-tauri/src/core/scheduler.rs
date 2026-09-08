//! Periodic background scheduler (`Scheduler`).
//!
//! The platform primitive other units register batch jobs on — e.g. the
//! periodic session/source sync (US-1.1). Runs a single job on a fixed interval
//! without overlapping executions, and can be stopped.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

#[derive(Default)]
pub struct Scheduler {
    running: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start running `job` immediately and then every `interval`. Executions do
    /// not overlap: the next tick waits for `interval` *after* the previous job
    /// finishes. Calling `start` while already running is a no-op.
    pub fn start<F, Fut>(&self, interval: Duration, job: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.running.swap(true, Ordering::SeqCst) {
            return; // already running
        }
        let running = self.running.clone();
        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                job().await;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(interval).await;
            }
        });
        *self.handle.lock().expect("scheduler lock poisoned") = Some(handle);
    }

    /// Signal the loop to stop and abort the task.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.handle.lock().expect("scheduler lock poisoned").take() {
            h.abort();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[tokio::test]
    async fn runs_job_repeatedly_then_stops() {
        let sched = Scheduler::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        sched.start(Duration::from_millis(5), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
            }
        });
        assert!(sched.is_running());
        tokio::time::sleep(Duration::from_millis(60)).await;
        sched.stop();
        let after_stop = counter.load(Ordering::SeqCst);
        assert!(after_stop >= 2, "job should have run several times");
        assert!(!sched.is_running());

        // No further increments after stop.
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(counter.load(Ordering::SeqCst), after_stop);
    }

    #[tokio::test]
    async fn double_start_is_noop() {
        let sched = Scheduler::new();
        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            let c = counter.clone();
            sched.start(Duration::from_millis(5), move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        assert!(sched.is_running());
        sched.stop();
    }
}
