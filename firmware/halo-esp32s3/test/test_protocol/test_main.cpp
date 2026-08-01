#include <unity.h>

#include <HaloProtocol.hpp>

#include <cstdint>
#include <fstream>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

void setUp() {}
void tearDown() {}

namespace {

struct GoldenVector {
  uint8_t messageType;
  uint16_t sequence;
  std::vector<uint8_t> payload;
  std::vector<uint8_t> frame;
};

struct DecoderStreamVector {
  std::string mode;
  std::vector<uint8_t> stream;
  std::string expectedError;
  uint8_t recoveredMessageType;
  uint16_t recoveredSequence;
  std::vector<uint8_t> recoveredPayload;
  std::vector<uint8_t> additionalRecoveredFrame;
  std::vector<uint8_t> bufferedTail;
  std::vector<uint8_t> followupStream;
  std::vector<uint8_t> followupFrame;
};

std::vector<uint8_t> fromHex(const std::string& value) {
  if (value.size() % 2 != 0) {
    throw std::runtime_error("hex value has odd length");
  }

  std::vector<uint8_t> bytes;
  bytes.reserve(value.size() / 2);
  for (size_t index = 0; index < value.size(); index += 2) {
    bytes.push_back(static_cast<uint8_t>(
        std::stoul(value.substr(index, 2), nullptr, 16)));
  }
  return bytes;
}

std::string toHex(const std::vector<uint8_t>& bytes) {
  static constexpr char kDigits[] = "0123456789abcdef";
  std::string result;
  result.reserve(bytes.size() * 2);
  for (const uint8_t byte : bytes) {
    result.push_back(kDigits[byte >> 4]);
    result.push_back(kDigits[byte & 0x0f]);
  }
  return result;
}

std::map<std::string, GoldenVector> loadGoldenVectors() {
  std::ifstream input("../../docs/protocol/golden-vectors.tsv");
  if (!input) {
    throw std::runtime_error("cannot open shared golden vectors");
  }

  std::map<std::string, GoldenVector> vectors;
  std::string line;
  std::getline(input, line);
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') {
      line.pop_back();
    }
    if (line.empty()) {
      continue;
    }

    std::vector<std::string> columns;
    std::stringstream row(line);
    std::string column;
    while (std::getline(row, column, '\t')) {
      columns.push_back(column);
    }
    if (!line.empty() && line.back() == '\t') {
      columns.emplace_back();
    }
    if (columns.size() != 5) {
      throw std::runtime_error("invalid shared golden vector row");
    }

    vectors.emplace(
        columns[0],
        GoldenVector{
            static_cast<uint8_t>(std::stoul(columns[1], nullptr, 16)),
            static_cast<uint16_t>(std::stoul(columns[2], nullptr, 16)),
            fromHex(columns[3]),
            fromHex(columns[4]),
        });
  }
  return vectors;
}

std::map<std::string, DecoderStreamVector> loadDecoderStreamVectors() {
  std::ifstream input("../../docs/protocol/decoder-stream-vectors.tsv");
  if (!input) {
    throw std::runtime_error("cannot open shared decoder stream vectors");
  }

  std::map<std::string, DecoderStreamVector> vectors;
  std::string line;
  std::getline(input, line);
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') {
      line.pop_back();
    }
    if (line.empty()) {
      continue;
    }

    std::vector<std::string> columns;
    std::stringstream row(line);
    std::string column;
    while (std::getline(row, column, '\t')) {
      columns.push_back(column);
    }
    if (!line.empty() && line.back() == '\t') {
      columns.emplace_back();
    }
    while (columns.size() < 11) {
      columns.emplace_back();
    }
    if (columns.size() != 11) {
      throw std::runtime_error("invalid shared decoder stream vector row");
    }

    vectors.emplace(
        columns[0],
        DecoderStreamVector{
            columns[1], fromHex(columns[2]), columns[3],
            static_cast<uint8_t>(std::stoul(columns[4], nullptr, 16)),
            static_cast<uint16_t>(std::stoul(columns[5], nullptr, 16)),
            fromHex(columns[6]), fromHex(columns[7]), fromHex(columns[8]),
            fromHex(columns[9]), fromHex(columns[10]),
        });
  }
  return vectors;
}

halo::MessageType messageType(uint8_t value) {
  return static_cast<halo::MessageType>(value);
}

