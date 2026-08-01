use crate::device::manager::{DeviceConnectionState, DeviceManager, DeviceStatus};
use crate::device::serial::SerialTransport;
use crate::device::simulator::SimulatedTransport;
use crate::device::transport::{DeviceTransport, TransportKind};
use crate::domain::engine::HaloEngine;
use crate::domain::model::{HaloSnapshot, PresentationIntent, TaskSignal};
use crate::probe_adapter::{AdapterState, AdapterStatus, ProbeAdapter};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};

pub const ROUND_COMPLETE_HOLD_MS: u64 = 300_000;
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEVICE_POLL_INTERVAL: Duration = Duration::from_millis(50);

struct DeviceWorkerTimer {
    origin: Instant,
}

impl DeviceWorkerTimer {
    fn start() -> Self {
        Self::starting_at(Instant::now())
    }

    fn starting_at(origin: Instant) -> Self {
        Self { origin }
    }

    fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms_at(Instant::now())
    }

    fn elapsed_ms_at(&self, now: Instant) -> u64 {
        duration_millis_u64(now.saturating_duration_since(self.origin))
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceTransportMode {
    #[default]
    Simulator,
    Serial,
}

impl DeviceTransportMode {
    pub fn from_environment() -> Self {
        Self::from_override(std::env::var_os("CODEX_HALO_DEVICE_TRANSPORT").as_deref())
    }

    fn from_override(value: Option<&OsStr>) -> Self {
        match value {
            Some(value) if value == OsStr::new("serial") => Self::Serial,
            _ => Self::Simulator,
        }
    }

    fn transport_kind(self) -> TransportKind {
        match self {
            Self::Simulator => TransportKind::Simulator,
            Self::Serial => TransportKind::Serial,
        }
    }
}

pub struct AppState {
    pub(crate) engine: Arc<Mutex<HaloEngine>>,
    pub(crate) adapter_status: Arc<Mutex<AdapterStatus>>,
    pub(crate) device_status: Arc<Mutex<DeviceStatus>>,
    worker_stop: Arc<AtomicBool>,
    worker_handle: Mutex<Option<JoinHandle<()>>>,
    device_worker_stop: Arc<AtomicBool>,
    device_worker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            engine: Arc::new(Mutex::new(HaloEngine::new(ROUND_COMPLETE_HOLD_MS))),
            adapter_status: Arc::new(Mutex::new(AdapterStatus::offline())),
            device_status: Arc::new(Mutex::new(safe_device_status())),
            worker_stop: Arc::new(AtomicBool::new(false)),
            worker_handle: Mutex::new(None),
            device_worker_stop: Arc::new(AtomicBool::new(false)),
            device_worker_handle: Mutex::new(None),
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

    pub fn start_device_worker<R: Runtime>(
        &self,
        app_handle: AppHandle<R>,
        mode: DeviceTransportMode,
    ) {
        let Ok(mut worker_handle) = self.device_worker_handle.lock() else {
            return;
        };
        if worker_handle.is_some() {
            return;
        }

        self.device_worker_stop.store(false, Ordering::Release);
        let engine = Arc::clone(&self.engine);
        let device_status = Arc::clone(&self.device_status);
        let stop = Arc::clone(&self.device_worker_stop);
        let worker_app_handle = app_handle.clone();
        match thread::Builder::new()
            .name("codex-halo-device".to_owned())
            .spawn(move || {
                run_device_worker(worker_app_handle, engine, device_status, stop, mode);
            }) {
            Ok(handle) => *worker_handle = Some(handle),
            Err(_) => {
                let status = device_worker_error_status(mode, "Device worker could not start");
                publish_device_status(&self.device_status, status, |status| {
                    let _ = app_handle.emit("halo://device-status", status);
                });
            }
        }
    }

    pub fn stop_device_worker(&self) {
        self.device_worker_stop.store(true, Ordering::Release);
        let handle = self
            .device_worker_handle
            .lock()
            .ok()
            .and_then(|mut handle| handle.take());
        if let Some(handle) = handle {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }

    pub fn stop_workers(&self) {
        self.stop_probe_worker();
        self.stop_device_worker();
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.stop_workers();
    }
}

fn safe_device_status() -> DeviceStatus {
    DeviceStatus {
        revision: 0,
        state: DeviceConnectionState::Virtual,
        transport: TransportKind::Simulator,
        message: None,
        firmware_version: None,
        retry_count: 0,
    }
}

fn device_worker_error_status(mode: DeviceTransportMode, message: &str) -> DeviceStatus {
    DeviceStatus {
        revision: 0,
        state: DeviceConnectionState::Error,
        transport: mode.transport_kind(),
        message: Some(message.to_owned()),
        firmware_version: None,
        retry_count: 0,
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

fn run_device_worker<R: Runtime>(
    app_handle: AppHandle<R>,
    engine: Arc<Mutex<HaloEngine>>,
    device_status: Arc<Mutex<DeviceStatus>>,
    stop: Arc<AtomicBool>,
    mode: DeviceTransportMode,
) {
    match mode {
        DeviceTransportMode::Simulator => run_device_manager(
            app_handle,
            engine,
            device_status,
            stop,
            DeviceManager::new(SimulatedTransport::default()),
        ),
        DeviceTransportMode::Serial => run_device_manager(
            app_handle,
            engine,
            device_status,
            stop,
            DeviceManager::new(SerialTransport::default()),
        ),
    }
}

fn run_device_manager<R: Runtime, T: DeviceTransport>(
    app_handle: AppHandle<R>,
    engine: Arc<Mutex<HaloEngine>>,
    device_status: Arc<Mutex<DeviceStatus>>,
    stop: Arc<AtomicBool>,
    mut manager: DeviceManager<T>,
) {
    let timer = DeviceWorkerTimer::start();
    while !stop.load(Ordering::Acquire) {
        run_device_manager_iteration(
            &engine,
            &device_status,
            &mut manager,
            timer.elapsed_ms(),
            |status| {
                let _ = app_handle.emit("halo://device-status", status);
            },
            |snapshot| {
                let _ = app_handle.emit("halo://snapshot", snapshot);
            },
        );

        park_device_worker(&stop);
    }
}

fn run_device_manager_iteration<T: DeviceTransport>(
    engine: &Mutex<HaloEngine>,
    device_status: &Mutex<DeviceStatus>,
    manager: &mut DeviceManager<T>,
    now_ms: u64,
    emit_device_status: impl FnOnce(DeviceStatus),
    emit_snapshot: impl FnOnce(HaloSnapshot),
) {
    let snapshot = match engine.lock() {
        Ok(engine) => engine.snapshot(),
        Err(_) => {
            let status = DeviceStatus {
                revision: 0,
                state: DeviceConnectionState::Error,
                transport: manager.status().transport,
                message: Some("Virtual device state is unavailable".to_owned()),
                firmware_version: None,
                retry_count: manager.status().retry_count,
            };
            publish_device_status(device_status, status, emit_device_status);
            return;
        }
    };

    let result = manager.step(now_ms, &snapshot);
    publish_device_status(device_status, manager.status().clone(), emit_device_status);

    if let Some(snapshot) = apply_device_intents(engine, result.intents) {
        emit_snapshot(snapshot);
    }
}

fn park_device_worker(stop: &AtomicBool) {
    if !stop.load(Ordering::Acquire) {
        thread::park_timeout(DEVICE_POLL_INTERVAL);
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

fn publish_device_status(
    device_status: &Mutex<DeviceStatus>,
    mut observed: DeviceStatus,
    emit: impl FnOnce(DeviceStatus),
) {
    let changed = {
        let Ok(mut current) = device_status.lock() else {
            return;
        };
        if same_device_status_payload(&current, &observed) {
            return;
        }
        observed.revision = current.revision.saturating_add(1);
        *current = observed.clone();
        observed
    };
    emit(changed);
}

fn same_device_status_payload(left: &DeviceStatus, right: &DeviceStatus) -> bool {
    left.state == right.state
        && left.transport == right.transport
        && left.message == right.message
        && left.firmware_version == right.firmware_version
        && left.retry_count == right.retry_count
}

fn apply_device_intents(
    engine: &Mutex<HaloEngine>,
    intents: Vec<PresentationIntent>,
) -> Option<HaloSnapshot> {
    if intents.is_empty() {
        return None;
    }
    let Ok(mut engine) = engine.lock() else {
        return None;
    };
    let before_revision = engine.snapshot().revision;
    for intent in intents {
        engine.apply_presentation_intent(intent);
    }
    let snapshot = engine.snapshot();
    (snapshot.revision != before_revision).then_some(snapshot)
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
    use crate::device::manager::{DeviceConnectionState, DeviceStatus};
    use crate::device::simulator::Fault;
    use crate::device::transport::TransportKind;
    use crate::domain::model::{
        Confidence, DisplayMode, NormalizedState, PresentationIntent, SignalSource, TaskKey,
        TaskStatus,
    };
    use std::ffi::OsStr;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

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

    #[test]
    fn device_transport_mode_defaults_to_simulator_and_requires_an_exact_serial_override() {
        assert_eq!(
            DeviceTransportMode::default(),
            DeviceTransportMode::Simulator
        );
        assert_eq!(
            DeviceTransportMode::from_override(Some(OsStr::new("serial"))),
            DeviceTransportMode::Serial
        );
        for value in [None, Some(OsStr::new("Serial")), Some(OsStr::new("usb"))] {
            assert_eq!(
                DeviceTransportMode::from_override(value),
                DeviceTransportMode::Simulator
            );
        }
    }

    #[test]
    fn device_timer_progresses_when_the_wall_clock_moves_backward() {
        let origin = Instant::now();
        let timer = DeviceWorkerTimer::starting_at(origin);
        let wall_clock_before_ms = 10_000_u64;
        let wall_clock_after_ms = 1_000_u64;

        let first_step_ms = timer.elapsed_ms_at(origin + Duration::from_millis(10));
        let retry_step_ms = timer.elapsed_ms_at(origin + Duration::from_millis(260));

        assert!(wall_clock_after_ms < wall_clock_before_ms);
        assert_eq!(first_step_ms, 10);
        assert_eq!(retry_step_ms, 260);
        assert_eq!(retry_step_ms - first_step_ms, 250);
        assert_eq!(duration_millis_u64(Duration::MAX), u64::MAX);
    }

    #[test]
    fn knob_intents_mutate_the_engine_and_return_the_changed_snapshot() {
        let engine = Mutex::new(HaloEngine::new(ROUND_COMPLETE_HOLD_MS));

        let snapshot = apply_device_intents(&engine, vec![PresentationIntent::ShortPress])
            .expect("a presentation change must produce a snapshot");

        assert_eq!(snapshot.display_mode, DisplayMode::Overview);
        assert_eq!(engine.lock().unwrap().snapshot(), snapshot);
    }

    #[test]
    fn device_status_is_emitted_only_for_semantic_changes_and_outside_the_lock() {
        let shared = Mutex::new(DeviceStatus {
            revision: 0,
            state: DeviceConnectionState::Virtual,
            transport: TransportKind::Simulator,
            message: None,
            firmware_version: None,
            retry_count: 0,
        });
        let same = shared.lock().unwrap().clone();
        let mut emitted = Vec::new();

        publish_device_status(&shared, same, |status| emitted.push(status));
        publish_device_status(
            &shared,
            DeviceStatus {
                revision: 99,
                state: DeviceConnectionState::Connecting,
                transport: TransportKind::Serial,
                message: Some("Connecting".to_owned()),
                firmware_version: None,
                retry_count: 0,
            },
            |status| {
                assert!(
                    shared.try_lock().is_ok(),
                    "device status must be emitted after releasing its mutex"
                );
                emitted.push(status);
            },
        );

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].revision, 1);
        assert_eq!(shared.lock().unwrap().revision, 1);
    }

    #[test]
    fn device_worker_iteration_survives_crc_error_and_continues_syncing() {
        let engine = Mutex::new(HaloEngine::new(ROUND_COMPLETE_HOLD_MS));
        let device_status = Mutex::new(safe_device_status());
        let mut manager = DeviceManager::new(SimulatedTransport::default());
        let mut emitted_statuses = Vec::new();
        let mut emitted_snapshots = Vec::new();

        run_device_manager_iteration(
            &engine,
            &device_status,
            &mut manager,
            0,
            |status| {
                assert!(device_status.try_lock().is_ok());
                emitted_statuses.push(status);
            },
            |snapshot| {
                assert!(engine.try_lock().is_ok());
                emitted_snapshots.push(snapshot);
            },
        );
        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);

        manager.transport_mut().script(Fault::CorruptCrcOnce);
        engine.lock().unwrap().set_global_brightness(50).unwrap();

        let mut completed_iterations = 0;
        for now_ms in [10, 259, 260, 261] {
            run_device_manager_iteration(
                &engine,
                &device_status,
                &mut manager,
                now_ms,
                |status| {
                    assert!(device_status.try_lock().is_ok());
                    emitted_statuses.push(status);
                },
                |snapshot| {
                    assert!(engine.try_lock().is_ok());
                    emitted_snapshots.push(snapshot);
                },
            );
            completed_iterations += 1;
        }

        assert_eq!(completed_iterations, 4);
        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
        assert_eq!(
            device_status.lock().unwrap().state,
            DeviceConnectionState::Virtual
        );
        assert_eq!(
            manager
                .transport()
                .applied_snapshot()
                .expect("the recovered worker must apply the latest snapshot")
                .global_brightness,
            50
        );
        assert!(emitted_statuses
            .iter()
            .any(|status| status.retry_count == 1));
        assert_eq!(emitted_statuses.last().unwrap().message, None);
        assert!(emitted_snapshots.is_empty());
    }

    #[test]
    fn stopping_all_workers_is_repeatable_and_never_double_joins() {
        fn counting_worker(stop: Arc<AtomicBool>, exits: Arc<AtomicUsize>) -> JoinHandle<()> {
            thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    thread::park_timeout(Duration::from_millis(5));
                }
                exits.fetch_add(1, Ordering::AcqRel);
            })
        }

        let state = AppState::default();
        let probe_exits = Arc::new(AtomicUsize::new(0));
        let device_exits = Arc::new(AtomicUsize::new(0));
        *state.worker_handle.lock().unwrap() = Some(counting_worker(
            Arc::clone(&state.worker_stop),
            Arc::clone(&probe_exits),
        ));
        *state.device_worker_handle.lock().unwrap() = Some(counting_worker(
            Arc::clone(&state.device_worker_stop),
            Arc::clone(&device_exits),
        ));

        state.stop_workers();
        state.stop_workers();

        assert_eq!(probe_exits.load(Ordering::Acquire), 1);
        assert_eq!(device_exits.load(Ordering::Acquire), 1);
        assert!(state.worker_handle.lock().unwrap().is_none());
        assert!(state.device_worker_handle.lock().unwrap().is_none());
    }
}
