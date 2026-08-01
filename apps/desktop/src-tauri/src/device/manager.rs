use crate::device::presentation::{DeviceSnapshot, DeviceUpdate};
use crate::device::protocol::{
    self, Decoder, DecoderMode, Frame, MessageType, ProtocolError, MAX_PAYLOAD,
};
use crate::device::transport::{DeviceTransport, TransportError, TransportKind};
use crate::domain::model::{HaloSnapshot, PresentationIntent};
use serde::Serialize;
use std::collections::VecDeque;

const ACK_TIMEOUT_MS: u64 = 250;
const MAX_RETRIES: u8 = 2;
const HEARTBEAT_INTERVAL_MS: u64 = 1_000;
const MAX_READS_PER_STEP: usize = 256;
const FEATURE_AMOLED: u16 = 0x0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceConnectionState {
    Virtual,
    Connecting,
    Online,
    Incompatible,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub revision: u64,
    pub state: DeviceConnectionState,
    pub transport: TransportKind,
    pub message: Option<String>,
    pub firmware_version: Option<String>,
    pub retry_count: u32,
}

pub struct StepResult {
    pub status_changed: bool,
    pub intents: Vec<PresentationIntent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceMetrics {
    pub retry_count: u32,
    pub reconnect_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagerPhase {
    Disconnected,
    Handshaking,
    Ready,
    Incompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedResponse {
    Capabilities,
    Ack(MessageType),
}

struct PendingRequest {
    bytes: Vec<u8>,
    sequence: u16,
    sent_at_ms: u64,
    retries: u8,
    expected: ExpectedResponse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingRequestToken {
    sequence: u16,
    retries: u8,
    expected: ExpectedResponse,
}

impl PendingRequest {
    fn token(&self) -> PendingRequestToken {
        PendingRequestToken {
            sequence: self.sequence,
            retries: self.retries,
            expected: self.expected,
        }
    }
}

struct OutboundWrite {
    message_type: MessageType,
    payload: Vec<u8>,
}

pub struct DeviceManager<T: DeviceTransport> {
    transport: T,
    decoder: Decoder,
    status: DeviceStatus,
    metrics: DeviceMetrics,
    phase: ManagerPhase,
    pending: Option<PendingRequest>,
    queued_writes: VecDeque<OutboundWrite>,
    target_snapshot: Option<DeviceSnapshot>,
    applied_snapshot: Option<DeviceSnapshot>,
    next_sequence: u16,
    last_heartbeat_ms: Option<u64>,
    last_knob_sequence: Option<u16>,
    ever_connected: bool,
}

impl<T: DeviceTransport> DeviceManager<T> {
    pub fn new(transport: T) -> Self {
        let transport_kind = transport.kind();
        Self {
            transport,
            decoder: Decoder::new(DecoderMode::StrictV01),
            status: DeviceStatus {
                revision: 0,
                state: DeviceConnectionState::Connecting,
                transport: transport_kind,
                message: None,
                firmware_version: None,
                retry_count: 0,
            },
            metrics: DeviceMetrics::default(),
            phase: ManagerPhase::Disconnected,
            pending: None,
            queued_writes: VecDeque::new(),
            target_snapshot: None,
            applied_snapshot: None,
            next_sequence: 0,
            last_heartbeat_ms: None,
            last_knob_sequence: None,
            ever_connected: false,
        }
    }

    pub fn status(&self) -> &DeviceStatus {
        &self.status
    }

    pub fn metrics(&self) -> &DeviceMetrics {
        &self.metrics
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn step(&mut self, now_ms: u64, snapshot: &HaloSnapshot) -> StepResult {
        let status_before = self.status.clone();
        let mut intents = Vec::new();

        if self.phase != ManagerPhase::Incompatible {
            if !self.transport.is_connected() {
                self.reset_connection_state();
            }
            if self.phase == ManagerPhase::Disconnected {
                self.connect(now_ms);
            }
            self.pump_reads(now_ms, snapshot, &mut intents);
            self.retry_if_timed_out(now_ms);

            if self.phase == ManagerPhase::Ready && self.pending.is_none() {
                self.queue_changed_snapshot(now_ms, snapshot);
                self.pump_reads(now_ms, snapshot, &mut intents);
            }

            if self.phase == ManagerPhase::Ready {
                self.send_heartbeat_if_due(now_ms);
            }
        }

        let status_changed = self.status_fields_changed(&status_before);
        if status_changed {
            self.status.revision = self.status.revision.wrapping_add(1);
        }
        StepResult {
            status_changed,
            intents,
        }
    }

    fn connect(&mut self, now_ms: u64) {
        self.status.state = DeviceConnectionState::Connecting;
        self.status.message = None;
        self.status.firmware_version = None;

        let endpoint = match self.transport.discover() {
            Ok(endpoints) => match endpoints.into_iter().next() {
                Some(endpoint) => endpoint,
                None => {
                    self.connection_error("Device endpoint was not found");
                    return;
                }
            },
            Err(_) => {
                self.connection_error("Device discovery failed");
                return;
            }
        };
        if self.transport.connect(&endpoint).is_err() {
            self.connection_error("Device connection failed");
            return;
        }

        if self.ever_connected {
            self.metrics.reconnect_count = self.metrics.reconnect_count.saturating_add(1);
        }
        self.ever_connected = true;
        self.decoder = Decoder::new(DecoderMode::StrictV01);
        self.pending = None;
        self.queued_writes.clear();
        self.target_snapshot = None;
        self.applied_snapshot = None;
        self.last_heartbeat_ms = None;
        self.last_knob_sequence = None;
        self.phase = ManagerPhase::Handshaking;
        self.begin_request(
            MessageType::Hello,
            vec![0],
            ExpectedResponse::Capabilities,
            now_ms,
        );
    }

    fn pump_reads(
        &mut self,
        now_ms: u64,
        snapshot: &HaloSnapshot,
        intents: &mut Vec<PresentationIntent>,
    ) {
        for _ in 0..MAX_READS_PER_STEP {
            let bytes = match self.transport.read() {
                Ok(bytes) if bytes.is_empty() => break,
                Ok(bytes) => bytes,
                Err(TransportError::Timeout) => break,
                Err(TransportError::Protocol(ProtocolError::UnsupportedVersion { .. })) => {
                    self.mark_incompatible("Protocol major is incompatible");
                    break;
                }
                Err(_) => {
                    self.force_reconnect("Device read failed");
                    break;
                }
            };

            let batch_request = self.pending.as_ref().map(PendingRequest::token);
            let decoded = self.decoder.push(&bytes);
            for result in decoded {
                match result {
                    Ok(frame) => self.handle_frame(frame, batch_request, now_ms, snapshot, intents),
                    Err(ProtocolError::UnsupportedVersion { .. }) => {
                        self.mark_incompatible("Protocol major is incompatible");
                    }
                    Err(_) => {}
                }
                if self.phase == ManagerPhase::Incompatible {
                    return;
                }
            }
        }
    }

    fn handle_frame(
        &mut self,
        frame: Frame,
        batch_request: Option<PendingRequestToken>,
        now_ms: u64,
        snapshot: &HaloSnapshot,
        intents: &mut Vec<PresentationIntent>,
    ) {
        if frame.message_type == MessageType::KnobEvent {
            if let Some(intent) = self.decode_knob_event(&frame) {
                intents.push(intent);
            }
            return;
        }

        let (Some(batch_request), Some(pending)) = (batch_request, self.pending.as_ref()) else {
            return;
        };
        if pending.token() != batch_request || frame.sequence != batch_request.sequence {
            return;
        }

        match pending.expected {
            ExpectedResponse::Capabilities if frame.message_type == MessageType::Capabilities => {
                let firmware_version = DeviceSnapshot::from_halo(snapshot)
                    .encode_payload()
                    .ok()
                    .and_then(|payload| parse_capabilities(&frame.payload, payload.len()));
                if let Some(firmware_version) = firmware_version {
                    self.pending = None;
                    self.status.firmware_version = Some(firmware_version);
                    self.queue_full_snapshot(now_ms, snapshot);
                } else {
                    self.mark_incompatible("Device capabilities are incompatible");
                }
            }
            ExpectedResponse::Ack(expected_type)
                if frame.message_type == MessageType::Ack
                    && frame.payload == [expected_type as u8] =>
            {
                self.pending = None;
                self.status.message = None;
                self.begin_next_write(now_ms);
            }
            ExpectedResponse::Ack(expected_type)
                if frame.message_type == MessageType::Nack
                    && valid_nack_payload(&frame.payload, expected_type) =>
            {
                self.retry_pending(now_ms, "Device rejected state update");
            }
            _ => {}
        }
    }

    fn queue_full_snapshot(&mut self, now_ms: u64, snapshot: &HaloSnapshot) {
        let projected = DeviceSnapshot::from_halo(snapshot);
        let payload = match projected.encode_payload() {
            Ok(payload) => payload,
            Err(_) => {
                self.force_reconnect("Device snapshot was invalid");
                return;
            }
        };
        self.target_snapshot = Some(projected);
        self.queued_writes.push_back(OutboundWrite {
            message_type: MessageType::FullSnapshot,
            payload,
        });
        self.begin_next_write(now_ms);
    }

    fn queue_changed_snapshot(&mut self, now_ms: u64, snapshot: &HaloSnapshot) {
        let projected = DeviceSnapshot::from_halo(snapshot);
        let Some(previous) = self.applied_snapshot.as_ref() else {
            self.queue_full_snapshot(now_ms, snapshot);
            return;
        };
        if projected.revision == previous.revision {
            return;
        }

        let updates = projected.diff(previous);
        for update in updates {
            let message_type = update_message_type(&update);
            let payload = match update.encode_payload() {
                Ok(payload) => payload,
                Err(_) => {
                    self.force_reconnect("Device update was invalid");
                    return;
                }
            };
            self.queued_writes.push_back(OutboundWrite {
                message_type,
                payload,
            });
        }
        self.target_snapshot = Some(projected);
        self.begin_next_write(now_ms);
    }

    fn begin_next_write(&mut self, now_ms: u64) {
        if self.pending.is_some() {
            return;
        }
        if let Some(write) = self.queued_writes.pop_front() {
            self.begin_request(
                write.message_type,
                write.payload,
                ExpectedResponse::Ack(write.message_type),
                now_ms,
            );
            return;
        }

        if let Some(snapshot) = self.target_snapshot.take() {
            self.applied_snapshot = Some(snapshot);
        }
        if self.phase == ManagerPhase::Handshaking {
            self.phase = ManagerPhase::Ready;
            self.status.state = match self.status.transport {
                TransportKind::Simulator => DeviceConnectionState::Virtual,
                TransportKind::Serial => DeviceConnectionState::Online,
            };
            self.status.message = None;
            self.last_heartbeat_ms = Some(now_ms);
        }
    }

    fn begin_request(
        &mut self,
        message_type: MessageType,
        payload: Vec<u8>,
        expected: ExpectedResponse,
        now_ms: u64,
    ) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let frame = Frame::new(message_type, sequence, payload);
        let bytes = match protocol::encode(&frame) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.force_reconnect("Device frame could not be encoded");
                return;
            }
        };
        if self.transport.write(&bytes).is_err() {
            self.force_reconnect("Device write failed");
            return;
        }
        self.pending = Some(PendingRequest {
            bytes,
            sequence,
            sent_at_ms: now_ms,
            retries: 0,
            expected,
        });
    }

    fn retry_if_timed_out(&mut self, now_ms: u64) {
        let timed_out = self
            .pending
            .as_ref()
            .is_some_and(|pending| now_ms.saturating_sub(pending.sent_at_ms) >= ACK_TIMEOUT_MS);
        if timed_out {
            self.retry_pending(now_ms, "Device response timed out");
        }
    }

    fn retry_pending(&mut self, now_ms: u64, message: &'static str) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        if pending.retries >= MAX_RETRIES {
            self.force_reconnect(message);
            return;
        }

        if self.transport.write(&pending.bytes).is_err() {
            self.force_reconnect("Device retry failed");
            return;
        }
        pending.retries += 1;
        pending.sent_at_ms = now_ms;
        self.metrics.retry_count = self.metrics.retry_count.saturating_add(1);
        self.status.retry_count = self.metrics.retry_count;
        self.status.message = Some(message.to_owned());
    }

    fn send_heartbeat_if_due(&mut self, now_ms: u64) {
        let due = self
            .last_heartbeat_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= HEARTBEAT_INTERVAL_MS);
        if !due {
            return;
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let frame = Frame::new(MessageType::Heartbeat, sequence, Vec::new());
        match protocol::encode(&frame).and_then(|bytes| {
            self.transport
                .write(&bytes)
                .map_err(|_| ProtocolError::PayloadTooLarge { actual: 0 })
        }) {
            Ok(()) => self.last_heartbeat_ms = Some(now_ms),
            Err(_) => self.force_reconnect("Device heartbeat failed"),
        }
    }

    fn decode_knob_event(&mut self, frame: &Frame) -> Option<PresentationIntent> {
        if !sequence_is_newer(self.last_knob_sequence, frame.sequence) {
            return None;
        }
        let intent = match frame.payload.as_slice() {
            [0x01, delta] if *delta != 0 => PresentationIntent::Rotate(*delta as i8),
            [0x02, 0] => PresentationIntent::ShortPress,
            [0x03, 0] => PresentationIntent::LongPress,
            _ => return None,
        };
        self.last_knob_sequence = Some(frame.sequence);
        Some(intent)
    }

    fn mark_incompatible(&mut self, message: &'static str) {
        let _ = self.transport.disconnect();
        self.reset_connection_state();
        self.phase = ManagerPhase::Incompatible;
        self.status.state = DeviceConnectionState::Incompatible;
        self.status.message = Some(message.to_owned());
    }

    fn force_reconnect(&mut self, message: &'static str) {
        let _ = self.transport.disconnect();
        self.reset_connection_state();
        self.status.state = DeviceConnectionState::Error;
        self.status.message = Some(message.to_owned());
    }

    fn connection_error(&mut self, message: &'static str) {
        self.reset_connection_state();
        self.status.state = DeviceConnectionState::Error;
        self.status.message = Some(message.to_owned());
    }

    fn reset_connection_state(&mut self) {
        self.decoder = Decoder::new(DecoderMode::StrictV01);
        self.pending = None;
        self.queued_writes.clear();
        self.target_snapshot = None;
        self.applied_snapshot = None;
        self.last_heartbeat_ms = None;
        self.last_knob_sequence = None;
        self.phase = ManagerPhase::Disconnected;
    }

    fn status_fields_changed(&self, before: &DeviceStatus) -> bool {
        self.status.state != before.state
            || self.status.transport != before.transport
            || self.status.message != before.message
            || self.status.firmware_version != before.firmware_version
            || self.status.retry_count != before.retry_count
    }
}

fn parse_capabilities(payload: &[u8], required_payload: usize) -> Option<String> {
    if payload.len() != 9 || payload[4] != 4 {
        return None;
    }
    let feature_flags = u16::from_le_bytes([payload[5], payload[6]]);
    let max_payload = u16::from_le_bytes([payload[7], payload[8]]);
    if feature_flags & FEATURE_AMOLED == 0
        || usize::from(max_payload) < required_payload
        || usize::from(max_payload) > MAX_PAYLOAD
    {
        return None;
    }
    Some(format!("{}.{}.{}", payload[1], payload[2], payload[3]))
}

fn valid_nack_payload(payload: &[u8], expected_type: MessageType) -> bool {
    match payload {
        [message_type, reason] => {
            *message_type == expected_type as u8 && (0x01..=0x05).contains(reason)
        }
        _ => false,
    }
}

fn update_message_type(update: &DeviceUpdate) -> MessageType {
    match update {
        DeviceUpdate::Ring(_) => MessageType::RingUpdate,
        DeviceUpdate::Display { .. } => MessageType::DisplayMode,
        DeviceUpdate::Brightness(_) => MessageType::Brightness,
    }
}

fn sequence_is_newer(previous: Option<u16>, candidate: u16) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance < 0x8000
}