void assertFrame(const halo::DecodeResult& result,
                 halo::MessageType type,
                 uint16_t sequence,
                 const std::vector<uint8_t>& payload) {
  TEST_ASSERT_TRUE(result.ok());
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(type),
                          static_cast<uint8_t>(result.frame.type));
  TEST_ASSERT_EQUAL_UINT16(sequence, result.frame.sequence);
  TEST_ASSERT_EQUAL_UINT32(payload.size(), result.frame.payload.size());
  if (!payload.empty()) {
    TEST_ASSERT_EQUAL_UINT8_ARRAY(payload.data(), result.frame.payload.data(),
                                  payload.size());
  }
}

void test_protocol_constants_and_all_message_types_are_stable() {
  TEST_ASSERT_EQUAL_UINT8(0x43, halo::kMagic[0]);
  TEST_ASSERT_EQUAL_UINT8(0x48, halo::kMagic[1]);
  TEST_ASSERT_EQUAL_UINT8(1, halo::kProtocolMajor);
  TEST_ASSERT_EQUAL_UINT32(512, halo::kMaxPayload);
  const uint8_t expected[] = {0x01, 0x02, 0x10, 0x11, 0x12, 0x13,
                              0x20, 0x70, 0x71, 0x80, 0x81};
  const halo::MessageType actual[] = {
      halo::MessageType::Hello,        halo::MessageType::Capabilities,
      halo::MessageType::FullSnapshot, halo::MessageType::RingUpdate,
      halo::MessageType::DisplayMode,  halo::MessageType::Brightness,
      halo::MessageType::Heartbeat,    halo::MessageType::Ack,
      halo::MessageType::Nack,         halo::MessageType::KnobEvent,
      halo::MessageType::Diagnostics,
  };
  for (size_t index = 0; index < 11; ++index) {
    TEST_ASSERT_EQUAL_UINT8(expected[index],
                            static_cast<uint8_t>(actual[index]));
  }
}

void test_hello_matches_shared_golden_vector() {
  const auto vectors = loadGoldenVectors();
  const halo::Frame hello{halo::MessageType::Hello, 1, {0}};
  TEST_ASSERT_EQUAL_STRING(toHex(vectors.at("hello").frame).c_str(),
                           toHex(halo::encode(hello)).c_str());
}

void test_all_shared_golden_vectors_encode_and_decode() {
  const auto vectors = loadGoldenVectors();
  TEST_ASSERT_EQUAL_UINT32(4, vectors.size());
  for (const auto& entry : vectors) {
    const GoldenVector& vector = entry.second;
    const halo::Frame frame{messageType(vector.messageType), vector.sequence,
                            vector.payload};
    TEST_ASSERT_EQUAL_STRING(toHex(vector.frame).c_str(),
                             toHex(halo::encode(frame)).c_str());

    halo::Decoder decoder;
    const auto decoded = decoder.push(vector.frame.data(), vector.frame.size());
    TEST_ASSERT_EQUAL_UINT32(1, decoded.size());
    assertFrame(decoded[0], frame.type, frame.sequence, frame.payload);
  }
}

void test_crc_matches_ccitt_false_reference() {
  const auto covered = fromHex("01010100010000");
  TEST_ASSERT_EQUAL_HEX16(
      0x916e, halo::crc16CcittFalse(covered.data(), covered.size()));
}

void test_fragmented_frame_is_retained_until_complete() {
  const auto bytes = loadGoldenVectors().at("hello").frame;
  halo::Decoder decoder;
  TEST_ASSERT_TRUE(decoder.push(bytes.data(), 3).empty());
  TEST_ASSERT_TRUE(decoder.push(bytes.data() + 3, 5).empty());
  const auto decoded = decoder.push(bytes.data() + 8, bytes.size() - 8);
  TEST_ASSERT_EQUAL_UINT32(1, decoded.size());
  assertFrame(decoded[0], halo::MessageType::Hello, 1, {0});
}

void test_minimum_frame_is_ten_bytes_and_decodes_one_byte_at_a_time() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  TEST_ASSERT_EQUAL_UINT32(10, heartbeat.size());

  halo::Decoder decoder;
  std::vector<halo::DecodeResult> decoded;
  for (const uint8_t byte : heartbeat) {
    const auto next = decoder.push(&byte, 1);
    decoded.insert(decoded.end(), next.begin(), next.end());
  }

  TEST_ASSERT_EQUAL_UINT32(1, decoded.size());
  assertFrame(decoded[0], halo::MessageType::Heartbeat, 2, {});
  TEST_ASSERT_EQUAL_UINT32(0, decoder.bufferedSize());
}

