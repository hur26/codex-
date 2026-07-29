use crate::domain::effects::{validate_global_brightness, EffectProfile, EffectValidationError};
use crate::domain::model::{
    BindingMode, DeviceMode, HaloSnapshot, RingSlot, TaskKey, TaskRecord, TaskSignal, TaskStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

const SLOT_COUNT: usize = 4;

#[derive(Clone, Debug)]
pub struct HaloEngine {
    round_complete_hold_ms: u64,
    revision: u64,
    global_brightness: u8,
    tasks: HashMap<TaskKey, TaskRecord>,
    slots: Vec<RingSlot>,
    queue: Vec<TaskKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EngineError {
    SlotOutOfBounds { slot: usize },
    TaskNotFound { task_key: TaskKey },
    EmptySlot { slot: usize },
    InvalidEffect { error: EffectValidationError },
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotOutOfBounds { slot } => write!(formatter, "slot {slot} is out of bounds"),
            Self::TaskNotFound { task_key } => {
                write!(formatter, "task {} does not exist", task_key.as_str())
            }
            Self::EmptySlot { slot } => write!(formatter, "slot {slot} is empty"),
            Self::InvalidEffect { error } => error.fmt(formatter),
        }
    }
}

impl std::error::Error for EngineError {}

impl HaloEngine {
    pub fn new(round_complete_hold_ms: u64) -> Self {
        let slots = (0..SLOT_COUNT).map(Self::empty_slot).collect();

        Self {
            round_complete_hold_ms,
            revision: 0,
            global_brightness: 100,
            tasks: HashMap::new(),
            slots,
            queue: Vec::new(),
        }
    }

    pub(crate) fn reset_after(round_complete_hold_ms: u64, revision: u64) -> Self {
        let mut engine = Self::new(round_complete_hold_ms);
        engine.revision = revision.saturating_add(1);
        engine
    }

    pub fn apply_signal(&mut self, signal: TaskSignal) {
        if let Some(task) = self.tasks.get(&signal.task_key) {
            if signal.received_at_ms < task.last_active_at_ms
                || (signal.received_at_ms == task.last_active_at_ms
                    && signal.state.status == task.status
                    && signal.state.source == task.source
                    && signal.state.confidence == task.confidence)
            {
                return;
            }
        }

        let task_key = signal.task_key;
        let task = TaskRecord {
            task_key: task_key.clone(),
            status: signal.state.status,
            source: signal.state.source,
            confidence: signal.state.confidence,
            last_active_at_ms: signal.received_at_ms,
        };
        self.tasks.insert(task_key.clone(), task);

        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.task_key.as_ref() == Some(&task_key))
        {
            slot.status = signal.state.status;
            slot.source = Some(signal.state.source);
            slot.confidence = Some(signal.state.confidence);
            self.remove_from_queue(&task_key);
            self.advance_revision();
            return;
        }