#[cfg(test)]
mod tests {
    use super::{DeviceConnectionState, DeviceManager};
    use crate::device::presentation::DeviceSnapshot;
    use crate::device::protocol::{self, Frame, MessageType};
    use crate::device::simulator::{Fault, KnobEvent, SimulatedTransport};
    use crate::device::transport::DeviceTransport;
    use crate::domain::effects::EffectProfile;
    use crate::domain::model::{
        BindingMode, DeviceMode, DisplayMode, HaloSnapshot, PresentationIntent, RingSlot,
        TaskStatus,
    };

    fn fixture_halo_snapshot() -> HaloSnapshot {
        HaloSnapshot {
            revision: 1,
            device_mode: DeviceMode::Virtual,
            global_brightness: 73,
            display_mode: DisplayMode::Detail,
            selected_slot: Some(2),
            slots: (0..4)
                .map(|index| RingSlot {
                    index,
                    task_key: None,
                    status: TaskStatus::Running,
                    source: None,
                    confidence: None,
                    binding_mode: BindingMode::Auto,
                    locked: false,
                    effect: EffectProfile::default(),
                })
                .collect(),
            tasks: Vec::new(),
            queue: Vec::new(),
        }
    }

    fn online_manager() -> DeviceManager<SimulatedTransport> {
        let mut manager = DeviceManager::new(SimulatedTransport::default());
        manager.step(0, &fixture_halo_snapshot());
        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
        manager
    }

