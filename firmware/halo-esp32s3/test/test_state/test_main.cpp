#include <HaloState.hpp>
#include <unity.h>

#include <cstdint>
#include <vector>

namespace {

using halo::MessageType;
using halo::NackReason;

std::vector<uint8_t> snapshotPayload(uint64_t revision = 42) {
  std::vector<uint8_t> payload;
  for (uint8_t index = 0; index < 8; ++index) {
    payload.push_back(static_cast<uint8_t>(revision >> (index * 8)));
  }

  payload.insert(payload.end(), {
                                    73, 2, 2, 4,
                                    0, 1, 100, 100, 0, 0, 25, 0,
                                    1, 2, 80, 125, 0, 1, 50, 0,
                                    2, 3, 60, 200, 0, 0, 75, 0,
                                    3, 6, 40, 44, 1, 1, 100, 0,
                                });
  return payload;
}

halo::Frame snapshotFrame(uint64_t revision = 42, uint16_t sequence = 7) {
  return {MessageType::FullSnapshot, sequence, snapshotPayload(revision)};
}

void assertResponse(const halo::ControllerResponse& response,
                    MessageType type, uint16_t sequence) {
  TEST_ASSERT_TRUE(response.shouldSend);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(type),
                          static_cast<uint8_t>(response.type));
  TEST_ASSERT_EQUAL_UINT16(sequence, response.sequence);
}

void establishSnapshot(halo::DeviceController& controller) {
  const auto response = controller.handle(snapshotFrame(), 100);
  assertResponse(response, MessageType::Ack, 7);
}

void test_full_snapshot_becomes_authoritative_state() {
  halo::DeviceController controller;
  const auto response = controller.handle(snapshotFrame(), 100);

  assertResponse(response, MessageType::Ack, 7);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(MessageType::FullSnapshot),
                          response.payload[0]);
  TEST_ASSERT_EQUAL_UINT64(42, controller.state().revision);
  TEST_ASSERT_EQUAL_UINT8(4, controller.state().ringCount);
  TEST_ASSERT_TRUE(controller.state().authoritative);
  TEST_ASSERT_FALSE(controller.state().disconnected);
  TEST_ASSERT_EQUAL_UINT8(3, controller.state().rings[3].index);
}

void test_full_snapshot_requires_exact_fixed_payload() {
  halo::DeviceController controller;
  auto frame = snapshotFrame();
  frame.payload.push_back(0);

  const auto response = controller.handle(frame, 100);

  assertResponse(response, MessageType::Nack, 7);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          response.payload[1]);
  TEST_ASSERT_FALSE(controller.state().authoritative);
}

void test_full_snapshot_rejects_invalid_ring_without_partial_write() {
  halo::DeviceController controller;
  auto frame = snapshotFrame();
  frame.payload[12 + 3] = 24;

  const auto response = controller.handle(frame, 100);

  assertResponse(response, MessageType::Nack, 7);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          response.payload[1]);
  TEST_ASSERT_FALSE(controller.state().authoritative);
}

void test_full_snapshot_overwrites_state_even_when_revision_is_lower() {
  halo::DeviceController controller;
  establishSnapshot(controller);

  auto older = snapshotFrame(41, 8);
  older.payload[8] = 10;
  const auto response = controller.handle(older, 200);

  assertResponse(response, MessageType::Ack, 8);
  TEST_ASSERT_EQUAL_UINT64(41, controller.state().revision);
  TEST_ASSERT_EQUAL_UINT8(10, controller.state().globalBrightness);
}

void test_incremental_write_requires_authoritative_snapshot() {
  halo::DeviceController controller;
  const halo::Frame frame{MessageType::Brightness, 9, {50}};

  const auto response = controller.handle(frame, 100);

  assertResponse(response, MessageType::Nack, 9);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::InvalidState),
                          response.payload[1]);
}

void test_ring_update_applies_complete_valid_record() {
  halo::DeviceController controller;
  establishSnapshot(controller);
  const halo::Frame frame{MessageType::RingUpdate, 10,
                          {2, 4, 55, 250, 0, 1, 30, 0}};

  const auto response = controller.handle(frame, 200);

  assertResponse(response, MessageType::Ack, 10);
  TEST_ASSERT_EQUAL_UINT8(55, controller.state().rings[2].brightness);
  TEST_ASSERT_EQUAL_UINT16(250, controller.state().rings[2].speedPercent);
}

void test_ring_update_rejects_invalid_status() {
  halo::DeviceController controller;
  establishSnapshot(controller);
  const halo::Frame frame{MessageType::RingUpdate, 11,
                          {1, 0, 55, 100, 0, 0, 30, 0}};

  const auto response = controller.handle(frame, 200);

  assertResponse(response, MessageType::Nack, 11);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          response.payload[1]);
  TEST_ASSERT_EQUAL_UINT8(2, static_cast<uint8_t>(controller.state().rings[1].status));
}

void test_display_and_brightness_updates_apply_and_ack() {
  halo::DeviceController controller;
  establishSnapshot(controller);

  const auto display = controller.handle(
      {MessageType::DisplayMode, 12, {1, 0xff}}, 200);
  const auto brightness = controller.handle(
      {MessageType::Brightness, 13, {100}}, 201);

  assertResponse(display, MessageType::Ack, 12);
  assertResponse(brightness, MessageType::Ack, 13);
  TEST_ASSERT_EQUAL_UINT8(1, static_cast<uint8_t>(controller.state().displayMode));
  TEST_ASSERT_EQUAL_UINT8(0xff, controller.state().selectedRing);
  TEST_ASSERT_EQUAL_UINT8(100, controller.state().globalBrightness);
  TEST_ASSERT_LESS_OR_EQUAL_UINT8(30, controller.state().effectiveBrightness);
}

