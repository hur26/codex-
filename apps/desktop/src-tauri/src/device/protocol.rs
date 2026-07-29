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

    fn try_from(value: u8) -> Result<Self, Self::Error> {
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
    UnsupportedVersion { actual: u8 },
    UnknownMessageType { actual: u8 },
    PayloadTooLarge { actual: usize },
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

#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
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
                self.buffer.drain(..frame_length);
                continue;
            }

            let version = self.buffer[2];
            if version != PROTOCOL_MAJOR {
                decoded.push(Err(ProtocolError::UnsupportedVersion { actual: version }));
                self.buffer.clear();
                return;
            }

            let message_type = match MessageType::try_from(self.buffer[3]) {
                Ok(message_type) => message_type,
                Err(error) => {
                    decoded.push(Err(error));
                    self.buffer.clear();
                    return;
                }
            };
            let sequence = u16::from_le_bytes([self.buffer[4], self.buffer[5]]);
            let payload = self.buffer[HEADER_BYTES..HEADER_BYTES + payload_length].to_vec();
            decoded.push(Ok(Frame::new(message_type, sequence, payload)));
            self.buffer.clear();
            return;
        }
    }

    fn resynchronize(&mut self) {
        let next_magic = self.buffer[1..]
            .windows(MAGIC.len())
            .position(|window| window == MAGIC)
            .map(|position| position + 1);

        if let Some(position) = next_magic {
            self.buffer.drain(..position);
        } else if self.buffer.last() == Some(&MAGIC[0]) {
            self.buffer.drain(..self.buffer.len() - 1);
        } else {
            self.buffer.clear();
        }
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

    #[test]
    fn encodes_the_hello_golden_vector() {
        let frame = Frame::new(MessageType::Hello, 1, vec![0]);
        assert_eq!(
            hex(&encode(&frame).expect("HELLO frame encodes")),
            "4348010101000100006e91"
        );
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
    fn crc_error_ignores_false_magic_inside_the_completed_bad_frame() {
        let false_header = decode_hex("4348012000000002");
        let mut corrupt = encode(&Frame::new(MessageType::Diagnostics, 9, false_header))
            .expect("fixture encodes");
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
