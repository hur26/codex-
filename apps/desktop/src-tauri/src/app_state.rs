use crate::domain::engine::HaloEngine;
use crate::domain::model::{HaloSnapshot, TaskSignal};
use crate::probe_adapter::{AdapterState, AdapterStatus, ProbeAdapter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};

pub const ROUND_COMPLETE_HOLD_MS: u64 = 300_000;
const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct AppState {
    pub(crate) engine: Arc<Mutex<HaloEngine>>,
    pub(crate) adapter_status: Arc<Mutex<AdapterStatus>>,
    worker_stop: Arc<AtomicBool>,
    worker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            engine: Arc::new(Mutex::new(HaloEngine::new(ROUND_COMPLETE_HOLD_MS))),
            adapter_status: Arc::new(Mutex::new(AdapterStatus::offline())),
            worker_stop: Arc::new(AtomicBool::new(false)),
            worker_handle: Mutex::new(None),
        }
    }
}

impl AppState {
    pub fn start_probe_worker<R: Runtime>(
        &self,
        app_handle: AppHandle<R>,
        probe_dir: Option<PathBuf>,
    ) {
        let Ok(mut worker_handle) = self.worker_handle.lock() else {
            return;
        };
        if worker_handle.is_some() {
            return;
        }

        self.worker_stop.store(false, Ordering::Release);
        let engine = Arc::clone(&self.engine);
        let adapter_status = Arc::clone(&self.adapter_status);
        let stop = Arc::clone(&self.worker_stop);
        match thread::Builder::new()
            .name("codex-halo-probe".to_owned())
            .spawn(move || {
                run_probe_worker(app_handle, engine, adapter_status, stop, probe_dir);
            }) {
            Ok(handle) => *worker_handle = Some(handle),
            Err(_) => {
                if let Ok(mut status) = self.adapter_status.lock() {
                    status.revision = status.revision.saturating_add(1);
                    status.state = AdapterState::Degraded;
                    status.message = Some("Hook 事件监听无法启动".to_owned());
                    status.rejected_events = status.rejected_events.saturating_add(1);
                }
            }
        }
    }

    pub fn stop_probe_worker(&self) {
        self.worker_stop.store(true, Ordering::Release);
        let handle = self
            .worker_handle
            .lock()
            .ok()
            .and_then(|mut handle| handle.take());
        if let Some(handle) = handle {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.stop_probe_worker();
    }
}

fn run_probe_worker<R: Runtime>(
    app_handle: AppHandle<R>,
    engine: Arc<Mutex<HaloEngine>>,
    adapter_status: Arc<Mutex<AdapterStatus>>,
    stop: Arc<AtomicBool>,
    probe_dir: Option<PathBuf>,
) {
    let mut adapter = ProbeAdapter::new(probe_dir);
    while !stop.load(Ordering::Acquire) {
        let batch = adapter.poll();
        publish_adapter_status(&adapter_status, batch.status, |status| {
            let _ = app_handle.emit("halo://adapter-status", status);
        });

        let now_ms = unix_time_ms();
        let changed_snapshot = apply_probe_batch(&engine, batch.signals, now_ms);
        if let Some(snapshot) = changed_snapshot {
            let _ = app_handle.emit("halo://snapshot", snapshot);
        }

        if stop.load(Ordering::Acquire) {
            break;
        }
        thread::park_timeout(POLL_INTERVAL);
    }
}

fn publish_adapter_status(
    adapter_status: &Mutex<AdapterStatus>,
    mut observed: AdapterStatus,
    emit: impl FnOnce(AdapterStatus),
) {
    let changed = {
        let Ok(mut current) = adapter_status.lock() else {
            return;
        };
        if current.same_payload(&observed) {
            return;
        }
        observed.revision = current.revision.saturating_add(1);
        *current = observed.clone();
        observed
    };
    emit(changed);
}

fn apply_probe_batch(
    engine: &Mutex<HaloEngine>,
    signals: Vec<TaskSignal>,
    now_ms: u64,
) -> Option<HaloSnapshot> {
    let Ok(mut engine) = engine.lock() else {
        return None;
    };
    let before_revision = engine.snapshot().revision;
    for signal in signals {
        engine.apply_signal(signal);
    }
    engine.tick(now_ms);
    let snapshot = engine.snapshot();
    (snapshot.revision != before_revision).then_some(snapshot)
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{Confidence, NormalizedState, SignalSource, TaskKey, TaskStatus};

    fn signal(received_at_ms: u64) -> TaskSignal {
        TaskSignal {
            task_key: TaskKey::parse("0123456789abcdef").unwrap(),
            state: NormalizedState {
                status: TaskStatus::Running,
                source: SignalSource::Hook,
                confidence: Confidence::Observed,
            },
            received_at_ms,
        }
    }

    #[test]
    fn a_probe_batch_returns_a_snapshot_only_when_revision_changes() {
        let engine = Mutex::new(HaloEngine::new(ROUND_COMPLETE_HOLD_MS));
        let first = apply_probe_batch(&engine, vec![signal(100)], 100)
            .expect("new signal must advance the revision");
        assert_eq!(first.revision, 1);

        assert!(apply_probe_batch(&engine, vec![signal(100)], 100).is_none());
        assert!(apply_probe_batch(&engine, Vec::new(), 100).is_none());
    }

    #[test]
    fn default_adapter_status_is_safe_and_offline() {
        let state = AppState::default();
        let status = state.adapter_status.lock().unwrap().clone();
        assert_eq!(status, AdapterStatus::offline());
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "revision": 0,
                "state": "offline",
                "mode": "hook",
                "message": "Hook 事件目录不可用",
                "acceptedEvents": 0,
                "ignoredEvents": 0,
                "rejectedEvents": 0
            })
        );
    }

    #[test]
    fn adapter_status_is_emitted_only_for_semantic_changes_and_outside_the_lock() {
        let shared = Mutex::new(AdapterStatus::offline());
        let observed = AdapterStatus {
            revision: 0,
            state: AdapterState::Online,
            mode: crate::probe_adapter::AdapterMode::Hook,
            message: None,
            accepted_events: 1,
            ignored_events: 0,
            rejected_events: 0,
        };
        let mut emitted = Vec::new();

        publish_adapter_status(&shared, observed.clone(), |status| {
            assert!(
                shared.try_lock().is_ok(),
                "adapter status must be emitted after releasing its mutex"
            );
            emitted.push(status);
        });
        publish_adapter_status(&shared, observed.clone(), |status| {
            emitted.push(status);
        });
        publish_adapter_status(
            &shared,
            AdapterStatus {
                accepted_events: 2,
                ..observed
            },
            |status| emitted.push(status),
        );

        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].revision, 1);
        assert_eq!(emitted[1].revision, 2);
        assert_eq!(shared.lock().unwrap().revision, 2);
    }
}
