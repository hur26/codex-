use crate::domain::effects::{Direction, EffectProfile};
use crate::domain::model::{DisplayMode, HaloSnapshot, RingSlot, TaskStatus};
use std::array;

const RING_COUNT: u8 = 4;
const NO_SELECTED_RING: u8 = 0xff;
const MAX_PERCENT: u16 = 100;
const MIN_SPEED_PERCENT: u16 = 25;
const MAX_SPEED_PERCENT: u16 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub revision: u64,
    pub global_brightness: u8,
    pub display_mode: DeviceDisplayMode,
    pub selected_ring: Option<u8>,
    pub rings: [DeviceRing; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRing {
    pub index: u8,
    pub status: DeviceTaskStatus,
    pub brightness: u8,
    pub speed_percent: u16,
    pub direction: DeviceDirection,
    pub tail_percent: u8,
    pub label: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceUpdate {
    Ring(DeviceRing),
    Display {
        mode: DeviceDisplayMode,
        selected_ring: Option<u8>,
    },
    Brightness(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceDisplayMode {
    Ambient,
    Overview,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceTaskStatus {
    Running,
    Waiting,
    RoundCompleted,
    Failed,
    Queued,
    Idle,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadField {
    GlobalBrightness,
    RingBrightness,
    SpeedPercent,
    TailPercent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayloadError {
    RingIndexOutOfRange { index: u8 },
    SelectedRingOutOfRange { selected_ring: u8 },
    PercentageOutOfRange { field: PayloadField, value: u16 },
    LabelNotEmpty,
}

impl DeviceSnapshot {
    pub fn from_halo(snapshot: &HaloSnapshot) -> Self {
        Self {
            revision: snapshot.revision,
            global_brightness: snapshot.global_brightness,
            display_mode: snapshot.display_mode.into(),
            selected_ring: snapshot
                .selected_slot
                .and_then(|index| u8::try_from(index).ok())
                .filter(|index| *index < RING_COUNT),
            rings: array::from_fn(|index| {
                snapshot
                    .slots
                    .get(index)
                    .map_or_else(|| DeviceRing::empty(index as u8), |slot| slot.into())
            }),
        }
    }

    pub fn encode_payload(&self) -> Result<Vec<u8>, PayloadError> {
        validate_percentage(
            PayloadField::GlobalBrightness,
            u16::from(self.global_brightness),
        )?;
        let selected_ring = encode_selected_ring(self.selected_ring)?;
        let encoded_rings = self
            .rings
            .iter()
            .map(DeviceRing::encode_payload)
            .collect::<Result<Vec<_>, _>>()?;
        let capacity = 12 + encoded_rings.iter().map(Vec::len).sum::<usize>();
        let mut payload = Vec::with_capacity(capacity);

        payload.extend_from_slice(&self.revision.to_le_bytes());
        payload.push(self.global_brightness);
        payload.push(self.display_mode.wire_value());
        payload.push(selected_ring);
        payload.push(RING_COUNT);
        for ring in encoded_rings {
            payload.extend_from_slice(&ring);
        }

        Ok(payload)
    }

    pub fn diff(&self, previous: &Self) -> Vec<DeviceUpdate> {
        let mut updates = Vec::new();

        for (ring, previous_ring) in self.rings.iter().zip(&previous.rings) {
            if ring != previous_ring {
                updates.push(DeviceUpdate::Ring(ring.clone()));
            }
        }
        if self.display_mode != previous.display_mode
            || self.selected_ring != previous.selected_ring
        {
            updates.push(DeviceUpdate::Display {
                mode: self.display_mode,
                selected_ring: self.selected_ring,
            });
        }
        if self.global_brightness != previous.global_brightness {
            updates.push(DeviceUpdate::Brightness(self.global_brightness));
        }

        updates
    }
}

impl DeviceRing {
    pub fn encode_payload(&self) -> Result<Vec<u8>, PayloadError> {
        if self.index >= RING_COUNT {
            return Err(PayloadError::RingIndexOutOfRange { index: self.index });
        }
        validate_percentage(PayloadField::RingBrightness, u16::from(self.brightness))?;
        validate_speed_percent(self.speed_percent)?;
        validate_percentage(PayloadField::TailPercent, u16::from(self.tail_percent))?;
        if !self.label.is_empty() {
            return Err(PayloadError::LabelNotEmpty);
        }

        let mut payload = Vec::with_capacity(8);
        payload.push(self.index);
        payload.push(self.status.wire_value());
        payload.push(self.brightness);
        payload.extend_from_slice(&self.speed_percent.to_le_bytes());
        payload.push(self.direction.wire_value());
        payload.push(self.tail_percent);
        payload.push(0);
        Ok(payload)
    }

    fn empty(index: u8) -> Self {
        let effect = EffectProfile::default();
        Self {
            index,
            status: DeviceTaskStatus::Idle,
            brightness: effect.brightness(),
            speed_percent: effect.speed_percent(),
            direction: effect.direction().into(),
            tail_percent: effect.tail_percent(),
            label: Vec::new(),
        }
    }
}

impl DeviceUpdate {
    pub fn encode_payload(&self) -> Result<Vec<u8>, PayloadError> {
        match self {
            Self::Ring(ring) => ring.encode_payload(),
            Self::Display {
                mode,
                selected_ring,
            } => Ok(vec![
                mode.wire_value(),
                encode_selected_ring(*selected_ring)?,
            ]),
            Self::Brightness(brightness) => {
                validate_percentage(PayloadField::GlobalBrightness, u16::from(*brightness))?;
                Ok(vec![*brightness])
            }
        }
    }
}

impl DeviceDisplayMode {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Ambient => 0,
            Self::Overview => 1,
            Self::Detail => 2,
        }
    }
}

impl DeviceTaskStatus {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Running => 1,
            Self::Waiting => 2,
            Self::RoundCompleted => 3,
            Self::Failed => 4,
            Self::Queued => 5,
            Self::Idle => 6,
            Self::Unknown => 7,
        }
    }
}

impl DeviceDirection {
    const fn wire_value(self) -> u8 {
        match self {
            Self::Clockwise => 0,
            Self::CounterClockwise => 1,
        }
    }
}

impl From<DisplayMode> for DeviceDisplayMode {
    fn from(value: DisplayMode) -> Self {
        match value {
            DisplayMode::Ambient => Self::Ambient,
            DisplayMode::Overview => Self::Overview,
            DisplayMode::Detail => Self::Detail,
        }
    }
}

impl From<TaskStatus> for DeviceTaskStatus {
    fn from(value: TaskStatus) -> Self {
        match value {
            TaskStatus::Running => Self::Running,
            TaskStatus::Waiting => Self::Waiting,
            TaskStatus::RoundCompleted => Self::RoundCompleted,
            TaskStatus::Failed => Self::Failed,
            TaskStatus::Queued => Self::Queued,
            TaskStatus::Idle => Self::Idle,
            TaskStatus::Unknown => Self::Unknown,
        }
    }
}

impl From<Direction> for DeviceDirection {
    fn from(value: Direction) -> Self {
        match value {
            Direction::Clockwise => Self::Clockwise,
            Direction::CounterClockwise => Self::CounterClockwise,
        }
    }
}

impl From<&RingSlot> for DeviceRing {
    fn from(slot: &RingSlot) -> Self {
        Self {
            index: slot.index as u8,
            status: slot.status.into(),
            brightness: slot.effect.brightness(),
            speed_percent: slot.effect.speed_percent(),
            direction: slot.effect.direction().into(),
            tail_percent: slot.effect.tail_percent(),
            label: Vec::new(),
        }
    }
}

fn encode_selected_ring(selected_ring: Option<u8>) -> Result<u8, PayloadError> {
    match selected_ring {
        Some(selected_ring) if selected_ring < RING_COUNT => Ok(selected_ring),
        Some(selected_ring) => Err(PayloadError::SelectedRingOutOfRange { selected_ring }),
        None => Ok(NO_SELECTED_RING),
    }
}

fn validate_percentage(field: PayloadField, value: u16) -> Result<(), PayloadError> {
    if value <= MAX_PERCENT {
        Ok(())
    } else {
        Err(PayloadError::PercentageOutOfRange { field, value })
    }
}

fn validate_speed_percent(value: u16) -> Result<(), PayloadError> {
    if (MIN_SPEED_PERCENT..=MAX_SPEED_PERCENT).contains(&value) {
        Ok(())
    } else {
        Err(PayloadError::PercentageOutOfRange {
            field: PayloadField::SpeedPercent,
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceDirection, DeviceDisplayMode, DeviceRing, DeviceSnapshot, DeviceTaskStatus,
        DeviceUpdate, PayloadError, PayloadField,
    };
    use crate::domain::effects::{Direction, EffectProfile};
    use crate::domain::model::{
        BindingMode, DeviceMode, DisplayMode, HaloSnapshot, RingSlot, TaskKey, TaskStatus,
    };

    fn base_snapshot() -> HaloSnapshot {
        HaloSnapshot {
            revision: 42,
            device_mode: DeviceMode::Virtual,
            global_brightness: 73,
            display_mode: DisplayMode::Detail,
            selected_slot: Some(2),
            slots: (0..4)
                .map(|index| RingSlot {
                    index,
                    task_key: Some(
                        TaskKey::parse(format!("0123456789abcde{index}"))
                            .expect("fixture key must be valid"),
                    ),
                    status: TaskStatus::Running,
                    source: None,
                    confidence: None,
                    binding_mode: BindingMode::Auto,
                    locked: false,
                    effect: EffectProfile::new(
                        40 + index as u8,
                        75 + index as u16,
                        if index % 2 == 0 {
                            Direction::Clockwise
                        } else {
                            Direction::CounterClockwise
                        },
                        20 + index as u8,
                    )
                    .expect("fixture effect must be valid"),
                })
                .collect(),
            tasks: Vec::new(),
            queue: Vec::new(),
        }
    }

    #[test]
    fn projection_contains_four_ring_states_but_no_task_identity() {
        let snapshot = base_snapshot();
        let projected = DeviceSnapshot::from_halo(&snapshot);
        let payload = projected.encode_payload().unwrap();

        assert_eq!(projected.rings.len(), 4);
        assert_eq!(projected.revision, snapshot.revision);
        assert_eq!(projected.display_mode, DeviceDisplayMode::Detail);
        assert!(!String::from_utf8_lossy(&payload).contains("0123456789abcdef"));
        assert!(projected.rings.iter().all(|ring| ring.label.is_empty()));
    }

    #[test]
    fn delta_sends_only_changed_ring_or_global_fields() {
        let before = DeviceSnapshot::from_halo(&base_snapshot());
        let mut changed = base_snapshot();
        changed.slots[2].status = TaskStatus::Waiting;
        let after = DeviceSnapshot::from_halo(&changed);

        assert_eq!(
            after.diff(&before),
            vec![DeviceUpdate::Ring(after.rings[2].clone())]
        );
    }

    #[test]
    fn full_snapshot_and_updates_match_protocol_payload_layouts() {
        let snapshot = DeviceSnapshot::from_halo(&base_snapshot());
        let payload = snapshot.encode_payload().expect("snapshot must encode");
        let mut expected = Vec::new();
        expected.extend_from_slice(&42_u64.to_le_bytes());
        expected.extend_from_slice(&[73, 2, 2, 4]);
        expected.extend_from_slice(&[0, 1, 40, 75, 0, 0, 20, 0]);
        expected.extend_from_slice(&[1, 1, 41, 76, 0, 1, 21, 0]);
        expected.extend_from_slice(&[2, 1, 42, 77, 0, 0, 22, 0]);
        expected.extend_from_slice(&[3, 1, 43, 78, 0, 1, 23, 0]);
        assert_eq!(payload, expected);

        assert_eq!(
            DeviceUpdate::Ring(snapshot.rings[1].clone()).encode_payload(),
            Ok(vec![1, 1, 41, 76, 0, 1, 21, 0])
        );
        assert_eq!(
            DeviceUpdate::Display {
                mode: DeviceDisplayMode::Overview,
                selected_ring: None,
            }
            .encode_payload(),
            Ok(vec![1, 0xff])
        );
        assert_eq!(DeviceUpdate::Brightness(65).encode_payload(), Ok(vec![65]));
    }

    #[test]
    fn ring_status_values_and_direction_values_are_fixed() {
        let statuses = [
            DeviceTaskStatus::Running,
            DeviceTaskStatus::Waiting,
            DeviceTaskStatus::RoundCompleted,
            DeviceTaskStatus::Failed,
            DeviceTaskStatus::Queued,
            DeviceTaskStatus::Idle,
            DeviceTaskStatus::Unknown,
        ];

        for (offset, status) in statuses.into_iter().enumerate() {
            let ring = DeviceRing {
                index: 0,
                status,
                brightness: 50,
                speed_percent: 100,
                direction: DeviceDirection::CounterClockwise,
                tail_percent: 25,
                label: Vec::new(),
            };
            let payload = ring.encode_payload().expect("ring must encode");
            assert_eq!(payload[1], offset as u8 + 1);
            assert_eq!(payload[5], 1);
        }
    }

    #[test]
    fn projected_speed_three_hundred_encodes_as_little_endian_u16() {
        let mut halo = base_snapshot();
        halo.slots[0].effect = EffectProfile::new(50, 300, Direction::Clockwise, 25)
            .expect("domain speed must allow three hundred percent");
        let projected = DeviceSnapshot::from_halo(&halo);

        let payload = projected.rings[0]
            .encode_payload()
            .expect("protocol must encode a valid domain speed");

        assert_eq!(&payload[3..5], &[0x2c, 0x01]);
    }

    #[test]
    fn encoding_rejects_non_empty_labels_even_when_short_utf8() {
        let mut ring = DeviceSnapshot::from_halo(&base_snapshot()).rings[0].clone();
        ring.label = b"safe".to_vec();

        assert_eq!(ring.encode_payload(), Err(PayloadError::LabelNotEmpty));
    }

    #[test]
    fn payload_encoders_reject_protocol_bound_violations() {
        let mut ring = DeviceSnapshot::from_halo(&base_snapshot()).rings[0].clone();
        ring.index = 4;
        assert_eq!(
            ring.encode_payload(),
            Err(PayloadError::RingIndexOutOfRange { index: 4 })
        );

        ring.index = 0;
        ring.speed_percent = 24;
        assert_eq!(
            ring.encode_payload(),
            Err(PayloadError::PercentageOutOfRange {
                field: PayloadField::SpeedPercent,
                value: 24,
            })
        );

        ring.speed_percent = 301;
        assert_eq!(
            ring.encode_payload(),
            Err(PayloadError::PercentageOutOfRange {
                field: PayloadField::SpeedPercent,
                value: 301,
            })
        );
        assert_eq!(
            DeviceUpdate::Display {
                mode: DeviceDisplayMode::Detail,
                selected_ring: Some(4),
            }
            .encode_payload(),
            Err(PayloadError::SelectedRingOutOfRange { selected_ring: 4 })
        );
        assert_eq!(
            DeviceUpdate::Brightness(101).encode_payload(),
            Err(PayloadError::PercentageOutOfRange {
                field: PayloadField::GlobalBrightness,
                value: 101,
            })
        );
    }

    #[test]
    fn diff_order_is_rings_then_display_then_brightness() {
        let before = DeviceSnapshot::from_halo(&base_snapshot());
        let mut after = before.clone();
        after.rings[3].status = DeviceTaskStatus::Failed;
        after.rings[0].status = DeviceTaskStatus::Waiting;
        after.display_mode = DeviceDisplayMode::Ambient;
        after.selected_ring = None;
        after.global_brightness = 50;

        assert_eq!(
            after.diff(&before),
            vec![
                DeviceUpdate::Ring(after.rings[0].clone()),
                DeviceUpdate::Ring(after.rings[3].clone()),
                DeviceUpdate::Display {
                    mode: DeviceDisplayMode::Ambient,
                    selected_ring: None,
                },
                DeviceUpdate::Brightness(50),
            ]
        );
    }
}
