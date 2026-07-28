use crate::domain::model::TaskStatus;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt;

const DEFAULT_BRIGHTNESS: u8 = 80;
const DEFAULT_SPEED_PERCENT: u16 = 100;
const DEFAULT_TAIL_PERCENT: u8 = 35;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectParameter {
    GlobalBrightness,
    Brightness,
    SpeedPercent,
    TailPercent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EffectValidationError {
    OutOfRange {
        field: EffectParameter,
        min: u16,
        max: u16,
        actual: u16,
    },
}

impl fmt::Display for EffectValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange {
                field,
                min,
                max,
                actual,
            } => write!(
                formatter,
                "{field:?} must be between {min} and {max}, received {actual}"
            ),
        }
    }
}

impl std::error::Error for EffectValidationError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectProfile {
    brightness: u8,
    speed_percent: u16,
    direction: Direction,
    tail_percent: u8,
}

impl EffectProfile {
    pub fn new(
        brightness: u8,
        speed_percent: u16,
        direction: Direction,
        tail_percent: u8,
    ) -> Result<Self, EffectValidationError> {
        validate_range(EffectParameter::Brightness, u16::from(brightness), 0, 100)?;
        validate_range(EffectParameter::SpeedPercent, speed_percent, 25, 300)?;
        validate_range(
            EffectParameter::TailPercent,
            u16::from(tail_percent),
            1,
            100,
        )?;

        Ok(Self {
            brightness,
            speed_percent,
            direction,
            tail_percent,
        })
    }

    pub const fn brightness(&self) -> u8 {
        self.brightness
    }

    pub const fn speed_percent(&self) -> u16 {
        self.speed_percent
    }

    pub const fn direction(&self) -> Direction {
        self.direction
    }

    pub const fn tail_percent(&self) -> u8 {
        self.tail_percent
    }
}

