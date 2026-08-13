pub const MAGIC: [u8; 2] = *b"CH";
pub const PROTOCOL_MAJOR: u8 = 1;
pub const MAX_PAYLOAD: usize = 512;

const HEADER_BYTES: usize = 8;
const CRC_BYTES: usize = 2;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageType {
    Hello = 0x01,
    Capabilities = 0x02,
    FullSnapshot = 0x10,
    RingUpdate = 0x11,
    DisplayMode = 0x12,
    Brightness = 0x13,
    Heartbeat = 0x20,
    Ack = 0x70,
    Nack = 0x71,
    KnobEvent = 0x80,
    Diagnostics = 0x81,
}

impl TryFrom<u8> for MessageType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0x01 => Ok(Self::Hello),
            0x02 => Ok(Self::Capabilities),
            0x10 => Ok(Self::FullSnapshot),
            0x11 => Ok(Self::RingUpdate),
            0x12 => Ok(Self::DisplayMode),
            0x13 => Ok(Self::Brightness),
            0x20 => Ok(Self::Heartbeat),
            0x70 => Ok(Self::Ack),
            0x71 => Ok(Self::Nack),
            0x80 => Ok(Self::KnobEvent),
            0x81 => Ok(Self::Diagnostics),
            actual => Err(ProtocolError::UnknownMessageType { actual }),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Info = 0x01,
    Warning = 0x02,
    Error = 0x03,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDiagnosticSeverity;

impl TryFrom<u8> for DiagnosticSeverity {
    type Error = InvalidDiagnosticSeverity;