void test_invalid_display_or_brightness_is_nacked() {
  halo::DeviceController controller;
  establishSnapshot(controller);

  const auto display = controller.handle(
      {MessageType::DisplayMode, 14, {3, 0}}, 200);
  const auto brightness = controller.handle(
      {MessageType::Brightness, 15, {101}}, 201);

  assertResponse(display, MessageType::Nack, 14);
  assertResponse(brightness, MessageType::Nack, 15);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          display.payload[1]);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          brightness.payload[1]);
}

void test_hello_returns_fixed_capabilities() {
  halo::DeviceController controller;
  const auto response = controller.handle({MessageType::Hello, 16, {0}}, 100);

  assertResponse(response, MessageType::Capabilities, 16);
  const std::vector<uint8_t> expected{0, 0, 1, 0, 4, 3, 0, 0, 2};
  TEST_ASSERT_EQUAL_UINT32(expected.size(), response.payload.size());
  TEST_ASSERT_EQUAL_UINT8_ARRAY(expected.data(), response.payload.data(),
                                expected.size());
}

void test_heartbeat_requires_empty_payload_and_has_no_response() {
  halo::DeviceController controller;
  establishSnapshot(controller);

  const auto valid = controller.handle({MessageType::Heartbeat, 17, {}}, 2000);
  const auto malformed = controller.handle({MessageType::Heartbeat, 18, {0}}, 2001);

  TEST_ASSERT_FALSE(valid.shouldSend);
  assertResponse(malformed, MessageType::Nack, 18);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          malformed.payload[1]);
  controller.tick(5000);
  TEST_ASSERT_FALSE(controller.state().disconnected);
  controller.tick(5001);
  TEST_ASSERT_TRUE(controller.state().disconnected);
}

void test_device_to_host_message_is_unsupported() {
  halo::DeviceController controller;
  const auto response = controller.handle(
      {MessageType::Capabilities, 19, {}}, 100);

  assertResponse(response, MessageType::Nack, 19);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::UnsupportedMessage),
                          response.payload[1]);
}

void test_decoder_error_is_nacked_and_version_error_enters_safe_state() {
  halo::DeviceController controller;
  establishSnapshot(controller);
  halo::DecodeResult error;
  error.error = halo::ProtocolError::UnsupportedVersion;
  error.context.rawMessageType = static_cast<uint8_t>(MessageType::FullSnapshot);
  error.context.sequence = 20;
  error.context.respondable = false;

  const auto response = controller.handleProtocolError(error);

  TEST_ASSERT_FALSE(response.shouldSend);
  TEST_ASSERT_TRUE(response.stateChanged);
  TEST_ASSERT_TRUE(controller.state().disconnected);
  TEST_ASSERT_FALSE(controller.state().authoritative);
  TEST_ASSERT_LESS_OR_EQUAL_UINT8(16, controller.state().effectiveBrightness);

  const auto blocked = controller.handle(
      {MessageType::Brightness, 21, {20}}, 200);
  assertResponse(blocked, MessageType::Nack, 21);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::InvalidState),
                          blocked.payload[1]);
}

void test_respondable_decoder_error_returns_nack() {
  halo::DeviceController controller;
  halo::DecodeResult error;
  error.error = halo::ProtocolError::UnknownMessageType;
  error.context.rawMessageType = 0xfe;
  error.context.sequence = 22;
  error.context.respondable = true;

  const auto response = controller.handleProtocolError(error);

  assertResponse(response, MessageType::Nack, 22);
  TEST_ASSERT_EQUAL_UINT8(0xfe, response.payload[0]);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::UnsupportedMessage),
                          response.payload[1]);
}

void test_watchdog_enters_low_brightness_disconnected_state() {
  halo::DeviceController controller;
  controller.handle(snapshotFrame(), 0);
  controller.tick(3001);

  TEST_ASSERT_TRUE(controller.state().disconnected);
  TEST_ASSERT_LESS_OR_EQUAL_UINT8(16, controller.state().effectiveBrightness);
}

void test_local_current_limit_caps_effective_brightness() {
  halo::PowerPolicy policy{500, 80};
  TEST_ASSERT_LESS_OR_EQUAL_UINT8(80, policy.limitBrightness(100, 20));
  TEST_ASSERT_EQUAL_UINT8(0, policy.limitBrightness(100, 0));
}

}  // namespace

void setUp() {}

void tearDown() {}

int main(int, char**) {
  UNITY_BEGIN();
  RUN_TEST(test_full_snapshot_becomes_authoritative_state);
  RUN_TEST(test_full_snapshot_requires_exact_fixed_payload);
  RUN_TEST(test_full_snapshot_rejects_invalid_ring_without_partial_write);
  RUN_TEST(test_full_snapshot_overwrites_state_even_when_revision_is_lower);
  RUN_TEST(test_incremental_write_requires_authoritative_snapshot);
  RUN_TEST(test_ring_update_applies_complete_valid_record);
  RUN_TEST(test_ring_update_rejects_invalid_status);
  RUN_TEST(test_display_and_brightness_updates_apply_and_ack);
  RUN_TEST(test_invalid_display_or_brightness_is_nacked);
  RUN_TEST(test_hello_returns_fixed_capabilities);
  RUN_TEST(test_heartbeat_requires_empty_payload_and_has_no_response);
  RUN_TEST(test_device_to_host_message_is_unsupported);
  RUN_TEST(test_decoder_error_is_nacked_and_version_error_enters_safe_state);
  RUN_TEST(test_respondable_decoder_error_returns_nack);
  RUN_TEST(test_watchdog_enters_low_brightness_disconnected_state);
  RUN_TEST(test_local_current_limit_caps_effective_brightness);
  return UNITY_END();
}