void test_crc_error_is_reported_and_following_frame_decodes() {
  const auto vectors = loadGoldenVectors();
  auto corrupt = vectors.at("hello").frame;
  corrupt.back() ^= 0xff;
  const auto heartbeat = vectors.at("heartbeat").frame;
  corrupt.insert(corrupt.end(), heartbeat.begin(), heartbeat.end());

  halo::Decoder decoder;
  const auto decoded = decoder.push(corrupt.data(), corrupt.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_FALSE(decoded[0].ok());
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::ProtocolError::CrcMismatch),
                          static_cast<uint8_t>(decoded[0].error));
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::MessageType::Hello),
                          decoded[0].context.rawMessageType);
  TEST_ASSERT_EQUAL_UINT16(1, decoded[0].context.sequence);
  TEST_ASSERT_FALSE(decoded[0].context.respondable);
  assertFrame(decoded[1], halo::MessageType::Heartbeat, 2, {});
}

void test_oversized_length_is_rejected_without_buffer_growth() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  auto bytes = fromHex("4348010101000102");
  bytes.insert(bytes.end(), heartbeat.begin(), heartbeat.end());

  halo::Decoder decoder{halo::DecoderMode::StrictV01};
  const auto decoded = decoder.push(bytes.data(), bytes.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(
      static_cast<uint8_t>(halo::ProtocolError::PayloadTooLarge),
      static_cast<uint8_t>(decoded[0].error));
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::MessageType::Hello),
                          decoded[0].context.rawMessageType);
  TEST_ASSERT_EQUAL_UINT16(1, decoded[0].context.sequence);
  TEST_ASSERT_EQUAL_UINT16(513, decoded[0].context.declaredPayloadLength);
  TEST_ASSERT_TRUE(decoded[0].context.respondable);
  assertFrame(decoded[1], halo::MessageType::Heartbeat, 2, {});
  TEST_ASSERT_LESS_OR_EQUAL_UINT32(halo::kMaxPayload + 10,
                                   decoder.bufferedSize());
}

void test_unknown_message_type_is_structured_and_recoverable() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  auto bytes = fromHex("434801ff03000000");
  bytes.insert(bytes.end(), heartbeat.begin(), heartbeat.end());

  halo::Decoder decoder{halo::DecoderMode::StrictV01};
  const auto decoded = decoder.push(bytes.data(), bytes.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(
      static_cast<uint8_t>(halo::ProtocolError::UnknownMessageType),
      static_cast<uint8_t>(decoded[0].error));
  TEST_ASSERT_EQUAL_UINT8(0xff, decoded[0].context.rawMessageType);
  TEST_ASSERT_EQUAL_UINT16(3, decoded[0].context.sequence);
  TEST_ASSERT_TRUE(decoded[0].context.respondable);
  assertFrame(decoded[1], halo::MessageType::Heartbeat, 2, {});
}

void test_unknown_version_is_structured_and_recoverable() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  auto bytes = fromHex("4348022002000000");
  bytes.insert(bytes.end(), heartbeat.begin(), heartbeat.end());

  halo::Decoder decoder;
  const auto decoded = decoder.push(bytes.data(), bytes.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(
      static_cast<uint8_t>(halo::ProtocolError::UnsupportedVersion),
      static_cast<uint8_t>(decoded[0].error));
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::MessageType::Heartbeat),
                          decoded[0].context.rawMessageType);
  TEST_ASSERT_EQUAL_UINT16(2, decoded[0].context.sequence);
  TEST_ASSERT_EQUAL_UINT8(2, decoded[0].context.protocolMajor);
  TEST_ASSERT_FALSE(decoded[0].context.respondable);
  assertFrame(decoded[1], halo::MessageType::Heartbeat, 2, {});
}

void test_consecutive_frames_decode_in_order() {
  const auto vectors = loadGoldenVectors();
  auto bytes = vectors.at("hello").frame;
  const auto brightness = vectors.at("brightness_80").frame;
  bytes.insert(bytes.end(), brightness.begin(), brightness.end());

  halo::Decoder decoder;
  const auto decoded = decoder.push(bytes.data(), bytes.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  assertFrame(decoded[0], halo::MessageType::Hello, 1, {0});
  assertFrame(decoded[1], halo::MessageType::Brightness, 0x1234, {0x50});
}

void test_leading_noise_and_overlapping_magic_resynchronize() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  std::vector<uint8_t> bytes{0xff, 0x00, 0x43, 0x43};
  bytes.insert(bytes.end(), heartbeat.begin() + 1, heartbeat.end());

  halo::Decoder decoder;
  const auto decoded = decoder.push(bytes.data(), bytes.size());
  TEST_ASSERT_EQUAL_UINT32(1, decoded.size());
  assertFrame(decoded[0], halo::MessageType::Heartbeat, 2, {});
}

