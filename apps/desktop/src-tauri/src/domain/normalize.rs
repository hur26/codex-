use super::model::{NormalizedState, SignalKind, SignalSource};
use std::fmt;

pub fn normalize_signal(
    kind: SignalKind,
    source: SignalSource,
) -> Result<NormalizedState, NormalizeError> {
    if kind == SignalKind::Failed && source == SignalSource::Hook {
        return Err(NormalizeError::UnsupportedHookFailure);
    }

    let status = match kind {
        SignalKind::UserPromptSubmit | SignalKind::PreToolUse | SignalKind::PostToolUse => {
            super::model::TaskStatus::Running
        }
        SignalKind::PermissionRequest => super::model::TaskStatus::Waiting,
        SignalKind::Stop => super::model::TaskStatus::RoundCompleted,
        SignalKind::Failed => super::model::TaskStatus::Failed,
    };
    let confidence = match (source, kind) {
        (SignalSource::Simulator, _) => super::model::Confidence::Simulated,
        (SignalSource::Hook, SignalKind::PermissionRequest) => {
            super::model::Confidence::Provisional
        }
        (SignalSource::Hook, _) => super::model::Confidence::Observed,
    };

    Ok(NormalizedState {
        status,
        source,
        confidence,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizeError {
    UnsupportedHookFailure,
}

impl fmt::Display for NormalizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("hook signals cannot infer a failure state")
    }
}

impl std::error::Error for NormalizeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{Confidence, TaskKey, TaskStatus};

    #[test]
    fn hook_activity_signals_are_observed_running_states() {
        for kind in [
            SignalKind::UserPromptSubmit,
            SignalKind::PreToolUse,
            SignalKind::PostToolUse,
        ] {
            assert_eq!(
                normalize_signal(kind, SignalSource::Hook).unwrap(),
                NormalizedState {
                    status: TaskStatus::Running,
                    source: SignalSource::Hook,
                    confidence: Confidence::Observed,
                }
            );
        }
    }

    #[test]
    fn permission_request_is_provisional_waiting() {
        assert_eq!(
            normalize_signal(SignalKind::PermissionRequest, SignalSource::Hook).unwrap(),
            NormalizedState {
                status: TaskStatus::Waiting,
                source: SignalSource::Hook,
                confidence: Confidence::Provisional,
            }
        );
    }

    #[test]
    fn stop_only_marks_the_current_round_completed() {
        assert_eq!(
            normalize_signal(SignalKind::Stop, SignalSource::Hook).unwrap(),
            NormalizedState {
                status: TaskStatus::RoundCompleted,
                source: SignalSource::Hook,
                confidence: Confidence::Observed,
            }
        );
    }

    #[test]
    fn failed_is_only_available_to_the_simulator() {
        assert_eq!(
            normalize_signal(SignalKind::Failed, SignalSource::Simulator).unwrap(),
            NormalizedState {
                status: TaskStatus::Failed,
                source: SignalSource::Simulator,
                confidence: Confidence::Simulated,
            }
        );
        assert_eq!(
            normalize_signal(SignalKind::Failed, SignalSource::Hook),
            Err(NormalizeError::UnsupportedHookFailure)
        );
    }

    #[test]
    fn task_key_requires_exactly_sixteen_lowercase_hex_characters() {
        let valid = TaskKey::parse("0123456789abcdef").unwrap();
        assert_eq!(valid.as_str(), "0123456789abcdef");

        for invalid in [
            "",
            "0123456789abcde",
            "0123456789abcdef0",
            "0123456789abcdeF",
            "0123456789abcdeg",
            "0123456789abcd-",
        ] {
            assert!(
                TaskKey::parse(invalid).is_err(),
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn frontend_enums_and_task_keys_use_safe_json_shapes() {
        assert_eq!(
            serde_json::to_string(&TaskStatus::RoundCompleted).unwrap(),
            "\"roundCompleted\""
        );
        assert_eq!(
            serde_json::to_string(&SignalKind::PermissionRequest).unwrap(),
            "\"permissionRequest\""
        );

        let key_json = serde_json::to_string(&TaskKey::parse("0123456789abcdef").unwrap()).unwrap();
        assert_eq!(key_json, "\"0123456789abcdef\"");
        assert!(serde_json::from_str::<TaskKey>("\"0123456789abcdeF\"").is_err());
    }
}
