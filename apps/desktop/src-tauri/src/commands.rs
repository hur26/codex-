use crate::app_state::{AppState, ROUND_COMPLETE_HOLD_MS};
use crate::domain::effects::{Direction, EffectParameter, EffectProfile, EffectValidationError};
use crate::domain::engine::EngineError;
use crate::domain::model::{HaloSnapshot, SignalKind, SignalSource, TaskKey, TaskSignal};
use crate::domain::normalize::normalize_signal;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulateSignalInput {
    pub task_key: String,
    pub signal_kind: String,
    pub received_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualBindInput {
    pub task_key: String,
    pub slot: usize,
    pub lock: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateEffectInput {
    pub slot: usize,
    pub brightness: u16,
    pub speed_percent: u16,
    pub direction: String,
    pub tail_percent: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CommandError {
    InvalidInput { argument: CommandArgument },
    InvalidTaskKey,
    UnknownSignalKind { signal_kind: String },
    UnknownDirection { direction: String },
    SlotOutOfBounds { slot: usize },
    TaskNotFound { task_key: String },
    EmptySlot { slot: usize },
    InvalidEffect { error: EffectValidationError },
    StateUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandArgument {
    Input,
    Slot,
    Left,
    Right,
    Value,
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { argument } => {
                write!(formatter, "invalid {argument:?} command argument")
            }
            Self::InvalidTaskKey => formatter.write_str("invalid task key"),
            Self::UnknownSignalKind { signal_kind } => {
                write!(formatter, "unknown signal kind: {signal_kind}")
            }
            Self::UnknownDirection { direction } => {
                write!(formatter, "unknown effect direction: {direction}")
            }
            Self::SlotOutOfBounds { slot } => write!(formatter, "slot {slot} is out of bounds"),
            Self::TaskNotFound { task_key } => write!(formatter, "task {task_key} does not exist"),
            Self::EmptySlot { slot } => write!(formatter, "slot {slot} is empty"),
            Self::InvalidEffect { error } => error.fmt(formatter),
            Self::StateUnavailable => formatter.write_str("virtual device state is unavailable"),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<EngineError> for CommandError {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::SlotOutOfBounds { slot } => Self::SlotOutOfBounds { slot },
            EngineError::TaskNotFound { task_key } => Self::TaskNotFound {
                task_key: task_key.as_str().to_owned(),
            },
            EngineError::EmptySlot { slot } => Self::EmptySlot { slot },
            EngineError::InvalidEffect { error } => Self::InvalidEffect { error },
        }
    }
}

#[tauri::command]
pub fn get_snapshot(state: tauri::State<'_, AppState>) -> Result<HaloSnapshot, CommandError> {
    get_snapshot_inner(&state)
}

#[tauri::command]
pub fn simulate_signal(
    state: tauri::State<'_, AppState>,
    input: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    simulate_signal_wire_inner(&state, input)
}

#[tauri::command]
pub fn manual_bind(
    state: tauri::State<'_, AppState>,
    input: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    manual_bind_wire_inner(&state, input)
}

#[tauri::command]
pub fn toggle_lock(
    state: tauri::State<'_, AppState>,
    slot: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    toggle_lock_wire_inner(&state, slot)
}

#[tauri::command]
pub fn swap_slots(
    state: tauri::State<'_, AppState>,
    left: Option<Value>,
    right: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    swap_slots_wire_inner(&state, left, right)
}

#[tauri::command]
pub fn update_effect(
    state: tauri::State<'_, AppState>,
    input: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    update_effect_wire_inner(&state, input)
}

#[tauri::command]
pub fn set_global_brightness(
    state: tauri::State<'_, AppState>,
    value: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    set_global_brightness_wire_inner(&state, value)
}

#[tauri::command]
pub fn reset_virtual_device(
    state: tauri::State<'_, AppState>,
) -> Result<HaloSnapshot, CommandError> {
    reset_virtual_device_inner(&state)
}

fn get_snapshot_inner(state: &AppState) -> Result<HaloSnapshot, CommandError> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| CommandError::StateUnavailable)?;
    Ok(engine.snapshot())
}

fn simulate_signal_wire_inner(
    state: &AppState,
    input: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    simulate_signal_inner(state, parse_wire_argument(input, CommandArgument::Input)?)
}

fn simulate_signal_inner(
    state: &AppState,
    input: SimulateSignalInput,
) -> Result<HaloSnapshot, CommandError> {
    let task_key = TaskKey::parse(&input.task_key).map_err(|_| CommandError::InvalidTaskKey)?;
    let signal_kind = parse_signal_kind(input.signal_kind)?;
    let normalized = normalize_signal(signal_kind, SignalSource::Simulator).map_err(|_| {
        CommandError::UnknownSignalKind {
            signal_kind: signal_kind_wire_value(signal_kind).to_owned(),
        }
    })?;
    mutate_and_snapshot(state, |engine| {
        engine.apply_signal(TaskSignal {
            task_key,
            state: normalized,
            received_at_ms: input.received_at_ms,
        });
        Ok(())
    })
}

fn manual_bind_wire_inner(
    state: &AppState,
    input: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    manual_bind_inner(state, parse_wire_argument(input, CommandArgument::Input)?)
}

fn manual_bind_inner(
    state: &AppState,
    input: ManualBindInput,
) -> Result<HaloSnapshot, CommandError> {
    let task_key = TaskKey::parse(&input.task_key).map_err(|_| CommandError::InvalidTaskKey)?;
    mutate_and_snapshot(state, |engine| {
        engine
            .manual_bind(&task_key, input.slot, input.lock)
            .map_err(CommandError::from)
    })
}

fn toggle_lock_wire_inner(
    state: &AppState,
    slot: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    toggle_lock_inner(state, parse_wire_argument(slot, CommandArgument::Slot)?)
}

fn toggle_lock_inner(state: &AppState, slot: usize) -> Result<HaloSnapshot, CommandError> {
    mutate_and_snapshot(state, |engine| {
        engine.toggle_lock(slot).map_err(CommandError::from)
    })
}

fn swap_slots_wire_inner(
    state: &AppState,
    left: Option<Value>,
    right: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    swap_slots_inner(
        state,
        parse_wire_argument(left, CommandArgument::Left)?,
        parse_wire_argument(right, CommandArgument::Right)?,
    )
}

fn swap_slots_inner(
    state: &AppState,
    left: usize,
    right: usize,
) -> Result<HaloSnapshot, CommandError> {
    mutate_and_snapshot(state, |engine| {
        engine.swap_slots(left, right).map_err(CommandError::from)
    })
}

fn update_effect_wire_inner(
    state: &AppState,
    input: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    update_effect_inner(state, parse_wire_argument(input, CommandArgument::Input)?)
}

fn update_effect_inner(
    state: &AppState,
    input: UpdateEffectInput,
) -> Result<HaloSnapshot, CommandError> {
    let brightness = validated_u8(EffectParameter::Brightness, input.brightness, 0, 100)?;
    let tail_percent = validated_u8(EffectParameter::TailPercent, input.tail_percent, 1, 100)?;
    let direction = parse_direction(input.direction)?;
    let effect = EffectProfile::new(brightness, input.speed_percent, direction, tail_percent)
        .map_err(|error| CommandError::InvalidEffect { error })?;

    mutate_and_snapshot(state, |engine| {
        engine
            .update_effect(input.slot, effect)
            .map_err(CommandError::from)
    })
}

fn set_global_brightness_wire_inner(
    state: &AppState,
    value: Option<Value>,
) -> Result<HaloSnapshot, CommandError> {
    set_global_brightness_inner(state, parse_wire_argument(value, CommandArgument::Value)?)
}

fn set_global_brightness_inner(state: &AppState, value: u16) -> Result<HaloSnapshot, CommandError> {
    let value = validated_u8(EffectParameter::GlobalBrightness, value, 0, 100)?;
    mutate_and_snapshot(state, |engine| {
        engine
            .set_global_brightness(value)
            .map_err(CommandError::from)
    })
}

fn reset_virtual_device_inner(state: &AppState) -> Result<HaloSnapshot, CommandError> {
    let mut engine = match state.engine.lock() {
        Ok(engine) => engine,
        Err(poisoned) => {
            let engine = poisoned.into_inner();
            state.engine.clear_poison();
            engine
        }
    };
    let previous_revision = engine.snapshot().revision;
    *engine =
        crate::domain::engine::HaloEngine::reset_after(ROUND_COMPLETE_HOLD_MS, previous_revision);
    Ok(engine.snapshot())
}

fn parse_wire_argument<T: DeserializeOwned>(
    value: Option<Value>,
    argument: CommandArgument,
) -> Result<T, CommandError> {
    value
        .ok_or(CommandError::InvalidInput { argument })
        .and_then(|value| {
            serde_json::from_value(value).map_err(|_| CommandError::InvalidInput { argument })
        })
}

fn mutate_and_snapshot(
    state: &AppState,
    operation: impl FnOnce(&mut crate::domain::engine::HaloEngine) -> Result<(), CommandError>,
) -> Result<HaloSnapshot, CommandError> {
    let mut engine = state
        .engine
        .lock()
        .map_err(|_| CommandError::StateUnavailable)?;
    operation(&mut engine)?;
    Ok(engine.snapshot())
}

fn parse_signal_kind(value: String) -> Result<SignalKind, CommandError> {
    match value.as_str() {
        "userPromptSubmit" => Ok(SignalKind::UserPromptSubmit),
        "preToolUse" => Ok(SignalKind::PreToolUse),
        "postToolUse" => Ok(SignalKind::PostToolUse),
        "permissionRequest" => Ok(SignalKind::PermissionRequest),
        "stop" => Ok(SignalKind::Stop),
        "failed" => Ok(SignalKind::Failed),
        _ => Err(CommandError::UnknownSignalKind { signal_kind: value }),
    }
}

const fn signal_kind_wire_value(value: SignalKind) -> &'static str {
    match value {
        SignalKind::UserPromptSubmit => "userPromptSubmit",
        SignalKind::PreToolUse => "preToolUse",
        SignalKind::PostToolUse => "postToolUse",
        SignalKind::PermissionRequest => "permissionRequest",
        SignalKind::Stop => "stop",
        SignalKind::Failed => "failed",
    }
}