    #[test]
    fn manager_handshakes_then_sends_an_authoritative_full_snapshot() {
        let transport = SimulatedTransport::default();
        let mut manager = DeviceManager::new(transport);

        manager.step(0, &fixture_halo_snapshot());
        manager.step(1, &fixture_halo_snapshot());

        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
        assert_eq!(
            manager.transport().applied_snapshot(),
            Some(&DeviceSnapshot::from_halo(&fixture_halo_snapshot()))
        );
    }

    #[test]
    fn manager_retries_twice_then_reconnects_with_a_full_snapshot() {
        let mut transport = SimulatedTransport::default();
        transport.script(Fault::TimeoutOnce);
        transport.script(Fault::TimeoutOnce);
        transport.script(Fault::TimeoutOnce);
        let mut manager = DeviceManager::new(transport);

        for now in [0, 250, 500, 750, 751] {
            manager.step(now, &fixture_halo_snapshot());
        }

        assert_eq!(manager.metrics().retry_count, 2);
        assert!(manager.metrics().reconnect_count >= 1);
        assert!(manager.transport().full_snapshot_count() >= 1);
    }

    #[test]
    fn incompatible_major_never_receives_state_writes() {
        let mut transport = SimulatedTransport::default();
        transport.set_protocol_major(2);
        let mut manager = DeviceManager::new(transport);
        manager.step(0, &fixture_halo_snapshot());

        assert_eq!(manager.status().state, DeviceConnectionState::Incompatible);
        assert_eq!(manager.transport().state_write_count(), 0);
    }