void test_maximum_payload_round_trips() {
  std::vector<uint8_t> payload(halo::kMaxPayload);
  for (size_t index = 0; index < payload.size(); ++index) {
    payload[index] = static_cast<uint8_t>(index % 251);
  }
  const halo::Frame frame{halo::MessageType::Diagnostics, 0xffff, payload};
  const auto encoded = halo::encode(frame);
  TEST_ASSERT_EQUAL_UINT32(halo::kMaxPayload + 10, encoded.size());
  TEST_ASSERT_EQUAL_UINT32(halo::kMaxPayload + 10,
                           halo::Decoder::bufferCapacity());

  halo::Decoder decoder;
  std::vector<halo::DecodeResult> decoded;
  for (const uint8_t byte : encoded) {
    const auto next = decoder.push(&byte, 1);
    decoded.insert(decoded.end(), next.begin(), next.end());
    TEST_ASSERT_LESS_OR_EQUAL_UINT32(halo::Decoder::bufferCapacity(),
                                     decoder.bufferedSize());
  }
  TEST_ASSERT_EQUAL_UINT32(1, decoded.size());
  assertFrame(decoded[0], frame.type, frame.sequence, frame.payload);
}

void test_oversized_encode_returns_no_frame() {
  const halo::Frame frame{halo::MessageType::Diagnostics, 7,
                          std::vector<uint8_t>(halo::kMaxPayload + 1)};
  TEST_ASSERT_TRUE(halo::encode(frame).empty());
}

void test_false_max_length_magic_inside_bad_frame_does_not_stall_recovery() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  const auto falseHeader = fromHex("4348012000000002");
  halo::Frame bad{halo::MessageType::RingUpdate, 9, falseHeader};
  auto bytes = halo::encode(bad);
  bytes.back() ^= 0xff;
  bytes.insert(bytes.end(), heartbeat.begin(), heartbeat.end());

  halo::Decoder decoder{halo::DecoderMode::StrictV01};
  const auto decoded = decoder.push(bytes.data(), bytes.size());
  TEST_ASSERT_EQUAL_UINT32(3, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::ProtocolError::CrcMismatch),
                          static_cast<uint8_t>(decoded[0].error));
  TEST_ASSERT_EQUAL_UINT8(
      static_cast<uint8_t>(halo::ProtocolError::InvalidPayloadLength),
      static_cast<uint8_t>(decoded[1].error));
  assertFrame(decoded[2], halo::MessageType::Heartbeat, 2, {});
}

void test_crc_error_resynchronizes_to_magic_inside_bad_frame() {
  const auto fixture = loadDecoderStreamVectors().at("crc_nested_valid");
  TEST_ASSERT_EQUAL_STRING("generic", fixture.mode.c_str());
  TEST_ASSERT_EQUAL_STRING("crc_mismatch", fixture.expectedError.c_str());

  halo::Decoder decoder;
  const auto decoded = decoder.push(fixture.stream.data(), fixture.stream.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::ProtocolError::CrcMismatch),
                          static_cast<uint8_t>(decoded[0].error));
  assertFrame(decoded[1], messageType(fixture.recoveredMessageType),
              fixture.recoveredSequence, fixture.recoveredPayload);
}