fn parse_direction(value: String) -> Result<Direction, CommandError> {
    match value.as_str() {
        "clockwise" => Ok(Direction::Clockwise),
        "counterClockwise" => Ok(Direction::CounterClockwise),
        _ => Err(CommandError::UnknownDirection { direction: value }),
    }
}

fn validated_u8(
    field: EffectParameter,
    actual: u16,
    min: u16,
    max: u16,
) -> Result<u8, CommandError> {
    if !(min..=max).contains(&actual) {
        return Err(CommandError::InvalidEffect {
            error: EffectValidationError::OutOfRange {
                field,
                min,
                max,
                actual,
            },
        });
    }

    u8::try_from(actual).map_err(|_| CommandError::InvalidEffect {
        error: EffectValidationError::OutOfRange {
            field,
            min,
            max,
            actual,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppState;
    use crate::domain::effects::{Direction, EffectParameter, EffectValidationError};
    use crate::domain::model::{Confidence, DeviceMode, SignalSource, TaskStatus};

    fn state() -> AppState {
        AppState::default()
    }

    fn running(task_key: &str, received_at_ms: u64) -> SimulateSignalInput {
        SimulateSignalInput {
            task_key: task_key.to_owned(),
            signal_kind: "userPromptSubmit".to_owned(),
            received_at_ms,
        }
    }

    #[test]
    fn initial_snapshot_has_four_empty_slots_in_virtual_mode() {
        let snapshot = get_snapshot_inner(&state()).expect("fresh state must be readable");

        assert_eq!(snapshot.device_mode, DeviceMode::Virtual);
        assert_eq!(snapshot.slots.len(), 4);
        assert!(snapshot.slots.iter().all(|slot| slot.task_key.is_none()));
        assert_eq!(
            serde_json::to_value(&snapshot).unwrap()["deviceMode"],
            "virtual"
        );
    }

    #[test]
    fn simulated_signal_returns_the_new_state_for_the_same_task() {
        let state = state();
        let snapshot = simulate_signal_inner(
            &state,
            SimulateSignalInput {
                task_key: "0123456789abcdef".to_owned(),
                signal_kind: "permissionRequest".to_owned(),
                received_at_ms: 42,
            },
        )
        .expect("valid simulation must succeed");

        assert_eq!(
            snapshot.slots[0].task_key.as_ref().map(|key| key.as_str()),
            Some("0123456789abcdef")
        );
        assert_eq!(snapshot.slots[0].status, TaskStatus::Waiting);
        assert_eq!(snapshot.tasks[0].last_active_at_ms, 42);
    }

    #[test]
    fn simulated_failure_is_explicitly_simulated() {
        let snapshot = simulate_signal_inner(
            &state(),
            SimulateSignalInput {
                task_key: "fedcba9876543210".to_owned(),
                signal_kind: "failed".to_owned(),
                received_at_ms: 7,
            },
        )
        .expect("simulator may create a failure");

        assert_eq!(snapshot.slots[0].status, TaskStatus::Failed);
        assert_eq!(snapshot.slots[0].source, Some(SignalSource::Simulator));
        assert_eq!(snapshot.slots[0].confidence, Some(Confidence::Simulated));
    }

    #[test]
    fn every_mutation_returns_an_atomic_snapshot() {
        let state = state();
        simulate_signal_inner(&state, running("0000000000000001", 1)).unwrap();
        simulate_signal_inner(&state, running("0000000000000002", 2)).unwrap();

        let bound = manual_bind_inner(
            &state,
            ManualBindInput {
                task_key: "0000000000000001".to_owned(),
                slot: 1,
                lock: true,
            },
        )
        .unwrap();
        assert_eq!(
            bound.slots[1].task_key.as_ref().unwrap().as_str(),
            "0000000000000001"
        );
        assert!(bound.slots[1].locked);

        let unlocked = toggle_lock_inner(&state, 1).unwrap();
        assert!(!unlocked.slots[1].locked);

        let swapped = swap_slots_inner(&state, 0, 1).unwrap();
        assert_eq!(
            swapped.slots[0].task_key.as_ref().map(|key| key.as_str()),
            Some("0000000000000001")
        );

        let effect = update_effect_inner(
            &state,
            UpdateEffectInput {
                slot: 0,
                brightness: 45,
                speed_percent: 175,
                direction: "counterClockwise".to_owned(),
                tail_percent: 64,
            },
        )
        .unwrap();
        assert_eq!(effect.slots[0].effect.brightness(), 45);
        assert_eq!(effect.slots[0].effect.speed_percent(), 175);
        assert_eq!(
            effect.slots[0].effect.direction(),
            Direction::CounterClockwise
        );
        assert_eq!(effect.slots[0].effect.tail_percent(), 64);

        let dimmed = set_global_brightness_inner(&state, 30).unwrap();
        assert_eq!(dimmed.global_brightness, 30);

        let reset = reset_virtual_device_inner(&state).unwrap();
        assert_eq!(reset.revision, dimmed.revision + 1);
        assert_eq!(reset.device_mode, DeviceMode::Virtual);
        assert_eq!(reset.global_brightness, 100);
        assert!(reset.tasks.is_empty());
        assert!(reset.slots.iter().all(|slot| slot.task_key.is_none()));
    }

    #[test]
    fn invalid_and_unknown_inputs_return_serializable_structured_errors() {
        let state = state();

        assert_eq!(
            toggle_lock_inner(&state, 4),
            Err(CommandError::SlotOutOfBounds { slot: 4 })
        );
        assert_eq!(
            simulate_signal_inner(
                &state,
                SimulateSignalInput {
                    task_key: "not-a-task-key".to_owned(),
                    signal_kind: "stop".to_owned(),
                    received_at_ms: 1,
                },
            ),
            Err(CommandError::InvalidTaskKey)
        );
        assert_eq!(
            serde_json::to_value(CommandError::InvalidTaskKey).unwrap(),
            serde_json::json!({ "code": "invalidTaskKey" })
        );
        assert_eq!(
            simulate_signal_inner(
                &state,
                SimulateSignalInput {
                    task_key: "0123456789abcdef".to_owned(),
                    signal_kind: "completed".to_owned(),
                    received_at_ms: 1,
                },
            ),
            Err(CommandError::UnknownSignalKind {
                signal_kind: "completed".to_owned()
            })
        );
        assert_eq!(
            update_effect_inner(
                &state,
                UpdateEffectInput {
                    slot: 0,
                    brightness: 101,
                    speed_percent: 100,
                    direction: "clockwise".to_owned(),
                    tail_percent: 35,
                },
            ),
            Err(CommandError::InvalidEffect {
                error: EffectValidationError::OutOfRange {
                    field: EffectParameter::Brightness,
                    min: 0,
                    max: 100,
                    actual: 101,
                }
            })
        );
        assert_eq!(
            update_effect_inner(
                &state,
                UpdateEffectInput {
                    slot: 0,
                    brightness: 80,
                    speed_percent: 100,
                    direction: "reverse".to_owned(),
                    tail_percent: 35,
                },
            ),
            Err(CommandError::UnknownDirection {
                direction: "reverse".to_owned()
            })
        );

        assert_eq!(
            serde_json::to_value(CommandError::SlotOutOfBounds { slot: 4 }).unwrap(),
            serde_json::json!({ "code": "slotOutOfBounds", "slot": 4 })
        );

        let poisoned = AppState::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoned.engine.lock().unwrap();
            panic!("poison the test mutex");
        }));
        assert_eq!(
            get_snapshot_inner(&poisoned),
            Err(CommandError::StateUnavailable)
        );
        let reset = reset_virtual_device_inner(&poisoned)
            .expect("reset must recover and clear a poisoned state");
        assert!(reset.tasks.is_empty());
        assert_eq!(
            get_snapshot_inner(&poisoned).expect("state must be readable after reset"),
            reset
        );
        assert_eq!(
            serde_json::to_value(CommandError::StateUnavailable).unwrap(),
            serde_json::json!({ "code": "stateUnavailable" })
        );
    }

    #[test]
    fn tauri_wire_boundary_maps_every_invalid_json_shape_to_stable_errors() {
        let state = state();
        let invalid_input = Err(CommandError::InvalidInput {
            argument: CommandArgument::Input,
        });

        for value in [
            None,
            Some(serde_json::json!({
                "taskKey": "0123456789abcdef",
                "signalKind": "stop"
            })),
            Some(serde_json::json!({
                "taskKey": "0123456789abcdef",
                "signalKind": "stop",
                "receivedAtMs": -1
            })),
            Some(serde_json::json!({
                "taskKey": "0123456789abcdef",
                "signalKind": "stop",
                "receivedAtMs": "now"
            })),
            Some(serde_json::json!({
                "taskKey": "0123456789abcdef",
                "signalKind": "stop",
                "receivedAtMs": 1,
                "prompt": "must not be accepted"
            })),
        ] {
            assert_eq!(
                simulate_signal_wire_inner(&state, value),
                invalid_input,
                "missing fields, negative values, wrong types, and unknown fields must be stable"
            );
        }

        for value in [
            None,
            Some(serde_json::json!(-1)),
            Some(serde_json::json!("0")),
        ] {
            assert_eq!(
                toggle_lock_wire_inner(&state, value),
                Err(CommandError::InvalidInput {
                    argument: CommandArgument::Slot,
                })
            );
        }

        assert_eq!(
            update_effect_wire_inner(
                &state,
                Some(serde_json::json!({
                    "slot": 0,
                    "brightness": -1,
                    "speedPercent": 100,
                    "direction": "clockwise",
                    "tailPercent": 35
                })),
            ),
            invalid_input
        );
        assert_eq!(
            set_global_brightness_wire_inner(&state, None),
            Err(CommandError::InvalidInput {
                argument: CommandArgument::Value,
            })
        );
        assert_eq!(
            swap_slots_wire_inner(&state, None, Some(serde_json::json!(1))),
            Err(CommandError::InvalidInput {
                argument: CommandArgument::Left,
            })
        );

        assert_eq!(
            serde_json::to_value(CommandError::InvalidInput {
                argument: CommandArgument::Input,
            })
            .unwrap(),
            serde_json::json!({
                "code": "invalidInput",
                "argument": "input"
            })
        );

        let snapshot = simulate_signal_wire_inner(
            &state,
            Some(serde_json::json!({
                "taskKey": "0123456789abcdef",
                "signalKind": "userPromptSubmit",
                "receivedAtMs": 9
            })),
        )
        .expect("a valid camelCase input object must cross the wire boundary");
        assert_eq!(snapshot.slots[0].status, TaskStatus::Running);

        let snapshot = set_global_brightness_wire_inner(&state, Some(serde_json::json!(25)))
            .expect("a valid scalar must cross the wire boundary");
        assert_eq!(snapshot.global_brightness, 25);
    }
}