    fn try_from(value: u8) -> Result<Self, InvalidDiagnosticSeverity> {
        match value {
            0x01 => Ok(Self::Info),
            0x02 => Ok(Self::Warning),
            0x03 => Ok(Self::Error),
            _ => Err(InvalidDiagnosticSeverity),
        }
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticCode {
    WatchdogDisconnected = 0x0001,
    CrcError = 0x0002,
    InvalidPayload = 0x0003,
    LocalLimit = 0x0004,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDiagnosticCode;

impl TryFrom<u16> for DiagnosticCode {
    type Error = InvalidDiagnosticCode;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x0001 => Ok(Self::WatchdogDisconnected),
            0x0002 => Ok(Self::CrcError),
            0x0003 => Ok(Self::InvalidPayload),
            0x0004 => Ok(Self::LocalLimit),
            _ => Err(InvalidDiagnosticCode),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub value: u32,
}

impl Diagnostic {
    pub fn encode_payload(self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(7);
        payload.push(self.severity as u8);
        payload.extend_from_slice(&(self.code as u16).to_le_bytes());
        payload.extend_from_slice(&self.value.to_le_bytes());
        payload
    }

    pub fn decode_payload(payload: &[u8]) -> Option<Self> {
        let [severity, code_low, code_high, value @ ..] = payload else {
            return None;
        };
        let value: [u8; 4] = (*value).try_into().ok()?;
        Some(Self {
            severity: DiagnosticSeverity::try_from(*severity).ok()?,
            code: DiagnosticCode::try_from(u16::from_le_bytes([*code_low, *code_high])).ok()?,
            value: u32::from_le_bytes(value),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub message_type: MessageType,
    pub sequence: u16,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: MessageType, sequence: u16, payload: Vec<u8>) -> Self {
        Self {
            message_type,
            sequence,
            payload,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    UnsupportedVersion {
        actual: u8,
    },
    UnknownMessageType {
        actual: u8,
    },
    PayloadTooLarge {
        actual: usize,
    },
    InvalidPayloadLength {
        message_type: MessageType,
        sequence: u16,
        actual: usize,
    },
    CrcMismatch,
}

pub fn encode(frame: &Frame) -> Result<Vec<u8>, ProtocolError> {
    let payload_length =
        u16::try_from(frame.payload.len()).map_err(|_| ProtocolError::PayloadTooLarge {
            actual: frame.payload.len(),
        })?;
    if frame.payload.len() > MAX_PAYLOAD {
        return Err(ProtocolError::PayloadTooLarge {
            actual: frame.payload.len(),
        });
    }

    let mut encoded = Vec::with_capacity(HEADER_BYTES + frame.payload.len() + CRC_BYTES);
    encoded.extend_from_slice(&MAGIC);
    encoded.push(PROTOCOL_MAJOR);
    encoded.push(frame.message_type as u8);
    encoded.extend_from_slice(&frame.sequence.to_le_bytes());
    encoded.extend_from_slice(&payload_length.to_le_bytes());
    encoded.extend_from_slice(&frame.payload);
    let crc = crc16_ccitt_false(&encoded[MAGIC.len()..]);
    encoded.extend_from_slice(&crc.to_le_bytes());
    Ok(encoded)
}

pub fn crc16_ccitt_false(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecoderMode {
    #[default]
    Generic,
    StrictV01,
}

pub struct Decoder {
    buffer: Vec<u8>,
    mode: DecoderMode,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new(DecoderMode::Generic)
    }
}

impl Decoder {
    pub fn new(mode: DecoderMode) -> Self {
        Self {
            buffer: Vec::new(),
            mode,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<Frame, ProtocolError>> {
        let mut decoded = Vec::new();
        for byte in bytes {
            self.push_byte(*byte);
            self.decode_ready(&mut decoded);
        }
        decoded
    }

    fn push_byte(&mut self, byte: u8) {
        match self.buffer.as_slice() {
            [] if byte == MAGIC[0] => self.buffer.push(byte),
            [] => {}
            [first] if *first == MAGIC[0] && byte == MAGIC[1] => self.buffer.push(byte),
            [first] if *first == MAGIC[0] && byte == MAGIC[0] => {}
            [_] => self.buffer.clear(),
            _ => self.buffer.push(byte),
        }
    }

    fn decode_ready(&mut self, decoded: &mut Vec<Result<Frame, ProtocolError>>) {
        loop {
            if self.buffer.len() < HEADER_BYTES {
                return;
            }

            let payload_length = usize::from(u16::from_le_bytes([self.buffer[6], self.buffer[7]]));
            if payload_length > MAX_PAYLOAD {
                decoded.push(Err(ProtocolError::PayloadTooLarge {
                    actual: payload_length,
                }));
                self.resynchronize();
                continue;
            }

            let version = self.buffer[2];
            if version != PROTOCOL_MAJOR {
                decoded.push(Err(ProtocolError::UnsupportedVersion { actual: version }));
                self.resynchronize();
                continue;
            }

            let message_type = match MessageType::try_from(self.buffer[3]) {
                Ok(message_type) => message_type,
                Err(error) => {
                    decoded.push(Err(error));
                    self.resynchronize();
                    continue;
                }
            };

            if self.mode == DecoderMode::StrictV01
                && payload_length != expected_v01_payload_length(message_type)
            {
                decoded.push(Err(ProtocolError::InvalidPayloadLength {
                    message_type,
                    sequence: u16::from_le_bytes([self.buffer[4], self.buffer[5]]),
                    actual: payload_length,
                }));
                self.resynchronize();
                continue;
            }

            let frame_length = HEADER_BYTES + payload_length + CRC_BYTES;
            if self.buffer.len() < frame_length {
                return;
            }

            let expected_crc = u16::from_le_bytes([
                self.buffer[frame_length - CRC_BYTES],
                self.buffer[frame_length - 1],
            ]);
            let actual_crc = crc16_ccitt_false(&self.buffer[MAGIC.len()..frame_length - CRC_BYTES]);
            if actual_crc != expected_crc {
                decoded.push(Err(ProtocolError::CrcMismatch));
                self.resynchronize();
                continue;
            }

            let sequence = u16::from_le_bytes([self.buffer[4], self.buffer[5]]);
            let payload = self.buffer[HEADER_BYTES..HEADER_BYTES + payload_length].to_vec();
            decoded.push(Ok(Frame::new(message_type, sequence, payload)));
            self.buffer.drain(..frame_length);
            self.align_to_magic();
        }
    }

    fn align_to_magic(&mut self) {
        self.synchronize_from(0);
    }

    fn resynchronize(&mut self) {
        self.synchronize_from(1);
    }

    fn synchronize_from(&mut self, search_start: usize) {
        let next_magic = self
            .buffer
            .get(search_start..)
            .and_then(|suffix| {
                suffix
                    .windows(MAGIC.len())
                    .position(|window| window == MAGIC)
            })
            .map(|position| position + search_start);
        if let Some(position) = next_magic {
            self.buffer.drain(..position);
        } else if self.buffer.last() == Some(&MAGIC[0]) {
            self.buffer.drain(..self.buffer.len() - 1);
        } else {
            self.buffer.clear();
        }
    }
}

fn expected_v01_payload_length(message_type: MessageType) -> usize {
    match message_type {
        MessageType::Hello | MessageType::Ack | MessageType::Brightness => 1,
        MessageType::Capabilities => 9,
        MessageType::FullSnapshot => 44,
        MessageType::RingUpdate => 8,
        MessageType::DisplayMode | MessageType::Nack | MessageType::KnobEvent => 2,
        MessageType::Heartbeat => 0,
        MessageType::Diagnostics => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let digits = std::str::from_utf8(pair).expect("hex fixture is UTF-8");
                u8::from_str_radix(digits, 16).expect("hex fixture is valid")
            })
            .collect()
    }

    fn decoder_stream_fixture(name: &str) -> Vec<&'static str> {
        include_str!("../../../../../docs/protocol/decoder-stream-vectors.tsv")
            .lines()
            .skip(1)
            .map(|line| {
                let mut columns = line.split('\t').collect::<Vec<_>>();
                assert!(columns.len() <= 11, "invalid decoder stream row: {line}");
                columns.resize(11, "");
                columns
            })
            .find(|columns| columns.first() == Some(&name))
            .unwrap_or_else(|| panic!("missing decoder stream fixture: {name}"))
    }

    #[test]
    fn encodes_the_hello_golden_vector() {
        let frame = Frame::new(MessageType::Hello, 1, vec![0]);
        assert_eq!(
            hex(&encode(&frame).expect("HELLO frame encodes")),
            "4348010101000100006e91"
        );
    }

    #[test]
    fn diagnostics_payload_round_trips_fixed_semantics_and_little_endian_value() {
        let diagnostic = Diagnostic {
            severity: DiagnosticSeverity::Warning,
            code: DiagnosticCode::CrcError,
            value: 0x7856_3412,
        };

        assert_eq!(
            diagnostic.encode_payload(),
            vec![0x02, 0x02, 0x00, 0x12, 0x34, 0x56, 0x78]
        );
        assert_eq!(
            Diagnostic::decode_payload(&diagnostic.encode_payload()),
            Some(diagnostic)
        );
    }

    #[test]
    fn diagnostics_payload_rejects_invalid_length_severity_and_code() {
        for payload in [
            vec![1, 1, 0, 0, 0, 0],
            vec![1, 1, 0, 0, 0, 0, 0, 0],
            vec![0, 1, 0, 0, 0, 0, 0],
            vec![4, 1, 0, 0, 0, 0, 0],
            vec![1, 0, 0, 0, 0, 0, 0],
            vec![1, 5, 0, 0, 0, 0, 0],
        ] {
            assert_eq!(Diagnostic::decode_payload(&payload), None, "{payload:?}");
        }
    }

    #[test]
    fn decodes_fragmented_frames_and_resynchronizes_after_noise() {
        let valid = decode_hex("43480120020000006cae");
        let mut decoder = Decoder::default();
        assert!(decoder.push(&[0xff, 0x00, valid[0]]).is_empty());
        let frames = decoder.push(&valid[1..]);
        assert_eq!(
            frames,
            vec![Ok(Frame::new(MessageType::Heartbeat, 2, vec![]))]
        );
    }

    #[test]
    fn rejects_crc_errors_and_oversized_lengths_without_allocating_the_claimed_size() {
        let mut corrupt = decode_hex("4348010101000100006e91");
        *corrupt.last_mut().expect("fixture has CRC") ^= 0xff;
        let mut decoder = Decoder::default();
        assert_eq!(
            decoder.push(&corrupt),
            vec![Err(ProtocolError::CrcMismatch)]
        );

        let oversized = [0x43, 0x48, 0x01, 0x01, 0x01, 0x00, 0x01, 0x02];
        assert_eq!(
            decoder.push(&oversized),
            vec![Err(ProtocolError::PayloadTooLarge { actual: 513 })]
        );
        assert!(
            decoder.buffer.len() <= MAGIC.len(),
            "oversized declaration must not grow the decoder buffer"
        );
    }

    #[test]
    fn rejects_unknown_versions_and_message_types_with_structured_errors() {
        let unsupported_version = decode_hex("43480220020000008c60");
        let unknown_message = decode_hex("434801ff03000000c38a");
        let mut decoder = Decoder::default();

        assert_eq!(
            decoder.push(&unsupported_version),
            vec![Err(ProtocolError::UnsupportedVersion { actual: 2 })]
        );
        assert_eq!(
            decoder.push(&unknown_message),
            vec![Err(ProtocolError::UnknownMessageType { actual: 0xff })]
        );
    }

    #[test]
    fn recovers_from_an_invalid_header_before_waiting_for_its_declared_payload() {
        let mut invalid_header = decode_hex("434802ff00000002");
        invalid_header.extend(decode_hex("43480120020000006cae"));
        let mut decoder = Decoder::default();

        assert_eq!(
            decoder.push(&invalid_header),
            vec![
                Err(ProtocolError::UnsupportedVersion { actual: 2 }),
                Ok(Frame::new(MessageType::Heartbeat, 2, vec![]))
            ]
        );
    }

    #[test]
    fn preserves_overlapping_magic_when_rejecting_an_invalid_header() {
        let mut decoder = Decoder::default();

        assert_eq!(
            decoder.push(&decode_hex("434843480120020000006cae")),
            vec![
                Err(ProtocolError::UnsupportedVersion { actual: 0x43 }),
                Ok(Frame::new(MessageType::Heartbeat, 2, vec![]))
            ]
        );
    }

    #[test]
    fn rejects_payloads_over_the_limit_before_encoding() {
        let frame = Frame::new(MessageType::Diagnostics, 7, vec![0; MAX_PAYLOAD + 1]);

        assert_eq!(
            encode(&frame),
            Err(ProtocolError::PayloadTooLarge {
                actual: MAX_PAYLOAD + 1
            })
        );
    }

    #[test]
    fn recovers_after_bad_crc_and_decodes_the_following_frame() {
        let mut corrupt = decode_hex("4348010101000100006e91");
        *corrupt.last_mut().expect("fixture has CRC") ^= 0xff;
        corrupt.extend(decode_hex("43480120020000006cae"));
        let mut decoder = Decoder::default();

        assert_eq!(
            decoder.push(&corrupt),
            vec![
                Err(ProtocolError::CrcMismatch),
                Ok(Frame::new(MessageType::Heartbeat, 2, vec![]))
            ]
        );
    }

    #[test]
    fn crc_error_resynchronizes_to_valid_frame_inside_bad_candidate() {
        let fixture = decoder_stream_fixture("crc_nested_valid");
        assert_eq!(fixture.len(), 11);
        assert_eq!(fixture[1], "generic");
        assert_eq!(fixture[3], "crc_mismatch");
        let stream = decode_hex(fixture[2]);
        let mut decoder = Decoder::default();

        assert_eq!(
            decoder.push(&stream),
            vec![
                Err(ProtocolError::CrcMismatch),
                Ok(Frame::new(
                    MessageType::try_from(u8::from_str_radix(fixture[4], 16).unwrap()).unwrap(),
                    u16::from_str_radix(fixture[5], 16).unwrap(),
                    decode_hex(fixture[6]),
                )),
            ]
        );
    }

    #[test]
    fn strict_v01_rejects_impossible_fixed_length_and_recovers_nested_frame() {
        let fixture = decoder_stream_fixture("strict_invalid_length_nested");
        assert_eq!(fixture.len(), 11);
        assert_eq!(fixture[1], "strict_v01");
        assert_eq!(fixture[3], "invalid_payload_length");
        let mut decoder = Decoder::new(DecoderMode::StrictV01);

        assert_eq!(
            decoder.push(&decode_hex(fixture[2])),
            vec![
                Err(ProtocolError::InvalidPayloadLength {
                    message_type: MessageType::Capabilities,
                    sequence: 9,
                    actual: MAX_PAYLOAD,
                }),
                Ok(Frame::new(
                    MessageType::try_from(u8::from_str_radix(fixture[4], 16).unwrap()).unwrap(),
                    u16::from_str_radix(fixture[5], 16).unwrap(),
                    decode_hex(fixture[6]),
                )),
            ]
        );
    }

    #[test]
    fn generic_decoder_retains_a_maximum_frame_containing_nested_magic() {
        let nested = decode_hex("43480120020000006cae");
        let mut payload = vec![0xa5; MAX_PAYLOAD];
        payload[..nested.len()].copy_from_slice(&nested);
        let frame = Frame::new(MessageType::Diagnostics, 24, payload);
        let encoded = encode(&frame).expect("maximum generic frame encodes");
        let prefix_length = HEADER_BYTES + nested.len();
        let mut decoder = Decoder::default();

        assert!(decoder.push(&encoded[..prefix_length]).is_empty());
        assert_eq!(decoder.buffer.len(), prefix_length);
        assert_eq!(decoder.push(&encoded[prefix_length..]), vec![Ok(frame)]);
    }

    #[test]
    fn crc_recovery_keeps_two_nested_frames_and_normalizes_the_outer_tail() {
        let fixture = decoder_stream_fixture("crc_two_nested_valid");
        assert_eq!(fixture.len(), 11);
        assert_eq!(fixture[1], "generic");
        assert_eq!(fixture[3], "crc_mismatch");
        let mut decoder = Decoder::default();

        assert_eq!(
            decoder.push(&decode_hex(fixture[2])),
            vec![
                Err(ProtocolError::CrcMismatch),
                Ok(Frame::new(MessageType::Heartbeat, 2, vec![])),
                Ok(Frame::new(MessageType::Brightness, 0x1234, vec![0x50])),
            ]
        );
        assert_eq!(hex(&decoder.buffer), fixture[8]);

        assert_eq!(fixture[7], "4348011334120100502983");
        assert_eq!(fixture[9], fixture[10]);
        assert_eq!(
            decoder.push(&decode_hex(fixture[9])),
            vec![Ok(Frame::new(MessageType::Hello, 1, vec![0]))]
        );
        assert!(decoder.buffer.is_empty());
    }

    #[test]
    fn encodes_and_decodes_all_published_golden_vectors() {
        let vectors = include_str!("../../../../../docs/protocol/golden-vectors.tsv");
        for line in vectors.lines().skip(1) {
            let columns: Vec<_> = line.split('\t').collect();
            assert_eq!(columns.len(), 5, "invalid TSV row: {line}");
            let message_type = MessageType::try_from(
                u8::from_str_radix(columns[1], 16).expect("message type hex"),
            )
            .expect("known message type");
            let sequence = u16::from_str_radix(columns[2], 16).expect("sequence hex");
            let payload = decode_hex(columns[3]);
            let frame = Frame::new(message_type, sequence, payload);
            assert_eq!(
                hex(&encode(&frame).expect("golden frame encodes")),
                columns[4]
            );
            let mut decoder = Decoder::default();
            assert_eq!(
                decoder.push(&decode_hex(columns[4])),
                vec![Ok(frame)],
                "golden frame decodes: {}",
                columns[0]
            );
        }
    }

    #[test]
    fn round_trips_a_frame_with_the_maximum_payload() {
        let payload = (0..MAX_PAYLOAD)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let frame = Frame::new(MessageType::Diagnostics, u16::MAX, payload);
        let encoded = encode(&frame).expect("maximum payload encodes");
        assert_eq!(encoded.len(), HEADER_BYTES + MAX_PAYLOAD + CRC_BYTES);

        let mut decoder = Decoder::default();
        assert!(decoder.push(&encoded[..257]).is_empty());
        assert_eq!(decoder.push(&encoded[257..]), vec![Ok(frame)]);
    }
}
