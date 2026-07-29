use crate::domain::model::{SignalKind, SignalSource, TaskKey, TaskSignal};
use crate::domain::normalize::normalize_signal;
use chrono::DateTime;
use serde::Deserialize;
use serde::Serialize;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Take};
use std::path::{Path, PathBuf};

pub const MAX_EVENTS_PER_POLL: usize = 128;
pub const MAX_EVENT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterState {
    Online,
    Degraded,
    Offline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterMode {
    Hook,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterStatus {
    pub state: AdapterState,
    pub mode: AdapterMode,
    pub message: Option<String>,
    pub accepted_events: u64,
    pub ignored_events: u64,
    pub rejected_events: u64,
}

impl AdapterStatus {
    pub fn offline() -> Self {
        Self {
            state: AdapterState::Offline,
            mode: AdapterMode::Hook,
            message: Some("Hook 事件目录不可用".to_owned()),
            accepted_events: 0,
            ignored_events: 0,
            rejected_events: 0,
        }
    }
}

#[derive(Debug)]
pub struct PollBatch {
    pub signals: Vec<TaskSignal>,
    pub status: AdapterStatus,
}

pub fn resolve_probe_dir(
    explicit_override: Option<&Path>,
    environment_override: Option<&OsStr>,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    explicit_override
        .map(Path::to_path_buf)
        .or_else(|| environment_override.map(PathBuf::from))
        .or_else(|| home_dir.map(|home| home.join(".codex-halo").join("probe")))
}

pub struct ProbeAdapter {
    probe_dir: Option<PathBuf>,
    cursor: Option<OsString>,
    accepted_events: u64,
    ignored_events: u64,
    rejected_events: u64,
    offline_episode: bool,
    has_safety_rejections: bool,
}

impl ProbeAdapter {
    pub fn new(probe_dir: Option<PathBuf>) -> Self {
        Self {
            probe_dir,
            cursor: None,
            accepted_events: 0,
            ignored_events: 0,
            rejected_events: 0,
            offline_episode: false,
            has_safety_rejections: false,
        }
    }

    pub fn poll(&mut self) -> PollBatch {
        let Some(probe_dir) = self.probe_dir.clone() else {
            return self.offline_batch();
        };
        if !probe_dir.is_dir() {
            return self.offline_batch();
        }

        let entries = match fs::read_dir(&probe_dir) {
            Ok(entries) => {
                self.offline_episode = false;
                entries
            }
            Err(_) => return self.offline_batch(),
        };
        let mut filenames = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else {
                self.reject();
                continue;
            };
            let filename = entry.file_name();
            if Path::new(&filename).extension() != Some(OsStr::new("json")) {
                continue;
            }
            let is_after_cursor = self.cursor.as_ref().is_none_or(|cursor| filename > *cursor);
            if is_after_cursor {
                filenames.push(filename);
            }
        }
        filenames.sort();
        filenames.truncate(MAX_EVENTS_PER_POLL);

        let mut signals = Vec::new();
        for filename in filenames {
            self.cursor = Some(filename.clone());
            match parse_event_file(&probe_dir.join(&filename)) {
                Ok(ParsedEvent::Signal(signal)) => {
                    self.accepted_events = self.accepted_events.saturating_add(1);
                    signals.push(signal);
                }
                Ok(ParsedEvent::Ignored) => {
                    self.accepted_events = self.accepted_events.saturating_add(1);
                    self.ignored_events = self.ignored_events.saturating_add(1);
                }
                Err(()) => self.reject(),
            }
        }

        let state = if self.has_safety_rejections {
            AdapterState::Degraded
        } else {
            AdapterState::Online
        };
        PollBatch {
            signals,
            status: self.status(state),
        }
    }

    fn reject(&mut self) {
        self.rejected_events = self.rejected_events.saturating_add(1);
        self.has_safety_rejections = true;
    }

    fn offline_batch(&mut self) -> PollBatch {
        if !self.offline_episode {
            self.rejected_events = self.rejected_events.saturating_add(1);
            self.offline_episode = true;
        }
        PollBatch {
            signals: Vec::new(),
            status: self.status(AdapterState::Offline),
        }
    }

    fn status(&self, state: AdapterState) -> AdapterStatus {
        let message = match state {
            AdapterState::Online => None,
            AdapterState::Degraded => Some("部分 Hook 事件未通过安全校验".to_owned()),
            AdapterState::Offline => Some("Hook 事件目录不可用".to_owned()),
        };
        AdapterStatus {
            state,
            mode: AdapterMode::Hook,
            message,
            accepted_events: self.accepted_events,
            ignored_events: self.ignored_events,
            rejected_events: self.rejected_events,
        }
    }
}

#[derive(Deserialize)]
struct RawEvent {
    schema_version: u64,
    received_at: String,
    hook_type: String,
    identity_candidates: Vec<IdentityCandidate>,
}

#[derive(Deserialize)]
struct IdentityCandidate {
    path: String,
    fingerprint: String,
}

enum ParsedEvent {
    Signal(TaskSignal),
    Ignored,
}

fn parse_event_file(path: &Path) -> Result<ParsedEvent, ()> {
    let path_metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if !path_metadata.file_type().is_file() || path_metadata.len() > MAX_EVENT_BYTES {
        return Err(());
    }
    let file = fs::File::open(path).map_err(|_| ())?;
    if file.metadata().map_err(|_| ())?.len() > MAX_EVENT_BYTES {
        return Err(());
    }
    let mut bytes = Vec::new();
    let mut limited: Take<fs::File> = file.take(MAX_EVENT_BYTES + 1);
    limited.read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() as u64 > MAX_EVENT_BYTES {
        return Err(());
    }

    let raw: RawEvent = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if raw.schema_version != 1 {
        return Err(());
    }
    let hook = KnownHook::parse(&raw.hook_type).ok_or(())?;
    let task_key = exact_fingerprint(&raw.identity_candidates, "$.session_id")?
        .ok_or(())
        .and_then(|fingerprint| TaskKey::parse(fingerprint).map_err(|_| ()))?;
    if let Some(turn) = exact_fingerprint(&raw.identity_candidates, "$.turn_id")? {
        TaskKey::parse(turn).map_err(|_| ())?;
    }
    let timestamp = DateTime::parse_from_rfc3339(&raw.received_at).map_err(|_| ())?;
    let received_at_ms = u64::try_from(timestamp.timestamp_millis()).map_err(|_| ())?;

    let Some(signal_kind) = hook.signal_kind() else {
        return Ok(ParsedEvent::Ignored);
    };
    let state = normalize_signal(signal_kind, SignalSource::Hook).map_err(|_| ())?;
    Ok(ParsedEvent::Signal(TaskSignal {
        task_key,
        state,
        received_at_ms,
    }))
}

fn exact_fingerprint(
    candidates: &[IdentityCandidate],
    expected_path: &str,
) -> Result<Option<String>, ()> {
    let mut matching = candidates
        .iter()
        .filter(|candidate| candidate.path == expected_path);
    let first = matching.next();
    if matching.next().is_some() {
        return Err(());
    }
    let Some(candidate) = first else {
        return Ok(None);
    };
    TaskKey::parse(&candidate.fingerprint).map_err(|_| ())?;
    Ok(Some(candidate.fingerprint.clone()))
}

#[derive(Clone, Copy)]
enum KnownHook {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreToolUse,
    PermissionRequest,
    PostToolUse,
    PreCompact,
    PostCompact,
    SubagentStart,
    SubagentStop,
    Stop,
}

impl KnownHook {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "SessionStart" => Some(Self::SessionStart),
            "SessionEnd" => Some(Self::SessionEnd),
            "UserPromptSubmit" => Some(Self::UserPromptSubmit),
            "PreToolUse" => Some(Self::PreToolUse),
            "PermissionRequest" => Some(Self::PermissionRequest),
            "PostToolUse" => Some(Self::PostToolUse),
            "PreCompact" => Some(Self::PreCompact),
            "PostCompact" => Some(Self::PostCompact),
            "SubagentStart" => Some(Self::SubagentStart),
            "SubagentStop" => Some(Self::SubagentStop),
            "Stop" => Some(Self::Stop),
            _ => None,
        }
    }

    const fn signal_kind(self) -> Option<SignalKind> {
        match self {
            Self::UserPromptSubmit => Some(SignalKind::UserPromptSubmit),
            Self::PreToolUse => Some(SignalKind::PreToolUse),
            Self::PostToolUse => Some(SignalKind::PostToolUse),
            Self::PermissionRequest => Some(SignalKind::PermissionRequest),
            Self::Stop => Some(SignalKind::Stop),
            Self::SessionStart
            | Self::SessionEnd
            | Self::PreCompact
            | Self::PostCompact
            | Self::SubagentStart
            | Self::SubagentStop => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::engine::HaloEngine;
    use crate::domain::model::{Confidence, SignalSource, TaskStatus};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let nonce = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock must be after unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "codex-halo-probe-adapter-{}-{timestamp}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test probe directory must be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.0.join(name), content).expect("test event must be written");
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn event(hook_type: &str, session: &str, received_at: &str) -> String {
        serde_json::json!({
            "schema_version": 1,
            "received_at": received_at,
            "hook_type": hook_type,
            "identity_candidates": [
                {
                    "path": "$.session_id",
                    "kind": "string",
                    "length": 36,
                    "fingerprint": session
                },
                {
                    "path": "$.turn_id",
                    "kind": "string",
                    "length": 36,
                    "fingerprint": "fedcba9876543210"
                }
            ],
            "prompt": "must never leave this parser",
            "tool_input": {"secret": "must never be retained"},
            "shape": [{"path": "$.prompt", "redacted": true}]
        })
        .to_string()
    }

    #[test]
    fn resolve_path_prefers_explicit_then_environment_then_cross_platform_home() {
        let explicit = Path::new("D:/explicit");
        let environment = OsStr::new("D:/environment");
        let home = Path::new("D:/home");

        assert_eq!(
            resolve_probe_dir(Some(explicit), Some(environment), Some(home)),
            Some(explicit.to_path_buf())
        );
        assert_eq!(
            resolve_probe_dir(None, Some(environment), Some(home)),
            Some(PathBuf::from(environment))
        );
        assert_eq!(
            resolve_probe_dir(None, None, Some(home)),
            Some(home.join(".codex-halo").join("probe"))
        );
        assert_eq!(resolve_probe_dir(None, None, None), None);
    }

    #[test]
    fn startup_processes_existing_json_oldest_first_once_and_caps_each_poll() {
        let directory = TestDir::new();
        for index in (0..130).rev() {
            directory.write(
                &format!("{index:03}.json"),
                &event(
                    "PreToolUse",
                    "0123456789abcdef",
                    &format!("2026-07-26T08:00:{:02}Z", index % 60),
                ),
            );
        }
        directory.write("ignored.tmp", "not an event");

        let mut adapter = ProbeAdapter::new(Some(directory.path().to_path_buf()));
        let first = adapter.poll();
        assert_eq!(first.signals.len(), MAX_EVENTS_PER_POLL);
        assert_eq!(first.signals[0].received_at_ms, 1_785_052_800_000);
        assert_eq!(first.signals[127].received_at_ms, 1_785_052_807_000);

        let second = adapter.poll();
        assert_eq!(second.signals.len(), 2);
        assert_eq!(second.signals[0].received_at_ms, 1_785_052_808_000);
        assert_eq!(second.signals[1].received_at_ms, 1_785_052_809_000);
        assert!(adapter.poll().signals.is_empty());
        assert_eq!(second.status.accepted_events, 130);
    }

    #[test]
    fn maps_only_supported_lifecycle_hooks_with_hook_confidence() {
        let directory = TestDir::new();
        for (index, hook) in [
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
        ]
        .into_iter()
        .enumerate()
        {
            directory.write(
                &format!("{index}.json"),
                &event(hook, "0123456789abcdef", "2026-07-26T08:00:00+00:00"),
            );
        }

        let batch = ProbeAdapter::new(Some(directory.path().to_path_buf())).poll();
        let states: Vec<_> = batch
            .signals
            .iter()
            .map(|signal| {
                (
                    signal.state.status,
                    signal.state.source,
                    signal.state.confidence,
                )
            })
            .collect();
        assert_eq!(
            states,
            vec![
                (
                    TaskStatus::Running,
                    SignalSource::Hook,
                    Confidence::Observed
                ),
                (
                    TaskStatus::Running,
                    SignalSource::Hook,
                    Confidence::Observed
                ),
                (
                    TaskStatus::Running,
                    SignalSource::Hook,
                    Confidence::Observed
                ),
                (
                    TaskStatus::Waiting,
                    SignalSource::Hook,
                    Confidence::Provisional
                ),
                (
                    TaskStatus::RoundCompleted,
                    SignalSource::Hook,
                    Confidence::Observed
                ),
            ]
        );
        assert_eq!(batch.status.state, AdapterState::Online);
        assert_eq!(batch.status.accepted_events, 5);
        assert_eq!(batch.status.rejected_events, 0);
    }

    #[test]
    fn known_non_state_hooks_are_accepted_and_ignored_without_failure_inference() {
        let directory = TestDir::new();
        for (index, hook) in [
            "SessionStart",
            "SessionEnd",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
        ]
        .into_iter()
        .enumerate()
        {
            directory.write(
                &format!("{index}.json"),
                &event(hook, "0123456789abcdef", "2026-07-26T08:00:00Z"),
            );
        }

        let batch = ProbeAdapter::new(Some(directory.path().to_path_buf())).poll();
        assert!(batch.signals.is_empty());
        assert_eq!(batch.status.state, AdapterState::Online);
        assert_eq!(batch.status.accepted_events, 6);
        assert_eq!(batch.status.ignored_events, 6);
        assert_eq!(batch.status.rejected_events, 0);
    }

    #[test]
    fn malformed_oversized_or_unsafe_events_only_degrade_safe_counters() {
        let directory = TestDir::new();
        directory.write("00-broken.json", "{");
        directory.write(
            "01-schema.json",
            &event("Stop", "0123456789abcdef", "2026-07-26T08:00:00Z")
                .replace("\"schema_version\":1", "\"schema_version\":2"),
        );
        directory.write(
            "02-unknown.json",
            &event("Failure", "0123456789abcdef", "2026-07-26T08:00:00Z"),
        );
        directory.write(
            "03-missing-session.json",
            r#"{"schema_version":1,"received_at":"2026-07-26T08:00:00Z","hook_type":"Stop","identity_candidates":[]}"#,
        );
        directory.write(
            "04-uppercase-session.json",
            &event("Stop", "0123456789abcdeF", "2026-07-26T08:00:00Z"),
        );
        directory.write(
            "05-invalid-turn.json",
            &event("Stop", "0123456789abcdef", "2026-07-26T08:00:00Z")
                .replace("fedcba9876543210", "INVALID"),
        );
        directory.write(
            "06-time.json",
            &event("Stop", "0123456789abcdef", "not-a-time"),
        );
        directory.write("07-large.json", &"x".repeat(MAX_EVENT_BYTES as usize + 1));
        fs::create_dir(directory.path().join("08-not-a-file.json"))
            .expect("unsafe json-shaped directory must be created");

        let mut adapter = ProbeAdapter::new(Some(directory.path().to_path_buf()));
        let batch = adapter.poll();
        assert!(batch.signals.is_empty());
        assert_eq!(batch.status.state, AdapterState::Degraded);
        assert_eq!(batch.status.accepted_events, 0);
        assert_eq!(batch.status.rejected_events, 9);
        assert!(batch
            .status
            .message
            .as_deref()
            .is_some_and(|message| !message.contains("00-broken")));
        assert!(adapter.poll().signals.is_empty());
        assert_eq!(adapter.poll().status.rejected_events, 9);
    }

    #[test]
    fn missing_directory_is_offline_without_panicking_or_disclosing_the_path() {
        let directory = TestDir::new();
        let missing = directory.path().join("private-user-name").join("missing");
        let mut adapter = ProbeAdapter::new(Some(missing.clone()));
        let first = adapter.poll();

        assert!(first.signals.is_empty());
        assert_eq!(first.status.state, AdapterState::Offline);
        assert_eq!(first.status.rejected_events, 1);
        assert!(!first
            .status
            .message
            .as_deref()
            .unwrap_or_default()
            .contains(missing.to_string_lossy().as_ref()));

        let repeated = adapter.poll();
        assert_eq!(repeated.status.state, AdapterState::Offline);
        assert_eq!(repeated.status.rejected_events, 1);

        fs::create_dir_all(&missing).expect("probe directory must recover");
        let recovered = adapter.poll();
        assert_eq!(recovered.status.state, AdapterState::Online);
        assert_eq!(recovered.status.rejected_events, 1);

        fs::remove_dir(&missing).expect("empty recovered directory must be removable");
        let missing_again = adapter.poll();
        assert_eq!(missing_again.status.state, AdapterState::Offline);
        assert_eq!(missing_again.status.rejected_events, 2);
        assert!(!missing_again
            .status
            .message
            .as_deref()
            .unwrap_or_default()
            .contains(missing.to_string_lossy().as_ref()));
    }

    #[test]
    fn two_session_fingerprints_create_two_engine_task_keys_without_crossing() {
        let directory = TestDir::new();
        directory.write(
            "0.json",
            &event("PreToolUse", "0123456789abcdef", "2026-07-26T08:00:00Z"),
        );
        directory.write(
            "1.json",
            &event("Stop", "1111111111111111", "2026-07-26T08:00:01Z"),
        );

        let batch = ProbeAdapter::new(Some(directory.path().to_path_buf())).poll();
        let mut engine = HaloEngine::new(300_000);
        for signal in batch.signals {
            engine.apply_signal(signal);
        }

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.tasks.len(), 2);
        assert_eq!(
            snapshot.slots[0].task_key.as_ref().unwrap().as_str(),
            "0123456789abcdef"
        );
        assert_eq!(snapshot.slots[0].status, TaskStatus::Running);
        assert_eq!(
            snapshot.slots[1].task_key.as_ref().unwrap().as_str(),
            "1111111111111111"
        );
        assert_eq!(snapshot.slots[1].status, TaskStatus::RoundCompleted);
    }
}