impl Default for EffectProfile {
    fn default() -> Self {
        Self {
            brightness: DEFAULT_BRIGHTNESS,
            speed_percent: DEFAULT_SPEED_PERCENT,
            direction: Direction::Clockwise,
            tail_percent: DEFAULT_TAIL_PERCENT,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EffectProfileWire {
    brightness: u8,
    speed_percent: u16,
    direction: Direction,
    tail_percent: u8,
}

impl<'de> Deserialize<'de> for EffectProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = EffectProfileWire::deserialize(deserializer)?;
        Self::new(
            value.brightness,
            value.speed_percent,
            value.direction,
            value.tail_percent,
        )
        .map_err(de::Error::custom)
    }
}

pub(crate) fn validate_global_brightness(value: u8) -> Result<(), EffectValidationError> {
    validate_range(EffectParameter::GlobalBrightness, u16::from(value), 0, 100)
}

fn validate_range(
    field: EffectParameter,
    actual: u16,
    min: u16,
    max: u16,
) -> Result<(), EffectValidationError> {
    if (min..=max).contains(&actual) {
        Ok(())
    } else {
        Err(EffectValidationError::OutOfRange {
            field,
            min,
            max,
            actual,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn red(&self) -> u8 {
        self.red
    }

    pub const fn green(&self) -> u8 {
        self.green
    }

    pub const fn blue(&self) -> u8 {
        self.blue
    }
}

pub const fn status_color(status: TaskStatus) -> Rgb {
    match status {
        TaskStatus::Running => Rgb::new(255, 185, 58),
        TaskStatus::Waiting => Rgb::new(255, 135, 43),
        TaskStatus::RoundCompleted => Rgb::new(83, 229, 151),
        TaskStatus::Failed => Rgb::new(255, 85, 91),
        TaskStatus::Queued => Rgb::new(159, 113, 255),
        TaskStatus::Idle => Rgb::new(45, 50, 55),
        TaskStatus::Unknown => Rgb::new(76, 174, 255),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        status_color, Direction, EffectParameter, EffectProfile, EffectValidationError, Rgb,
    };
    use crate::domain::engine::{EngineError, HaloEngine};
    use crate::domain::model::TaskStatus;

    fn profile(
        brightness: u8,
        speed_percent: u16,
        direction: Direction,
        tail_percent: u8,
    ) -> EffectProfile {
        EffectProfile::new(brightness, speed_percent, direction, tail_percent)
            .expect("test profile must be valid")
    }

    #[test]
    fn global_and_per_ring_brightness_only_accept_zero_through_one_hundred() {
        let mut engine = HaloEngine::new(300_000);

        assert_eq!(engine.set_global_brightness(0), Ok(()));
        assert_eq!(engine.set_global_brightness(100), Ok(()));
        assert_eq!(
            engine.set_global_brightness(101),
            Err(EngineError::InvalidEffect {
                error: EffectValidationError::OutOfRange {
                    field: EffectParameter::GlobalBrightness,
                    min: 0,
                    max: 100,
                    actual: 101,
                },
            })
        );
        assert_eq!(
            serde_json::to_value(
                engine
                    .set_global_brightness(101)
                    .expect_err("out-of-range brightness must fail")
            )
            .expect("engine error must serialize"),
            serde_json::json!({
                "code": "invalidEffect",
                "error": {
                    "code": "outOfRange",
                    "field": "globalBrightness",
                    "min": 0,
                    "max": 100,
                    "actual": 101
                }
            })
        );
        assert_eq!(engine.snapshot().global_brightness, 100);

        assert!(EffectProfile::new(0, 100, Direction::Clockwise, 30).is_ok());
        assert!(EffectProfile::new(100, 100, Direction::Clockwise, 30).is_ok());
        assert!(serde_json::from_value::<EffectProfile>(serde_json::json!({
            "brightness": 101,
            "speedPercent": 100,
            "direction": "clockwise",
            "tailPercent": 30
        }))
        .is_err());
    }

    #[test]
    fn speed_only_accepts_twenty_five_through_three_hundred_percent() {
        assert!(EffectProfile::new(80, 25, Direction::Clockwise, 30).is_ok());
        assert!(EffectProfile::new(80, 300, Direction::Clockwise, 30).is_ok());
        assert_eq!(
            EffectProfile::new(80, 24, Direction::Clockwise, 30),
            Err(EffectValidationError::OutOfRange {
                field: EffectParameter::SpeedPercent,
                min: 25,
                max: 300,
                actual: 24,
            })
        );
        assert!(serde_json::from_value::<EffectProfile>(serde_json::json!({
            "brightness": 80,
            "speedPercent": 301,
            "direction": "clockwise",
            "tailPercent": 30
        }))
        .is_err());
    }

    #[test]
    fn tail_only_accepts_one_through_one_hundred_percent() {
        assert!(EffectProfile::new(80, 100, Direction::Clockwise, 1).is_ok());
        assert!(EffectProfile::new(80, 100, Direction::Clockwise, 100).is_ok());
        assert_eq!(
            EffectProfile::new(80, 100, Direction::Clockwise, 0),
            Err(EffectValidationError::OutOfRange {
                field: EffectParameter::TailPercent,
                min: 1,
                max: 100,
                actual: 0,
            })
        );
        assert!(serde_json::from_value::<EffectProfile>(serde_json::json!({
            "brightness": 80,
            "speedPercent": 100,
            "direction": "clockwise",
            "tailPercent": 101
        }))
        .is_err());
    }

    #[test]
    fn direction_only_accepts_the_two_supported_wire_values() {
        for value in ["clockwise", "counterClockwise"] {
            let decoded = serde_json::from_value::<EffectProfile>(serde_json::json!({
                "brightness": 80,
                "speedPercent": 100,
                "direction": value,
                "tailPercent": 30
            }));
            assert!(decoded.is_ok(), "{value} must be accepted");
        }

        for value in ["reverse", "counterclockwise", "CLOCKWISE"] {
            let decoded = serde_json::from_value::<EffectProfile>(serde_json::json!({
                "brightness": 80,
                "speedPercent": 100,
                "direction": value,
                "tailPercent": 30
            }));
            assert!(decoded.is_err(), "{value} must be rejected");
        }
    }

    #[test]
    fn updating_one_slot_changes_only_that_slots_effect_in_the_snapshot() {
        let mut engine = HaloEngine::new(300_000);
        let before = engine.snapshot();
        let changed = profile(42, 175, Direction::CounterClockwise, 66);

        engine
            .update_effect(2, changed.clone())
            .expect("valid slot and profile must update");

        let after = engine.snapshot();
        assert_eq!(after.slots[2].effect, changed);
        for slot in [0, 1, 3] {
            assert_eq!(after.slots[slot].effect, before.slots[slot].effect);
        }
        assert_eq!(
            engine.update_effect(4, profile(50, 100, Direction::Clockwise, 50)),
            Err(EngineError::SlotOutOfBounds { slot: 4 })
        );
    }

    #[test]
    fn status_colors_are_fixed_typed_presets_not_free_form_strings() {
        assert_eq!(status_color(TaskStatus::Running), Rgb::new(255, 185, 58));
        assert_eq!(status_color(TaskStatus::Waiting), Rgb::new(255, 135, 43));
        assert_eq!(
            status_color(TaskStatus::RoundCompleted),
            Rgb::new(83, 229, 151)
        );
        assert_eq!(status_color(TaskStatus::Failed), Rgb::new(255, 85, 91));
        assert_eq!(status_color(TaskStatus::Queued), Rgb::new(159, 113, 255));
        assert_eq!(status_color(TaskStatus::Idle), Rgb::new(45, 50, 55));
        assert_eq!(status_color(TaskStatus::Unknown), Rgb::new(76, 174, 255));

        assert!(
            serde_json::from_value::<EffectProfile>(serde_json::json!({
                "brightness": 80,
                "speedPercent": 100,
                "direction": "clockwise",
                "tailPercent": 30,
                "color": "#ff00ff"
            }))
            .is_err(),
            "free-form color input must not be accepted"
        );
    }
}