    #[test]
    fn incompatible_capabilities_never_receive_state_writes() {
        let snapshot = fixture_halo_snapshot();
        let required_payload = DeviceSnapshot::from_halo(&snapshot)
            .encode_payload()
            .unwrap()
            .len() as u16;

        let mut undersized = SimulatedTransport::default();
        undersized.set_max_payload(required_payload - 1);
        let mut undersized_manager = DeviceManager::new(undersized);
        undersized_manager.step(0, &snapshot);
        assert_eq!(
            undersized_manager.status().state,
            DeviceConnectionState::Incompatible
        );
        assert_eq!(undersized_manager.transport().state_write_count(), 0);

        let mut missing_amoled = SimulatedTransport::default();
        missing_amoled.set_feature_flags(0x0002);
        let mut missing_amoled_manager = DeviceManager::new(missing_amoled);
        missing_amoled_manager.step(0, &snapshot);
        assert_eq!(
            missing_amoled_manager.status().state,
            DeviceConnectionState::Incompatible
        );
        assert_eq!(missing_amoled_manager.transport().state_write_count(), 0);
    }

    #[test]
    fn manager_rejects_impossible_fixed_length_and_processes_nested_knob_event() {
        let mut manager = online_manager();
        let mut bytes = vec![0x43, 0x48, 0x01, 0x02, 0x09, 0x00, 0x00, 0x02];
        bytes.extend(
            protocol::encode(&Frame::new(MessageType::KnobEvent, 77, vec![0x02, 0])).unwrap(),
        );
        manager.transport_mut().queue_raw_response(bytes);

        assert_eq!(
            manager.step(10, &fixture_halo_snapshot()).intents,
            vec![PresentationIntent::ShortPress]
        );
    }