void test_incomplete_max_length_candidate_recovers_to_complete_nested_frame() {
  const auto fixture =
      loadDecoderStreamVectors().at("strict_invalid_length_nested");
  TEST_ASSERT_EQUAL_STRING("strict_v01", fixture.mode.c_str());
  TEST_ASSERT_EQUAL_STRING("invalid_payload_length",
                           fixture.expectedError.c_str());

  halo::Decoder decoder{halo::DecoderMode::StrictV01};
  const auto decoded = decoder.push(fixture.stream.data(), fixture.stream.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(
      static_cast<uint8_t>(halo::ProtocolError::InvalidPayloadLength),
      static_cast<uint8_t>(decoded[0].error));
  TEST_ASSERT_EQUAL_UINT8(
      static_cast<uint8_t>(halo::MessageType::Capabilities),
      decoded[0].context.rawMessageType);
  TEST_ASSERT_EQUAL_UINT16(9, decoded[0].context.sequence);
  TEST_ASSERT_EQUAL_UINT16(halo::kMaxPayload,
                           decoded[0].context.declaredPayloadLength);
  TEST_ASSERT_TRUE(decoded[0].context.respondable);
  assertFrame(decoded[1], messageType(fixture.recoveredMessageType),
              fixture.recoveredSequence, fixture.recoveredPayload);
}

void test_generic_decoder_retains_max_frame_with_nested_valid_frame_bytes() {
  const auto heartbeat = loadGoldenVectors().at("heartbeat").frame;
  std::vector<uint8_t> payload(halo::kMaxPayload, 0xa5);
  std::copy(heartbeat.begin(), heartbeat.end(), payload.begin());
  const halo::Frame frame{halo::MessageType::Diagnostics, 24, payload};
  const auto encoded = halo::encode(frame);
  const size_t prefixLength = 8 + heartbeat.size();

  halo::Decoder decoder;
  TEST_ASSERT_TRUE(decoder.push(encoded.data(), prefixLength).empty());
  TEST_ASSERT_EQUAL_UINT32(prefixLength, decoder.bufferedSize());

  const auto decoded = decoder.push(encoded.data() + prefixLength,
                                     encoded.size() - prefixLength);
  TEST_ASSERT_EQUAL_UINT32(1, decoded.size());
  assertFrame(decoded[0], frame.type, frame.sequence, frame.payload);
}

void test_crc_recovery_keeps_two_nested_frames_and_normalizes_outer_tail() {
  const auto fixture =
      loadDecoderStreamVectors().at("crc_two_nested_valid");
  TEST_ASSERT_EQUAL_STRING("generic", fixture.mode.c_str());
  TEST_ASSERT_EQUAL_STRING("crc_mismatch", fixture.expectedError.c_str());
  TEST_ASSERT_EQUAL_STRING("4348011334120100502983",
                           toHex(fixture.additionalRecoveredFrame).c_str());

  halo::Decoder decoder;
  const auto decoded = decoder.push(fixture.stream.data(), fixture.stream.size());
  TEST_ASSERT_EQUAL_UINT32(3, decoded.size());
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::ProtocolError::CrcMismatch),
                          static_cast<uint8_t>(decoded[0].error));
  assertFrame(decoded[1], halo::MessageType::Heartbeat, 2, {});
  assertFrame(decoded[2], halo::MessageType::Brightness, 0x1234, {0x50});
  const size_t bufferedBeforeFollowup = decoder.bufferedSize();

  const auto followup =
      decoder.push(fixture.followupStream.data(), fixture.followupStream.size());
  TEST_ASSERT_EQUAL_UINT32(1, followup.size());
  assertFrame(followup[0], halo::MessageType::Hello, 1, {0});
  TEST_ASSERT_EQUAL_STRING(toHex(fixture.followupFrame).c_str(),
                           toHex(halo::encode(followup[0].frame)).c_str());
  TEST_ASSERT_EQUAL_UINT32(fixture.bufferedTail.size(),
                           bufferedBeforeFollowup);
  TEST_ASSERT_EQUAL_UINT32(0, decoder.bufferedSize());
}

}  // namespace

int main(int, char**) {
  UNITY_BEGIN();
  RUN_TEST(test_protocol_constants_and_all_message_types_are_stable);
  RUN_TEST(test_hello_matches_shared_golden_vector);
  RUN_TEST(test_all_shared_golden_vectors_encode_and_decode);
  RUN_TEST(test_crc_matches_ccitt_false_reference);
  RUN_TEST(test_fragmented_frame_is_retained_until_complete);
  RUN_TEST(test_minimum_frame_is_ten_bytes_and_decodes_one_byte_at_a_time);
  RUN_TEST(test_crc_error_is_reported_and_following_frame_decodes);
  RUN_TEST(test_oversized_length_is_rejected_without_buffer_growth);
  RUN_TEST(test_unknown_message_type_is_structured_and_recoverable);
  RUN_TEST(test_unknown_version_is_structured_and_recoverable);
  RUN_TEST(test_consecutive_frames_decode_in_order);
  RUN_TEST(test_leading_noise_and_overlapping_magic_resynchronize);
  RUN_TEST(test_maximum_payload_round_trips);
  RUN_TEST(test_oversized_encode_returns_no_frame);
  RUN_TEST(test_false_max_length_magic_inside_bad_frame_does_not_stall_recovery);
  RUN_TEST(test_crc_error_resynchronizes_to_magic_inside_bad_frame);
  RUN_TEST(test_incomplete_max_length_candidate_recovers_to_complete_nested_frame);
  RUN_TEST(test_generic_decoder_retains_max_frame_with_nested_valid_frame_bytes);
  RUN_TEST(test_crc_recovery_keeps_two_nested_frames_and_normalizes_outer_tail);
  return UNITY_END();
}
