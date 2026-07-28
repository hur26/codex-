use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Running,
    Waiting,
    RoundCompleted,
    Failed,
    Queued,
    Idle,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalSource {
    Hook,
    Simulator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Observed,
    Provisional,
    Simulated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalKind {
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    PermissionRequest,
    Stop,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BindingMode {
    Auto,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TaskKey(String);

impl TaskKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, TaskKeyError> {
        let value = value.into();
        let is_valid = value.len() == 16
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));

        if is_valid {
            Ok(Self(value))
        } else {
            Err(TaskKeyError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskKeyError;

impl fmt::Display for TaskKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid task key")
    }
}

impl std::error::Error for TaskKeyError {}

impl Serialize for TaskKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedState {
    pub status: TaskStatus,
    pub source: SignalSource,
    pub confidence: Confidence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSignal {
    pub task_key: TaskKey,
    pub state: NormalizedState,
    pub received_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub task_key: TaskKey,
    pub status: TaskStatus,
    pub source: SignalSource,
    pub confidence: Confidence,
    pub last_active_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RingSlot {
    pub index: usize,
    pub task_key: Option<TaskKey>,
    pub status: TaskStatus,
    pub source: Option<SignalSource>,
    pub confidence: Option<Confidence>,
    pub binding_mode: BindingMode,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloSnapshot {
    pub slots: Vec<RingSlot>,
    pub tasks: Vec<TaskRecord>,
    pub queue: Vec<TaskRecord>,
}