    #[test]
    fn required_capabilities_allow_unknown_feature_bits() {
        let snapshot = fixture_halo_snapshot();
        let required_payload = DeviceSnapshot::from_halo(&snapshot)
            .encode_payload()
            .unwrap()
            .len() as u16;
        let mut transport = SimulatedTransport::default();
        transport.set_feature_flags(0x8001);
        transport.set_max_payload(required_payload);
        let mut manager = DeviceManager::new(transport);

        manager.step(0, &snapshot);

        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
        assert_eq!(manager.transport().state_write_count(), 1);
    }

    #[test]
    fn old_knob_sequences_are_ignored_and_new_events_become_intents() {
        let mut manager = online_manager();
        manager
            .transport_mut()
            .inject_knob_with_sequence(9, KnobEvent::ShortPress)
            .unwrap();
        manager
            .transport_mut()
            .inject_knob_with_sequence(9, KnobEvent::Rotate(1))
            .unwrap();
        manager
            .transport_mut()
            .inject_knob_with_sequence(10, KnobEvent::Rotate(-1))
            .unwrap();

        assert_eq!(
            manager.step(100, &fixture_halo_snapshot()).intents,
            vec![
                PresentationIntent::ShortPress,
                PresentationIntent::Rotate(-1)
            ]
        );
    }

