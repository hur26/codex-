use crate::device::presentation::{
    DeviceDirection, DeviceDisplayMode, DeviceRing, DeviceSnapshot, DeviceTaskStatus,
};
use crate::device::protocol::{
    self, Decoder, DecoderMode, Frame, MessageType, ProtocolError, PROTOCOL_MAJOR,
};
use crate::device::transport::{DeviceTransport, Endpoint, TransportError, TransportKind};
use std::collections::VecDeque;

const DEFAULT_FEATURE_FLAGS: u16 = 0x0003;
const DEFAULT_MAX_PAYLOAD: u16 = 512;
const RECENT_STATE_WRITE_LIMIT: usize = 64;
const RING_COUNT: usize = 4;
const RING_PAYLOAD_BYTES: usize = 8;
const FULL_SNAPSHOT_PREFIX_BYTES: usize = 12;
const FULL_SNAPSHOT_PAYLOAD_BYTES: usize =
    FULL_SNAPSHOT_PREFIX_BYTES + RING_COUNT * RING_PAYLOAD_BYTES;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NackReason {
    MalformedPayload = 0x01,
    UnsupportedMessage = 0x02,
    InvalidState = 0x03,
    Busy = 0x04,
    InternalError = 0x05,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnobEvent {
    Rotate(i8),
    ShortPress,
    LongPress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    TimeoutOnce,
    NackOnce(NackReason),
    MalformedNackOnce,
    UnknownNackReasonOnce(u8),
    AckBatchWithFutureOnce(MessageType),
    CorruptCrcOnce,
}

struct PendingResponse {
    bytes: Vec<u8>,
    fault: Option<ResponseFault>,
}

#[derive(Clone, Copy)]
enum ResponseFault {
    Timeout,
    CorruptCrc,
    AckBatchWithFuture(MessageType),
}

pub struct SimulatedTransport {
    connected: bool,
    decoder: Decoder,
    responses: VecDeque<PendingResponse>,
    faults: VecDeque<Fault>,
    protocol_major: u8,
    feature_flags: u16,
    max_payload: u16,
    next_knob_sequence: u16,
    applied_snapshot: Option<DeviceSnapshot>,
    state_write_log: Vec<(MessageType, u16)>,
    state_write_count: usize,
    full_snapshot_count: usize,
    heartbeat_count: usize,
    max_pending_response_count: usize,
}

impl Default for SimulatedTransport {
    fn default() -> Self {
        Self {
            connected: false,
            decoder: Decoder::new(DecoderMode::StrictV01),
            responses: VecDeque::new(),
            faults: VecDeque::new(),
            protocol_major: PROTOCOL_MAJOR,
            feature_flags: DEFAULT_FEATURE_FLAGS,
            max_payload: DEFAULT_MAX_PAYLOAD,
            next_knob_sequence: 0,
            applied_snapshot: None,
            state_write_log: Vec::new(),
            state_write_count: 0,
            full_snapshot_count: 0,
            heartbeat_count: 0,
            max_pending_response_count: 0,
        }
    }
}

impl SimulatedTransport {
    pub fn script(&mut self, fault: Fault) {
        self.faults.push_back(fault);
    }

    pub fn pending_fault_count(&self) -> usize {
        self.faults.len()
    }

    #[cfg(test)]
    pub(crate) fn queue_raw_response(&mut self, bytes: Vec<u8>) {
        self.responses
            .push_back(PendingResponse { bytes, fault: None });
    }

    pub fn set_protocol_major(&mut self, protocol_major: u8) {
        self.protocol_major = protocol_major;
    }

    pub fn set_feature_flags(&mut self, feature_flags: u16) {
        self.feature_flags = feature_flags;
    }

    pub fn set_max_payload(&mut self, max_payload: u16) {
        self.max_payload = max_payload;
    }

    pub fn full_snapshot_count(&self) -> usize {
        self.full_snapshot_count
    }

    pub fn state_write_count(&self) -> usize {
        self.state_write_count
    }

    pub fn state_write_log(&self) -> &[(MessageType, u16)] {
        &self.state_write_log
    }

    pub fn heartbeat_count(&self) -> usize {
        self.heartbeat_count
    }

    pub fn max_pending_response_count(&self) -> usize {
        self.max_pending_response_count
    }

    pub fn inject_knob(&mut self, event: KnobEvent) -> Result<(), TransportError> {
        let sequence = self.next_knob_sequence;
        self.inject_knob_with_sequence(sequence, event)
    }

    pub fn inject_knob_with_sequence(
        &mut self,
        sequence: u16,
        event: KnobEvent,
    ) -> Result<(), TransportError> {
        if event == KnobEvent::Rotate(0) {
            return Err(TransportError::InvalidKnobDelta);
        }
        self.next_knob_sequence = sequence.wrapping_add(1);
        let payload = match event {
            KnobEvent::Rotate(delta) => vec![0x01, delta as u8],
            KnobEvent::ShortPress => vec![0x02, 0],
            KnobEvent::LongPress => vec![0x03, 0],
        };
        self.queue_response(Frame::new(MessageType::KnobEvent, sequence, payload), None)
    }

    pub fn applied_snapshot(&self) -> Option<&DeviceSnapshot> {
        self.applied_snapshot.as_ref()
    }

    fn process_frame(&mut self, frame: Frame) -> Result<(), TransportError> {
        if frame.message_type == MessageType::Heartbeat && frame.payload.is_empty() {
            self.heartbeat_count += 1;
            return Ok(());
        }
        if matches!(
            frame.message_type,
            MessageType::FullSnapshot
                | MessageType::RingUpdate
                | MessageType::DisplayMode
                | MessageType::Brightness
        ) {
            self.state_write_count = self.state_write_count.saturating_add(1);
            if frame.message_type == MessageType::FullSnapshot {
                self.full_snapshot_count = self.full_snapshot_count.saturating_add(1);
            }
            if self.state_write_log.len() == RECENT_STATE_WRITE_LIMIT {
                self.state_write_log.remove(0);
            }
            self.state_write_log
                .push((frame.message_type, frame.sequence));
        }
        let response_fault = match self.faults.pop_front() {
            Some(Fault::NackOnce(reason)) => return self.queue_nack(&frame, reason, None),
            Some(Fault::MalformedNackOnce) => {
                return self.queue_response(
                    Frame::new(
                        MessageType::Nack,
                        frame.sequence,
                        vec![frame.message_type as u8],
                    ),
                    None,
                );
            }
            Some(Fault::UnknownNackReasonOnce(reason)) => {
                return self.queue_response(
                    Frame::new(
                        MessageType::Nack,
                        frame.sequence,
                        vec![frame.message_type as u8, reason],
                    ),
                    None,
                );
            }
            Some(Fault::TimeoutOnce) => Some(ResponseFault::Timeout),
            Some(Fault::AckBatchWithFutureOnce(message_type)) => {
                Some(ResponseFault::AckBatchWithFuture(message_type))
            }
            Some(Fault::CorruptCrcOnce) => Some(ResponseFault::CorruptCrc),
            None => None,
        };

        match frame.message_type {
            MessageType::Hello if frame.payload == [0] => self.queue_response(
                Frame::new(
                    MessageType::Capabilities,
                    frame.sequence,
                    self.capabilities_payload(),
                ),
                response_fault,
            ),
            MessageType::Hello => {
                self.queue_nack(&frame, NackReason::MalformedPayload, response_fault)
            }
            MessageType::FullSnapshot => match parse_snapshot(&frame.payload) {
                Some(snapshot) => {
                    self.applied_snapshot = Some(snapshot);
                    self.queue_ack(&frame, response_fault)
                }
                None => self.queue_nack(&frame, NackReason::MalformedPayload, response_fault),
            },
            MessageType::RingUpdate => match parse_ring(&frame.payload) {
                Some(ring) => {
                    if let Some(snapshot) = self.applied_snapshot.as_mut() {
                        let index = usize::from(ring.index);
                        snapshot.rings[index] = ring;
                        self.queue_ack(&frame, response_fault)
                    } else {
                        self.queue_nack(&frame, NackReason::InvalidState, response_fault)
                    }
                }
                None => self.queue_nack(&frame, NackReason::MalformedPayload, response_fault),
            },
            MessageType::DisplayMode => match parse_display(&frame.payload) {
                Some((display_mode, selected_ring)) => {
                    if let Some(snapshot) = self.applied_snapshot.as_mut() {
                        snapshot.display_mode = display_mode;
                        snapshot.selected_ring = selected_ring;
                        self.queue_ack(&frame, response_fault)
                    } else {
                        self.queue_nack(&frame, NackReason::InvalidState, response_fault)
                    }
                }
                None => self.queue_nack(&frame, NackReason::MalformedPayload, response_fault),
            },
            MessageType::Brightness => match frame.payload.as_slice() {
                [brightness] if *brightness <= 100 => {
                    if let Some(snapshot) = self.applied_snapshot.as_mut() {
                        snapshot.global_brightness = *brightness;
                        self.queue_ack(&frame, response_fault)
                    } else {
                        self.queue_nack(&frame, NackReason::InvalidState, response_fault)
                    }
                }
                _ => self.queue_nack(&frame, NackReason::MalformedPayload, response_fault),
            },
            MessageType::Heartbeat => {
                self.queue_nack(&frame, NackReason::MalformedPayload, response_fault)
            }
            MessageType::Capabilities
            | MessageType::Ack
            | MessageType::Nack
            | MessageType::KnobEvent
            | MessageType::Diagnostics => {
                self.queue_nack(&frame, NackReason::UnsupportedMessage, response_fault)
            }
        }
    }

    fn queue_ack(
        &mut self,
        request: &Frame,
        fault: Option<ResponseFault>,
    ) -> Result<(), TransportError> {
        self.queue_response(
            Frame::new(
                MessageType::Ack,
                request.sequence,
                vec![request.message_type as u8],
            ),
            fault,
        )
    }

    fn capabilities_payload(&self) -> Vec<u8> {
        let mut payload = vec![0, 0, 1, 0, 4];
        payload.extend_from_slice(&self.feature_flags.to_le_bytes());
        payload.extend_from_slice(&self.max_payload.to_le_bytes());
        payload
    }

    fn queue_nack(
        &mut self,
        request: &Frame,
        reason: NackReason,
        fault: Option<ResponseFault>,
    ) -> Result<(), TransportError> {
        self.queue_response(
            Frame::new(
                MessageType::Nack,
                request.sequence,
                vec![request.message_type as u8, reason as u8],
            ),
            fault,
        )
    }

    fn queue_response(
        &mut self,
        response: Frame,
        fault: Option<ResponseFault>,
    ) -> Result<(), TransportError> {
        let mut bytes = self.encode_response(&response)?;
        let fault = match fault {
            Some(ResponseFault::AckBatchWithFuture(message_type)) => {
                bytes.extend(self.encode_response(&Frame::new(
                    MessageType::Ack,
                    response.sequence.wrapping_add(1),
                    vec![message_type as u8],
                ))?);
                None
            }
            fault => fault,
        };
        self.responses.push_back(PendingResponse { bytes, fault });
        self.max_pending_response_count = self.max_pending_response_count.max(self.responses.len());
        Ok(())
    }

    fn encode_response(&self, response: &Frame) -> Result<Vec<u8>, TransportError> {
        let mut bytes = protocol::encode(response)?;
        if self.protocol_major != PROTOCOL_MAJOR {
            bytes[2] = self.protocol_major;
            let crc_offset = bytes.len() - 2;
            let crc = protocol::crc16_ccitt_false(&bytes[2..crc_offset]);
            bytes[crc_offset..].copy_from_slice(&crc.to_le_bytes());
        }
        Ok(bytes)
    }
}

impl DeviceTransport for SimulatedTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Simulator
    }

    fn discover(&mut self) -> Result<Vec<Endpoint>, TransportError> {
        Ok(vec![Endpoint::virtual_device()])
    }

    fn connect(&mut self, endpoint: &Endpoint) -> Result<(), TransportError> {
        if endpoint != &Endpoint::virtual_device() {
            return Err(TransportError::EndpointNotFound);
        }
        self.connected = true;
        self.decoder = Decoder::new(DecoderMode::StrictV01);
        self.responses.clear();
        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if !self.connected {
            return Err(TransportError::Disconnected);
        }
        let mut first_error = None;
        for decoded in self.decoder.push(bytes) {
            let result = match decoded {
                Ok(frame) => self.process_frame(frame),
                Err(ProtocolError::InvalidPayloadLength {
                    message_type,
                    sequence,
                    ..
                }) => self.queue_nack(
                    &Frame::new(message_type, sequence, Vec::new()),
                    NackReason::MalformedPayload,
                    None,
                ),
                Err(error) => Err(TransportError::Protocol(error)),
            };
            if first_error.is_none() {
                first_error = result.err();
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn read(&mut self) -> Result<Vec<u8>, TransportError> {
        if !self.connected {
            return Err(TransportError::Disconnected);
        }
        let Some(mut response) = self.responses.pop_front() else {
            return Ok(Vec::new());
        };

        match response.fault {
            Some(ResponseFault::Timeout) => Err(TransportError::Timeout),
            Some(ResponseFault::CorruptCrc) => {
                if let Some(crc_byte) = response.bytes.last_mut() {
                    *crc_byte ^= 0xff;
                }
                Ok(response.bytes)
            }
            Some(ResponseFault::AckBatchWithFuture(_)) => Ok(response.bytes),
            None => Ok(response.bytes),
        }
    }

    fn disconnect(&mut self) -> Result<(), TransportError> {
        self.connected = false;
        self.decoder = Decoder::new(DecoderMode::StrictV01);
        self.responses.clear();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

fn parse_snapshot(payload: &[u8]) -> Option<DeviceSnapshot> {
    if payload.len() != FULL_SNAPSHOT_PAYLOAD_BYTES || payload[11] != RING_COUNT as u8 {
        return None;
    }
    let revision = u64::from_le_bytes(payload[0..8].try_into().ok()?);
    let global_brightness = payload[8];
    if global_brightness > 100 {
        return None;
    }
    let display_mode = parse_display_mode(payload[9])?;
    let selected_ring = parse_selected_ring(payload[10])?;
    let rings = payload[FULL_SNAPSHOT_PREFIX_BYTES..]
        .chunks_exact(RING_PAYLOAD_BYTES)
        .enumerate()
        .map(|(position, bytes)| {
            let ring = parse_ring(bytes)?;
            (usize::from(ring.index) == position).then_some(ring)
        })
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()?;

    Some(DeviceSnapshot {
        revision,
        global_brightness,
        display_mode,
        selected_ring,
        rings,
    })
}

fn parse_ring(payload: &[u8]) -> Option<DeviceRing> {
    if payload.len() != RING_PAYLOAD_BYTES || payload[7] != 0 {
        return None;
    }
    let index = payload[0];
    let brightness = payload[2];
    let speed_percent = u16::from_le_bytes([payload[3], payload[4]]);
    let tail_percent = payload[6];
    if index >= RING_COUNT as u8
        || brightness > 100
        || !(25..=300).contains(&speed_percent)
        || tail_percent > 100
    {
        return None;
    }

    Some(DeviceRing {
        index,
        status: parse_task_status(payload[1])?,
        brightness,
        speed_percent,
        direction: parse_direction(payload[5])?,
        tail_percent,
        label: Vec::new(),
    })
}

fn parse_display(payload: &[u8]) -> Option<(DeviceDisplayMode, Option<u8>)> {
    match payload {
        [display_mode, selected_ring] => Some((
            parse_display_mode(*display_mode)?,
            parse_selected_ring(*selected_ring)?,
        )),
        _ => None,
    }
}

fn parse_display_mode(value: u8) -> Option<DeviceDisplayMode> {
    match value {
        0 => Some(DeviceDisplayMode::Ambient),
        1 => Some(DeviceDisplayMode::Overview),
        2 => Some(DeviceDisplayMode::Detail),
        _ => None,
    }
}

fn parse_selected_ring(value: u8) -> Option<Option<u8>> {
    match value {
        0..=3 => Some(Some(value)),
        0xff => Some(None),
        _ => None,
    }
}

fn parse_task_status(value: u8) -> Option<DeviceTaskStatus> {
    match value {
        1 => Some(DeviceTaskStatus::Running),
        2 => Some(DeviceTaskStatus::Waiting),
        3 => Some(DeviceTaskStatus::RoundCompleted),
        4 => Some(DeviceTaskStatus::Failed),
        5 => Some(DeviceTaskStatus::Queued),
        6 => Some(DeviceTaskStatus::Idle),
        7 => Some(DeviceTaskStatus::Unknown),
        _ => None,
    }
}

fn parse_direction(value: u8) -> Option<DeviceDirection> {
    match value {
        0 => Some(DeviceDirection::Clockwise),
        1 => Some(DeviceDirection::CounterClockwise),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Fault, KnobEvent, NackReason, SimulatedTransport};
    use crate::device::presentation::{
        DeviceDirection, DeviceDisplayMode, DeviceRing, DeviceSnapshot, DeviceTaskStatus,
        DeviceUpdate,
    };
    use crate::device::protocol::{self, Decoder, Frame, MessageType, ProtocolError};
    use crate::device::transport::{DeviceTransport, Endpoint, TransportError, TransportKind};

    fn fixture_device_snapshot() -> DeviceSnapshot {
        DeviceSnapshot {
            revision: 42,
            global_brightness: 73,
            display_mode: DeviceDisplayMode::Detail,
            selected_ring: Some(2),
            rings: std::array::from_fn(|index| DeviceRing {
                index: index as u8,
                status: DeviceTaskStatus::Running,
                brightness: 40 + index as u8,
                speed_percent: 75 + index as u16,
                direction: if index % 2 == 0 {
                    DeviceDirection::Clockwise
                } else {
                    DeviceDirection::CounterClockwise
                },
                tail_percent: 20 + index as u8,
                label: Vec::new(),
            }),
        }
    }

    fn decode_one(bytes: &[u8]) -> Frame {
        let frames = Decoder::default().push(bytes);
        assert_eq!(frames.len(), 1);
        frames.into_iter().next().unwrap().unwrap()
    }

    fn connect(simulator: &mut SimulatedTransport) {
        simulator.connect(&Endpoint::virtual_device()).unwrap();
    }

    fn write_frame(simulator: &mut SimulatedTransport, frame: Frame) {
        let bytes = protocol::encode(&frame).unwrap();
        simulator.write(&bytes).unwrap();
    }

    #[test]
    fn simulator_handshakes_acks_state_and_records_the_snapshot() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);

        write_frame(&mut simulator, Frame::new(MessageType::Hello, 1, vec![0]));
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Capabilities,
                1,
                vec![0, 0, 1, 0, 4, 0x03, 0x00, 0x00, 0x02],
            )
        );

        let snapshot = fixture_device_snapshot();
        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                2,
                snapshot.encode_payload().unwrap(),
            ),
        );
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(MessageType::Ack, 2, vec![MessageType::FullSnapshot as u8])
        );
        assert_eq!(simulator.applied_snapshot(), Some(&snapshot));
    }

    #[test]
    fn simulator_applies_incremental_state_writes_and_acks_each_sequence() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        let snapshot = fixture_device_snapshot();
        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                1,
                snapshot.encode_payload().unwrap(),
            ),
        );
        simulator.read().unwrap();

        let mut ring = snapshot.rings[3].clone();
        ring.status = DeviceTaskStatus::Waiting;
        let writes = [
            (
                MessageType::RingUpdate,
                DeviceUpdate::Ring(ring.clone()).encode_payload().unwrap(),
            ),
            (
                MessageType::DisplayMode,
                DeviceUpdate::Display {
                    mode: DeviceDisplayMode::Overview,
                    selected_ring: None,
                }
                .encode_payload()
                .unwrap(),
            ),
            (
                MessageType::Brightness,
                DeviceUpdate::Brightness(55).encode_payload().unwrap(),
            ),
        ];

        for (offset, (message_type, payload)) in writes.into_iter().enumerate() {
            let sequence = offset as u16 + 2;
            write_frame(&mut simulator, Frame::new(message_type, sequence, payload));
            assert_eq!(
                decode_one(&simulator.read().unwrap()),
                Frame::new(MessageType::Ack, sequence, vec![message_type as u8])
            );
        }

        let applied = simulator.applied_snapshot().unwrap();
        assert_eq!(applied.rings[3], ring);
        assert_eq!(applied.display_mode, DeviceDisplayMode::Overview);
        assert_eq!(applied.selected_ring, None);
        assert_eq!(applied.global_brightness, 55);
    }

    #[test]
    fn state_write_instrumentation_keeps_bounded_recent_history_and_cumulative_counts() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        let snapshot = fixture_device_snapshot();
        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                1,
                snapshot.encode_payload().unwrap(),
            ),
        );
        simulator.read().unwrap();

        for offset in 0..200_u16 {
            write_frame(
                &mut simulator,
                Frame::new(
                    MessageType::Brightness,
                    offset + 2,
                    vec![(offset % 101) as u8],
                ),
            );
            simulator.read().unwrap();
        }

        assert_eq!(simulator.state_write_count(), 201);
        assert_eq!(simulator.full_snapshot_count(), 1);
        assert_eq!(
            simulator.state_write_log().len(),
            super::RECENT_STATE_WRITE_LIMIT
        );
        assert!(simulator
            .state_write_log()
            .iter()
            .all(|(message_type, _)| *message_type == MessageType::Brightness));
    }

    #[test]
    fn malformed_payloads_are_nacked_without_mutating_applied_state() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        let snapshot = fixture_device_snapshot();
        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                1,
                snapshot.encode_payload().unwrap(),
            ),
        );
        simulator.read().unwrap();

        let mut malformed_snapshot = snapshot.encode_payload().unwrap();
        malformed_snapshot[8] = 101;
        write_frame(
            &mut simulator,
            Frame::new(MessageType::FullSnapshot, 2, malformed_snapshot),
        );
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Nack,
                2,
                vec![
                    MessageType::FullSnapshot as u8,
                    NackReason::MalformedPayload as u8
                ],
            )
        );
        assert_eq!(simulator.applied_snapshot(), Some(&snapshot));

        let mut malformed_ring = snapshot.rings[0].encode_payload().unwrap();
        malformed_ring[7] = 1;
        malformed_ring.push(b'x');
        write_frame(
            &mut simulator,
            Frame::new(MessageType::RingUpdate, 3, malformed_ring),
        );
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Nack,
                3,
                vec![
                    MessageType::RingUpdate as u8,
                    NackReason::MalformedPayload as u8
                ],
            )
        );
        assert_eq!(simulator.applied_snapshot(), Some(&snapshot));
    }

    #[test]
    fn scripted_faults_are_fifo_and_one_shot() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        simulator.script(Fault::TimeoutOnce);
        simulator.script(Fault::NackOnce(NackReason::Busy));
        simulator.script(Fault::CorruptCrcOnce);

        write_frame(&mut simulator, Frame::new(MessageType::Hello, 1, vec![0]));
        assert_eq!(simulator.read(), Err(TransportError::Timeout));
        assert_eq!(simulator.pending_fault_count(), 2);

        write_frame(&mut simulator, Frame::new(MessageType::Hello, 2, vec![0]));
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Nack,
                2,
                vec![MessageType::Hello as u8, NackReason::Busy as u8],
            )
        );
        assert_eq!(simulator.pending_fault_count(), 1);

        write_frame(&mut simulator, Frame::new(MessageType::Hello, 3, vec![0]));
        assert_eq!(
            Decoder::default().push(&simulator.read().unwrap()),
            vec![Err(ProtocolError::CrcMismatch)]
        );
        assert_eq!(simulator.pending_fault_count(), 0);

        write_frame(&mut simulator, Frame::new(MessageType::Hello, 4, vec![0]));
        assert_eq!(
            decode_one(&simulator.read().unwrap()).message_type,
            MessageType::Capabilities
        );
    }

    #[test]
    fn injected_nack_rejects_a_state_write_without_applying_it() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        simulator.script(Fault::NackOnce(NackReason::Busy));
        let snapshot = fixture_device_snapshot();

        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                7,
                snapshot.encode_payload().unwrap(),
            ),
        );

        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Nack,
                7,
                vec![MessageType::FullSnapshot as u8, NackReason::Busy as u8],
            )
        );
        assert_eq!(simulator.applied_snapshot(), None);
    }

    #[test]
    fn injected_nack_is_bound_to_exactly_one_queued_write() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        simulator.script(Fault::NackOnce(NackReason::Busy));
        let rejected = fixture_device_snapshot();
        let mut accepted = rejected.clone();
        accepted.revision = 43;
        accepted.global_brightness = 55;

        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                7,
                rejected.encode_payload().unwrap(),
            ),
        );
        write_frame(
            &mut simulator,
            Frame::new(
                MessageType::FullSnapshot,
                8,
                accepted.encode_payload().unwrap(),
            ),
        );

        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Nack,
                7,
                vec![MessageType::FullSnapshot as u8, NackReason::Busy as u8],
            )
        );
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(MessageType::Ack, 8, vec![MessageType::FullSnapshot as u8])
        );
        assert_eq!(simulator.applied_snapshot(), Some(&accepted));
    }

    #[test]
    fn simulator_uses_the_streaming_decoder_for_fragmented_writes() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        let hello = protocol::encode(&Frame::new(MessageType::Hello, 9, vec![0])).unwrap();

        simulator.write(&hello[..5]).unwrap();
        assert!(simulator.read().unwrap().is_empty());
        simulator.write(&hello[5..]).unwrap();

        assert_eq!(
            decode_one(&simulator.read().unwrap()).message_type,
            MessageType::Capabilities
        );
    }

    #[test]
    fn decoder_errors_do_not_drop_later_recovered_frames_from_the_same_write() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        let mut bytes = protocol::encode(&Frame::new(MessageType::Heartbeat, 1, vec![])).unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        bytes.extend(protocol::encode(&Frame::new(MessageType::Hello, 2, vec![0])).unwrap());

        assert_eq!(
            simulator.write(&bytes),
            Err(TransportError::Protocol(ProtocolError::CrcMismatch))
        );
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Capabilities,
                2,
                vec![0, 0, 1, 0, 4, 0x03, 0x00, 0x00, 0x02],
            )
        );
    }

    #[test]
    fn simulator_rejects_impossible_fixed_length_before_processing_nested_hello() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);
        let mut bytes = vec![0x43, 0x48, 0x01, 0x02, 0x09, 0x00, 0x00, 0x02];
        bytes.extend(protocol::encode(&Frame::new(MessageType::Hello, 10, vec![0])).unwrap());

        assert_eq!(simulator.write(&bytes), Ok(()));
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Nack,
                9,
                vec![
                    MessageType::Capabilities as u8,
                    NackReason::MalformedPayload as u8,
                ],
            )
        );
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(
                MessageType::Capabilities,
                10,
                vec![0, 0, 1, 0, 4, 0x03, 0x00, 0x00, 0x02],
            )
        );
    }

    #[test]
    fn injected_knob_events_use_protocol_payload_values_and_sequences() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);

        simulator.inject_knob(KnobEvent::Rotate(-1)).unwrap();
        simulator.inject_knob(KnobEvent::ShortPress).unwrap();
        simulator.inject_knob(KnobEvent::LongPress).unwrap();

        let expected = [(0, vec![1, 0xff]), (1, vec![2, 0]), (2, vec![3, 0])];
        for (sequence, payload) in expected {
            assert_eq!(
                decode_one(&simulator.read().unwrap()),
                Frame::new(MessageType::KnobEvent, sequence, payload)
            );
        }
    }

    #[test]
    fn zero_delta_knob_rotation_is_rejected_without_queueing_or_advancing_sequence() {
        let mut simulator = SimulatedTransport::default();
        connect(&mut simulator);

        assert_eq!(
            simulator.inject_knob(KnobEvent::Rotate(0)),
            Err(TransportError::InvalidKnobDelta)
        );
        assert!(simulator.read().unwrap().is_empty());

        simulator.inject_knob(KnobEvent::Rotate(1)).unwrap();
        assert_eq!(
            decode_one(&simulator.read().unwrap()),
            Frame::new(MessageType::KnobEvent, 0, vec![1, 1])
        );
    }

    #[test]
    fn simulator_discovers_and_connects_only_its_virtual_endpoint() {
        let mut simulator = SimulatedTransport::default();
        assert_eq!(simulator.kind(), TransportKind::Simulator);
        assert_eq!(
            simulator.discover().unwrap(),
            vec![Endpoint::virtual_device()]
        );
        assert!(!simulator.is_connected());

        assert_eq!(
            simulator.connect(&Endpoint {
                id: "other".into(),
                label: "Other".into(),
            }),
            Err(TransportError::EndpointNotFound)
        );
        connect(&mut simulator);
        assert!(simulator.is_connected());
        simulator.disconnect().unwrap();
        assert!(!simulator.is_connected());
    }
}