        if let Some(slot) = self.slots.iter().position(|slot| slot.task_key.is_none()) {
            self.bind_slot(slot, task_key, BindingMode::Auto, false);
        } else {
            self.enqueue_recent(task_key);
        }
        self.advance_revision();
    }

    pub fn manual_bind(
        &mut self,
        task: &TaskKey,
        slot: usize,
        lock: bool,
    ) -> Result<(), EngineError> {
        self.validate_slot(slot)?;
        if !self.tasks.contains_key(task) {
            return Err(EngineError::TaskNotFound {
                task_key: task.clone(),
            });
        }

        let current_slot = self
            .slots
            .iter()
            .position(|candidate| candidate.task_key.as_ref() == Some(task));

        if current_slot == Some(slot) {
            let changed = self.slots[slot].binding_mode != BindingMode::Manual
                || self.slots[slot].locked != lock;
            self.bind_slot(slot, task.clone(), BindingMode::Manual, lock);
            self.remove_from_queue(task);
            if changed {
                self.advance_revision();
            }
            return Ok(());
        }

        let displaced = self.slots[slot].task_key.clone();
        if let Some(current_slot) = current_slot {
            self.clear_slot(current_slot);
        }
        self.remove_from_queue(task);
        self.bind_slot(slot, task.clone(), BindingMode::Manual, lock);

        if let Some(displaced) = displaced {
            self.enqueue_recent(displaced);
        }
        self.fill_empty_slots_from_queue();

        self.advance_revision();
        Ok(())
    }

    pub fn toggle_lock(&mut self, slot: usize) -> Result<(), EngineError> {
        self.validate_slot(slot)?;
        if self.slots[slot].task_key.is_none() {
            return Err(EngineError::EmptySlot { slot });
        }

        self.slots[slot].locked = !self.slots[slot].locked;
        self.advance_revision();
        Ok(())
    }

    pub fn swap_slots(&mut self, left: usize, right: usize) -> Result<(), EngineError> {
        self.validate_slot(left)?;
        self.validate_slot(right)?;

        let before = self.slots.clone();
        let left_effect = self.slots[left].effect.clone();
        let right_effect = self.slots[right].effect.clone();
        self.slots.swap(left, right);
        self.slots[left].index = left;
        self.slots[left].effect = left_effect;
        self.slots[right].index = right;
        self.slots[right].effect = right_effect;
        if self.slots != before {
            self.advance_revision();
        }
        Ok(())
    }

    pub fn update_effect(&mut self, slot: usize, effect: EffectProfile) -> Result<(), EngineError> {
        self.validate_slot(slot)?;
        if self.slots[slot].effect == effect {
            return Ok(());
        }
        self.slots[slot].effect = effect;
        self.advance_revision();
        Ok(())
    }

    pub fn set_global_brightness(&mut self, value: u8) -> Result<(), EngineError> {
        validate_global_brightness(value).map_err(|error| EngineError::InvalidEffect { error })?;
        if self.global_brightness == value {
            return Ok(());
        }
        self.global_brightness = value;
        self.advance_revision();
        Ok(())
    }

    pub fn tick(&mut self, now_ms: u64) {
        let releasable: Vec<_> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                if slot.binding_mode != BindingMode::Auto
                    || slot.locked
                    || slot.status != TaskStatus::RoundCompleted
                {
                    return None;
                }

                let task_key = slot.task_key.as_ref()?;
                let completed_at_ms = self.tasks.get(task_key)?.last_active_at_ms;
                (now_ms >= completed_at_ms.saturating_add(self.round_complete_hold_ms))
                    .then_some(index)
            })
            .collect();

        let changed = !releasable.is_empty();
        for slot in releasable {
            self.clear_slot(slot);
        }
        self.fill_empty_slots_from_queue();
        if changed {
            self.advance_revision();
        }
    }

    pub fn snapshot(&self) -> HaloSnapshot {
        let mut tasks: Vec<_> = self.tasks.values().cloned().collect();
        tasks.sort_by(|left, right| {
            right
                .last_active_at_ms
                .cmp(&left.last_active_at_ms)
                .then_with(|| left.task_key.as_str().cmp(right.task_key.as_str()))
        });

        let queue = self
            .queue
            .iter()
            .filter_map(|task_key| self.tasks.get(task_key))
            .cloned()
            .map(|mut task| {
                task.status = TaskStatus::Queued;
                task
            })
            .collect();

        HaloSnapshot {
            revision: self.revision,
            device_mode: DeviceMode::Virtual,
            global_brightness: self.global_brightness,
            slots: self.slots.clone(),
            tasks,
            queue,
        }
    }

    fn empty_slot(index: usize) -> RingSlot {
        RingSlot {
            index,
            task_key: None,
            status: TaskStatus::Idle,
            source: None,
            confidence: None,
            binding_mode: BindingMode::Auto,
            locked: false,
            effect: EffectProfile::default(),
        }
    }

    fn bind_slot(
        &mut self,
        slot: usize,
        task_key: TaskKey,
        binding_mode: BindingMode,
        locked: bool,
    ) {
        let task = self
            .tasks
            .get(&task_key)
            .expect("binding is only called for known tasks");
        let slot = &mut self.slots[slot];
        slot.task_key = Some(task_key);
        slot.status = task.status;
        slot.source = Some(task.source);
        slot.confidence = Some(task.confidence);
        slot.binding_mode = binding_mode;
        slot.locked = locked;
    }

    fn clear_slot(&mut self, slot: usize) {
        let slot = &mut self.slots[slot];
        slot.task_key = None;
        slot.status = TaskStatus::Idle;
        slot.source = None;
        slot.confidence = None;
        slot.binding_mode = BindingMode::Auto;
        slot.locked = false;
    }

    fn enqueue_recent(&mut self, task_key: TaskKey) {
        self.remove_from_queue(&task_key);
        self.queue.push(task_key);
        let tasks = &self.tasks;
        self.queue.sort_by(|left, right| {
            let left_active = tasks.get(left).map_or(0, |task| task.last_active_at_ms);
            let right_active = tasks.get(right).map_or(0, |task| task.last_active_at_ms);
            right_active
                .cmp(&left_active)
                .then_with(|| left.as_str().cmp(right.as_str()))
        });
    }

    fn remove_from_queue(&mut self, task_key: &TaskKey) {
        self.queue.retain(|queued| queued != task_key);
    }

    fn fill_empty_slots_from_queue(&mut self) {
        while let Some(slot) = self.slots.iter().position(|slot| slot.task_key.is_none()) {
            if self.queue.is_empty() {
                return;
            }
            let task_key = self.queue.remove(0);
            if self.tasks.contains_key(&task_key) {
                self.bind_slot(slot, task_key, BindingMode::Auto, false);
            }
        }
    }

    fn validate_slot(&self, slot: usize) -> Result<(), EngineError> {
        if slot < self.slots.len() {
            Ok(())
        } else {
            Err(EngineError::SlotOutOfBounds { slot })
        }
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::effects::{Direction, EffectProfile};
    use crate::domain::model::{
        BindingMode, Confidence, NormalizedState, SignalSource, TaskKey, TaskSignal, TaskStatus,
    };

    fn key(value: u64) -> TaskKey {
        TaskKey::parse(format!("{value:016x}")).expect("test key must be valid")
    }

    fn signal(value: u64, status: TaskStatus, received_at_ms: u64) -> TaskSignal {
        TaskSignal {
            task_key: key(value),
            state: NormalizedState {
                status,
                source: SignalSource::Hook,
                confidence: Confidence::Observed,
            },
            received_at_ms,
        }
    }

    #[test]
    fn four_recent_tasks_fill_the_four_empty_slots() {
        let mut engine = HaloEngine::new(300_000);

        for value in 1..=4 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }

        let snapshot = engine.snapshot();
        let bound_keys: Vec<_> = snapshot
            .slots
            .iter()
            .map(|slot| slot.task_key.clone())
            .collect();

        assert_eq!(
            bound_keys,
            vec![Some(key(1)), Some(key(2)), Some(key(3)), Some(key(4))]
        );
        assert!(snapshot.queue.is_empty());
    }

    #[test]
    fn fifth_task_is_queued_without_overwriting_a_locked_slot() {
        let mut engine = HaloEngine::new(300_000);
        for value in 1..=4 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }
        engine.toggle_lock(0).expect("occupied slot can be locked");

        engine.apply_signal(signal(5, TaskStatus::Running, 500));

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.slots[0].task_key, Some(key(1)));
        assert!(snapshot.slots[0].locked);
        assert_eq!(snapshot.queue.len(), 1);
        assert_eq!(snapshot.queue[0].task_key, key(5));
        assert_eq!(snapshot.queue[0].status, TaskStatus::Queued);
    }

    #[test]
    fn manual_binding_never_places_the_same_task_in_two_slots() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::Running, 100));

        engine
            .manual_bind(&key(1), 2, true)
            .expect("known task can be manually bound");

        let snapshot = engine.snapshot();
        assert_eq!(
            snapshot
                .slots
                .iter()
                .filter(|slot| slot.task_key.as_ref() == Some(&key(1)))
                .count(),
            1
        );
        assert_eq!(snapshot.slots[2].task_key, Some(key(1)));
        assert_eq!(snapshot.slots[2].binding_mode, BindingMode::Manual);
        assert!(snapshot.slots[2].locked);
    }

    #[test]
    fn manual_move_backfills_the_vacated_slot_from_the_queue() {
        let mut engine = HaloEngine::new(300_000);
        for value in 1..=5 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }

        engine
            .manual_bind(&key(1), 2, false)
            .expect("known task can be moved");

        let snapshot = engine.snapshot();
        assert!(snapshot.slots.iter().all(|slot| slot.task_key.is_some()));
        assert_eq!(snapshot.queue.len(), 1);
        assert_eq!(snapshot.queue[0].task_key, key(3));
        assert!(snapshot
            .slots
            .iter()
            .any(|slot| slot.task_key.as_ref() == Some(&key(5))));
    }

    #[test]
    fn automatic_assignment_never_overwrites_a_locked_slot() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::Running, 100));
        engine
            .manual_bind(&key(1), 0, true)
            .expect("known task can be locked");

        for value in 2..=5 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.slots[0].task_key, Some(key(1)));
        assert_eq!(snapshot.slots[0].binding_mode, BindingMode::Manual);
        assert!(snapshot.slots[0].locked);
        assert_eq!(snapshot.queue[0].task_key, key(5));
    }

    #[test]
    fn swapping_slots_preserves_each_complete_slot_payload() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::Running, 100));
        engine.apply_signal(signal(2, TaskStatus::Waiting, 200));
        engine
            .manual_bind(&key(1), 0, true)
            .expect("known task can be manually bound");

        let before = engine.snapshot();
        let left_payload = before.slots[0].clone();
        let right_payload = before.slots[1].clone();

        engine.swap_slots(0, 1).expect("valid slots can be swapped");

        let after = engine.snapshot();
        assert_eq!(after.slots[0].task_key, right_payload.task_key);
        assert_eq!(after.slots[0].status, right_payload.status);
        assert_eq!(after.slots[0].binding_mode, right_payload.binding_mode);
        assert_eq!(after.slots[0].locked, right_payload.locked);
        assert_eq!(after.slots[1].task_key, left_payload.task_key);
        assert_eq!(after.slots[1].status, left_payload.status);
        assert_eq!(after.slots[1].binding_mode, left_payload.binding_mode);
        assert_eq!(after.slots[1].locked, left_payload.locked);
        assert_eq!(after.slots[0].index, 0);
        assert_eq!(after.slots[1].index, 1);
    }

    #[test]
    fn swapping_tasks_keeps_each_effect_on_its_physical_ring() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::Running, 100));
        engine.apply_signal(TaskSignal {
            task_key: key(2),
            state: NormalizedState {
                status: TaskStatus::Failed,
                source: SignalSource::Simulator,
                confidence: Confidence::Simulated,
            },
            received_at_ms: 200,
        });
        engine
            .manual_bind(&key(1), 0, true)
            .expect("known task can be manually bound");
        engine
            .update_effect(
                0,
                EffectProfile::new(40, 75, Direction::Clockwise, 25)
                    .expect("test effect must be valid"),
            )
            .expect("valid slot can be updated");
        engine
            .update_effect(
                1,
                EffectProfile::new(90, 250, Direction::CounterClockwise, 80)
                    .expect("test effect must be valid"),
            )
            .expect("valid slot can be updated");
        let before = engine.snapshot();

        engine.swap_slots(0, 1).expect("valid slots can be swapped");

        let after = engine.snapshot();
        assert_eq!(after.slots[0].index, 0);
        assert_eq!(after.slots[0].task_key, before.slots[1].task_key);
        assert_eq!(after.slots[0].status, before.slots[1].status);
        assert_eq!(after.slots[0].source, before.slots[1].source);
        assert_eq!(after.slots[0].confidence, before.slots[1].confidence);
        assert_eq!(after.slots[0].binding_mode, before.slots[1].binding_mode);
        assert_eq!(after.slots[0].locked, before.slots[1].locked);
        assert_eq!(after.slots[0].effect, before.slots[0].effect);

        assert_eq!(after.slots[1].index, 1);
        assert_eq!(after.slots[1].task_key, before.slots[0].task_key);
        assert_eq!(after.slots[1].status, before.slots[0].status);
        assert_eq!(after.slots[1].source, before.slots[0].source);
        assert_eq!(after.slots[1].confidence, before.slots[0].confidence);
        assert_eq!(after.slots[1].binding_mode, before.slots[0].binding_mode);
        assert_eq!(after.slots[1].locked, before.slots[0].locked);
        assert_eq!(after.slots[1].effect, before.slots[1].effect);
    }

    #[test]
    fn automatic_round_completed_slot_releases_after_hold_duration() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::RoundCompleted, 1_000));

        engine.tick(300_999);
        assert_eq!(engine.snapshot().slots[0].task_key, Some(key(1)));

        engine.tick(301_000);
        assert_eq!(engine.snapshot().slots[0].task_key, None);
    }

    #[test]
    fn locked_round_completed_slot_does_not_release() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::RoundCompleted, 1_000));
        engine.toggle_lock(0).expect("occupied slot can be locked");

        engine.tick(301_000);

        assert_eq!(engine.snapshot().slots[0].task_key, Some(key(1)));
        assert!(engine.snapshot().slots[0].locked);
    }

    #[test]
    fn repeated_signal_for_same_key_refreshes_without_duplication() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::Running, 100));
        engine.apply_signal(signal(1, TaskStatus::Waiting, 900));

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.slots[0].task_key, Some(key(1)));
        assert_eq!(snapshot.slots[0].status, TaskStatus::Waiting);
        assert_eq!(snapshot.tasks[0].last_active_at_ms, 900);
        assert!(snapshot.queue.is_empty());
    }

    #[test]
    fn older_signal_is_completely_ignored_for_bound_and_queued_tasks() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::Waiting, 1_000));
        for value in 2..=4 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }
        engine.apply_signal(signal(5, TaskStatus::Waiting, 1_100));
        let before = engine.snapshot();

        engine.apply_signal(signal(1, TaskStatus::Running, 900));
        engine.apply_signal(signal(5, TaskStatus::Running, 1_000));

        assert_eq!(engine.snapshot(), before);
    }

    #[test]
    fn queue_is_sorted_by_recent_activity_and_refresh_reorders_it() {
        let mut engine = HaloEngine::new(300_000);
        for value in 1..=4 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }
        engine.apply_signal(signal(5, TaskStatus::Running, 500));
        engine.apply_signal(signal(6, TaskStatus::Waiting, 600));

        assert_eq!(
            engine
                .snapshot()
                .queue
                .iter()
                .map(|task| task.task_key.clone())
                .collect::<Vec<_>>(),
            vec![key(6), key(5)]
        );

        engine.apply_signal(signal(5, TaskStatus::Waiting, 700));

        let queue = engine.snapshot().queue;
        assert_eq!(queue[0].task_key, key(5));
        assert_eq!(queue[0].last_active_at_ms, 700);
        assert_eq!(queue[1].task_key, key(6));
    }

    #[test]
    fn releasing_completed_slot_immediately_backfills_from_queue_head() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::RoundCompleted, 100));
        for value in 2..=4 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }
        engine.apply_signal(signal(5, TaskStatus::Running, 500));
        engine.apply_signal(signal(6, TaskStatus::Waiting, 600));

        engine.tick(300_100);

        let snapshot = engine.snapshot();
        assert_eq!(snapshot.slots[0].task_key, Some(key(6)));
        assert_eq!(snapshot.slots[0].status, TaskStatus::Waiting);
        assert_eq!(snapshot.queue.len(), 1);
        assert_eq!(snapshot.queue[0].task_key, key(5));
    }

    #[test]
    fn manually_binding_queued_task_keeps_it_globally_unique() {
        let mut engine = HaloEngine::new(300_000);
        for value in 1..=5 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }

        engine
            .manual_bind(&key(5), 1, true)
            .expect("queued task can be manually bound");

        let snapshot = engine.snapshot();
        assert_eq!(
            snapshot
                .slots
                .iter()
                .filter(|slot| slot.task_key.as_ref() == Some(&key(5)))
                .count(),
            1
        );
        assert_eq!(snapshot.slots[1].task_key, Some(key(5)));
        assert_eq!(snapshot.slots[1].binding_mode, BindingMode::Manual);
        assert!(snapshot.slots[1].locked);
        assert!(snapshot.queue.iter().all(|task| task.task_key != key(5)));
    }

    #[test]
    fn invalid_slot_and_unknown_task_return_structured_errors() {
        let mut engine = HaloEngine::new(300_000);

        assert_eq!(
            engine.toggle_lock(4),
            Err(EngineError::SlotOutOfBounds { slot: 4 })
        );
        assert_eq!(
            engine.manual_bind(&key(9), 0, false),
            Err(EngineError::TaskNotFound { task_key: key(9) })
        );
        assert_eq!(
            engine.toggle_lock(0),
            Err(EngineError::EmptySlot { slot: 0 })
        );

        assert_eq!(
            serde_json::to_value(EngineError::TaskNotFound { task_key: key(9) })
                .expect("engine error must serialize"),
            serde_json::json!({
                "code": "taskNotFound",
                "taskKey": "0000000000000009"
            })
        );
    }

    #[test]
    fn revision_is_monotonic_only_for_accepted_or_actual_state_changes() {
        let mut engine = HaloEngine::new(300_000);
        assert_eq!(engine.snapshot().revision, 0);
        assert_eq!(engine.snapshot().revision, 0, "reads never mutate revision");

        engine.apply_signal(signal(1, TaskStatus::Running, 100));
        assert_eq!(engine.snapshot().revision, 1);
        engine.apply_signal(signal(1, TaskStatus::Waiting, 99));
        assert_eq!(engine.snapshot().revision, 1, "old signals are ignored");
        engine.apply_signal(signal(1, TaskStatus::Waiting, 100));
        assert_eq!(
            engine.snapshot().revision,
            2,
            "an equal-timestamp accepted signal advances once"
        );

        engine
            .set_global_brightness(100)
            .expect("same valid brightness is a no-op");
        engine
            .update_effect(0, EffectProfile::default())
            .expect("same valid profile is a no-op");
        engine.swap_slots(0, 0).expect("same slot swap is a no-op");
        assert_eq!(engine.snapshot().revision, 2);

        engine
            .manual_bind(&key(1), 0, false)
            .expect("auto binding can become manual");
        assert_eq!(engine.snapshot().revision, 3);
        engine
            .manual_bind(&key(1), 0, false)
            .expect("identical manual binding is a no-op");
        assert_eq!(engine.snapshot().revision, 3);

        engine.toggle_lock(0).expect("lock changes state");
        assert_eq!(engine.snapshot().revision, 4);
        engine
            .set_global_brightness(50)
            .expect("brightness changes state");
        assert_eq!(engine.snapshot().revision, 5);
        engine
            .update_effect(
                0,
                EffectProfile::new(50, 200, Direction::CounterClockwise, 50)
                    .expect("test profile is valid"),
            )
            .expect("effect changes state");
        assert_eq!(engine.snapshot().revision, 6);
        engine.apply_signal(signal(2, TaskStatus::Running, 200));
        assert_eq!(engine.snapshot().revision, 7);
        engine.swap_slots(0, 1).expect("different assignments swap");
        assert_eq!(engine.snapshot().revision, 8);

        assert!(engine.set_global_brightness(101).is_err());
        assert!(engine.toggle_lock(4).is_err());
        assert_eq!(
            engine.snapshot().revision,
            8,
            "failed changes never advance"
        );
    }

    #[test]
    fn tick_advances_revision_once_only_when_a_slot_is_released() {
        let mut engine = HaloEngine::new(300_000);
        engine.apply_signal(signal(1, TaskStatus::RoundCompleted, 1_000));
        let accepted = engine.snapshot().revision;

        engine.tick(300_999);
        assert_eq!(engine.snapshot().revision, accepted);
        engine.tick(301_000);
        assert_eq!(engine.snapshot().revision, accepted + 1);
        engine.tick(301_001);
        assert_eq!(engine.snapshot().revision, accepted + 1);
    }

    #[test]
    fn identical_bound_and_queued_signal_replays_do_not_advance_revision() {
        let mut engine = HaloEngine::new(300_000);
        let bound = signal(1, TaskStatus::Running, 100);
        engine.apply_signal(bound.clone());
        let bound_revision = engine.snapshot().revision;

        engine.apply_signal(bound);
        assert_eq!(engine.snapshot().revision, bound_revision);

        for value in 2..=4 {
            engine.apply_signal(signal(value, TaskStatus::Running, value * 100));
        }
        let queued = signal(5, TaskStatus::Waiting, 500);
        engine.apply_signal(queued.clone());
        let queued_revision = engine.snapshot().revision;

        engine.apply_signal(queued);
        assert_eq!(engine.snapshot().revision, queued_revision);

        engine.apply_signal(signal(5, TaskStatus::Running, 500));
        assert_eq!(
            engine.snapshot().revision,
            queued_revision + 1,
            "same timestamp with a different state is an actual change"
        );
    }
}