    #[test]
    fn heartbeat_is_sent_once_per_elapsed_interval() {
        let mut manager = online_manager();

        manager.step(999, &fixture_halo_snapshot());
        assert_eq!(manager.transport().heartbeat_count(), 0);
        manager.step(1_000, &fixture_halo_snapshot());
        assert_eq!(manager.transport().heartbeat_count(), 1);
        manager.step(1_999, &fixture_halo_snapshot());
        assert_eq!(manager.transport().heartbeat_count(), 1);
        manager.step(2_000, &fixture_halo_snapshot());
        assert_eq!(manager.transport().heartbeat_count(), 2);
    }

    #[test]
    fn heartbeat_continues_while_a_state_ack_is_pending() {
        let mut manager = online_manager();
        manager.transport_mut().script(Fault::TimeoutOnce);
        manager.transport_mut().script(Fault::TimeoutOnce);
        manager.transport_mut().script(Fault::TimeoutOnce);
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.global_brightness = 50;

        manager.step(900, &changed);
        manager.step(1_000, &changed);
        assert_eq!(manager.transport().heartbeat_count(), 1);

        manager.step(1_150, &changed);
        manager.step(1_151, &changed);
        manager.step(2_000, &changed);
        assert_eq!(manager.transport().heartbeat_count(), 2);
        assert_eq!(manager.metrics().retry_count, 2);
    }

    #[test]
    fn unchanged_revision_does_not_repeat_state_writes() {
        let mut manager = online_manager();
        let writes = manager.transport().state_write_count();
        let mut changed_without_revision = fixture_halo_snapshot();
        changed_without_revision.global_brightness = 10;

        manager.step(10, &changed_without_revision);

        assert_eq!(manager.transport().state_write_count(), writes);
    }

    #[test]
    fn changed_revision_sends_ordered_diffs_one_ack_at_a_time() {
        let mut manager = online_manager();
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.slots[0].status = TaskStatus::Waiting;
        changed.slots[3].status = TaskStatus::Failed;
        changed.display_mode = DisplayMode::Overview;
        changed.selected_slot = None;
        changed.global_brightness = 50;

        manager.step(10, &changed);

        assert_eq!(
            manager.transport().state_write_log(),
            &[
                (MessageType::FullSnapshot, 1),
                (MessageType::RingUpdate, 2),
                (MessageType::RingUpdate, 3),
                (MessageType::DisplayMode, 4),
                (MessageType::Brightness, 5),
            ]
        );
        assert_eq!(manager.transport().max_pending_response_count(), 1);
        assert_eq!(
            manager.transport().applied_snapshot().unwrap().rings[0].status,
            crate::device::presentation::DeviceTaskStatus::Waiting
        );
        assert_eq!(
            manager.transport().applied_snapshot().unwrap().rings[3].status,
            crate::device::presentation::DeviceTaskStatus::Failed
        );
    }

    #[test]
    fn future_ack_from_the_same_read_batch_cannot_ack_the_next_write() {
        let mut manager = online_manager();
        manager
            .transport_mut()
            .script(Fault::AckBatchWithFutureOnce(MessageType::Brightness));
        manager.transport_mut().script(Fault::TimeoutOnce);
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.slots[0].status = TaskStatus::Waiting;
        changed.global_brightness = 50;

        manager.step(10, &changed);

        assert_eq!(manager.applied_snapshot.as_ref().unwrap().revision, 1);
        assert_eq!(manager.metrics().retry_count, 0);
        assert_eq!(
            manager.transport().state_write_log(),
            &[
                (MessageType::FullSnapshot, 1),
                (MessageType::RingUpdate, 2),
                (MessageType::Brightness, 3),
            ]
        );

        manager.step(260, &changed);
        assert_eq!(manager.applied_snapshot.as_ref().unwrap().revision, 1);
        manager.step(261, &changed);
        assert_eq!(manager.applied_snapshot.as_ref().unwrap().revision, 2);
    }

    #[test]
    fn malformed_and_unknown_nacks_wait_for_timeout_before_retrying() {
        for fault in [Fault::MalformedNackOnce, Fault::UnknownNackReasonOnce(0xff)] {
            let mut manager = online_manager();
            manager.transport_mut().script(fault);
            let initial_writes = manager.transport().state_write_count();
            let mut changed = fixture_halo_snapshot();
            changed.revision = 2;
            changed.global_brightness = 50;

            manager.step(10, &changed);
            assert_eq!(manager.transport().state_write_count(), initial_writes + 1);
            assert_eq!(manager.metrics().retry_count, 0);

            manager.step(259, &changed);
            assert_eq!(manager.transport().state_write_count(), initial_writes + 1);
            manager.step(260, &changed);
            assert_eq!(manager.transport().state_write_count(), initial_writes + 2);
            assert_eq!(manager.metrics().retry_count, 1);
            manager.step(261, &changed);
            assert_eq!(
                manager
                    .transport()
                    .applied_snapshot()
                    .unwrap()
                    .global_brightness,
                50
            );
        }
    }

    #[test]
    fn nack_once_retries_and_applies_the_final_state() {
        let mut manager = online_manager();
        manager
            .transport_mut()
            .script(Fault::NackOnce(crate::device::simulator::NackReason::Busy));
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.global_brightness = 50;

        manager.step(10, &changed);

        assert_eq!(manager.metrics().retry_count, 1);
        assert_eq!(manager.transport().state_write_count(), 3);
        assert_eq!(
            manager
                .transport()
                .applied_snapshot()
                .unwrap()
                .global_brightness,
            50
        );
    }

    #[test]
    fn repeated_nacks_retry_twice_then_reconnect_with_the_final_state() {
        let mut manager = online_manager();
        for _ in 0..3 {
            manager
                .transport_mut()
                .script(Fault::NackOnce(crate::device::simulator::NackReason::Busy));
        }
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.global_brightness = 50;

        manager.step(10, &changed);
        assert_eq!(manager.metrics().retry_count, 2);
        manager.step(11, &changed);

        assert_eq!(manager.metrics().retry_count, 2);
        assert_eq!(manager.metrics().reconnect_count, 1);
        assert_eq!(manager.transport().full_snapshot_count(), 2);
        assert_eq!(
            manager
                .transport()
                .applied_snapshot()
                .unwrap()
                .global_brightness,
            50
        );
    }

    #[test]
    fn corrupt_crc_once_retries_after_timeout_and_applies_the_final_state() {
        let mut manager = online_manager();
        manager.transport_mut().script(Fault::CorruptCrcOnce);
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.global_brightness = 50;

        manager.step(10, &changed);
        assert_eq!(manager.metrics().retry_count, 0);
        manager.step(259, &changed);
        assert_eq!(manager.metrics().retry_count, 0);
        manager.step(260, &changed);
        assert_eq!(manager.metrics().retry_count, 1);
        manager.step(261, &changed);

        assert_eq!(manager.metrics().retry_count, 1);
        assert!(manager.metrics().retry_count <= 2);
        assert_eq!(
            manager
                .transport()
                .applied_snapshot()
                .unwrap()
                .global_brightness,
            50
        );
    }

    #[test]
    fn successful_ack_clears_the_transient_retry_message() {
        let mut manager = online_manager();
        manager.transport_mut().script(Fault::TimeoutOnce);
        let mut changed = fixture_halo_snapshot();
        changed.revision = 2;
        changed.global_brightness = 50;

        manager.step(10, &changed);
        manager.step(260, &changed);
        assert_eq!(
            manager.status().message.as_deref(),
            Some("Device response timed out")
        );

        manager.step(261, &changed);

        assert_eq!(manager.status().message, None);
        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
    }

    #[test]
    fn every_new_connection_sends_a_full_snapshot() {
        let mut manager = online_manager();
        assert_eq!(manager.transport().full_snapshot_count(), 1);
        manager.transport_mut().disconnect().unwrap();

        manager.step(10, &fixture_halo_snapshot());

        assert_eq!(manager.transport().full_snapshot_count(), 2);
        assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
    }

    #[test]
    fn knob_sequence_comparison_accepts_wrap_and_rejects_pre_wrap_stale_events() {
        let mut manager = online_manager();
        manager
            .transport_mut()
            .inject_knob_with_sequence(u16::MAX, KnobEvent::ShortPress)
            .unwrap();
        manager
            .transport_mut()
            .inject_knob_with_sequence(0, KnobEvent::LongPress)
            .unwrap();
        manager
            .transport_mut()
            .inject_knob_with_sequence(u16::MAX, KnobEvent::Rotate(1))
            .unwrap();

        assert_eq!(
            manager.step(100, &fixture_halo_snapshot()).intents,
            vec![
                PresentationIntent::ShortPress,
                PresentationIntent::LongPress
            ]
        );
    }

    #[test]
    fn serialized_status_is_camel_case_and_contains_no_identity() {
        let manager = online_manager();
        let json = serde_json::to_string(manager.status()).unwrap();

        assert!(json.contains("\"state\":\"virtual\""));
        assert!(json.contains("\"firmwareVersion\""));
        assert!(!json.contains("taskKey"));
        assert!(!json.contains("serialNumber"));
    }
}
