#include <HaloState.hpp>
#include <unity.h>

#include <cstdint>
#include <iterator>
#include <vector>

namespace {

using halo::MessageType;
using halo::NackReason;

void assertDiagnostic(const halo::Diagnostic& diagnostic,
                      halo::DiagnosticSeverity severity,
                      halo::DiagnosticCode code, uint32_t value) {
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(severity),
                          static_cast<uint8_t>(diagnostic.severity));
  TEST_ASSERT_EQUAL_UINT16(static_cast<uint16_t>(code),
                           static_cast<uint16_t>(diagnostic.code));
  TEST_ASSERT_EQUAL_UINT32(value, diagnostic.value);
}

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

void test_diagnostic_payload_is_exact_little_endian_and_semantically_validated() {
  const halo::Diagnostic expected{halo::DiagnosticSeverity::Warning,
                                  halo::DiagnosticCode::CrcError, 0x78563412};
  const std::vector<uint8_t> payload{2, 2, 0, 0x12, 0x34, 0x56, 0x78};
  const auto encoded = expected.encodePayload();
  TEST_ASSERT_EQUAL_UINT8_ARRAY(payload.data(), encoded.data(), payload.size());
  const auto decoded = halo::Diagnostic::decodePayload(payload);
  TEST_ASSERT_TRUE(decoded.has_value());
  assertDiagnostic(*decoded, expected.severity, expected.code, expected.value);

  for (const auto& invalid : std::vector<std::vector<uint8_t>>{
           {1, 1, 0, 0, 0, 0},
           {1, 1, 0, 0, 0, 0, 0, 0},
           {0, 1, 0, 0, 0, 0, 0},
           {4, 1, 0, 0, 0, 0, 0},
           {1, 0, 0, 0, 0, 0, 0},
           {1, 5, 0, 0, 0, 0, 0},
       }) {
    TEST_ASSERT_FALSE(halo::Diagnostic::decodePayload(invalid).has_value());
  }
}

void test_valid_desktop_diagnostic_is_silent_and_does_not_refresh_watchdog() {
  halo::DeviceController controller;
  establishSnapshot(controller);
  while (controller.pendingDiagnostic().has_value()) {
    controller.popPendingDiagnostic();
  }
  const auto before = controller.state();
  const halo::Diagnostic diagnostic{halo::DiagnosticSeverity::Info,
                                    halo::DiagnosticCode::LocalLimit, 30};

  const auto response = controller.handle(
      {MessageType::Diagnostics, 30, diagnostic.encodePayload()}, 3000);

  TEST_ASSERT_FALSE(response.shouldSend);
  TEST_ASSERT_FALSE(response.stateChanged);
  TEST_ASSERT_EQUAL_UINT64(before.revision, controller.state().revision);
  TEST_ASSERT_EQUAL_UINT8(before.globalBrightness,
                          controller.state().globalBrightness);
  controller.tick(3101);
  TEST_ASSERT_TRUE(controller.state().disconnected);
}

void test_invalid_desktop_diagnostic_is_nacked_and_queues_malformed_report() {
  halo::DeviceController controller;
  const auto response = controller.handle(
      {MessageType::Diagnostics, 31, {2, 5, 0, 0, 0, 0, 0}}, 100);

  assertResponse(response, MessageType::Nack, 31);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          response.payload[1]);
  TEST_ASSERT_FALSE(controller.state().authoritative);
  const auto pending = controller.pendingDiagnostic();
  TEST_ASSERT_TRUE(pending.has_value());
  assertDiagnostic(*pending, halo::DiagnosticSeverity::Warning,
                   halo::DiagnosticCode::InvalidPayload, 1);
}

void test_incompatible_state_consumes_valid_diagnostic_and_rejects_invalid() {
  halo::DeviceController controller;
  establishSnapshot(controller);
  halo::DecodeResult versionError;
  versionError.error = halo::ProtocolError::UnsupportedVersion;
  versionError.context.rawMessageType =
      static_cast<uint8_t>(MessageType::FullSnapshot);
  versionError.context.sequence = 32;
  versionError.context.respondable = false;
  controller.handleProtocolError(versionError);

  const auto safeState = controller.state();
  const auto pendingBefore = controller.pendingDiagnostic();
  TEST_ASSERT_TRUE(safeState.disconnected);
  TEST_ASSERT_FALSE(safeState.authoritative);
  TEST_ASSERT_TRUE(pendingBefore.has_value());
  assertDiagnostic(*pendingBefore, halo::DiagnosticSeverity::Info,
                   halo::DiagnosticCode::LocalLimit, 30);

  const halo::Diagnostic valid{halo::DiagnosticSeverity::Warning,
                               halo::DiagnosticCode::CrcError, 7};
  const auto silent = controller.handle(
      {MessageType::Diagnostics, 33, valid.encodePayload()}, 3000);

  TEST_ASSERT_FALSE(silent.shouldSend);
  TEST_ASSERT_FALSE(silent.stateChanged);
  TEST_ASSERT_EQUAL_UINT64(safeState.revision, controller.state().revision);
  TEST_ASSERT_EQUAL_UINT8(safeState.globalBrightness,
                          controller.state().globalBrightness);
  TEST_ASSERT_EQUAL_UINT8(safeState.effectiveBrightness,
                          controller.state().effectiveBrightness);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(safeState.displayMode),
                          static_cast<uint8_t>(controller.state().displayMode));
  TEST_ASSERT_EQUAL_UINT8(safeState.selectedRing,
                          controller.state().selectedRing);
  TEST_ASSERT_EQUAL_UINT8(safeState.authoritative,
                          controller.state().authoritative);
  TEST_ASSERT_EQUAL_UINT8(safeState.disconnected,
                          controller.state().disconnected);
  TEST_ASSERT_EQUAL_UINT32(1, controller.pendingDiagnosticCount());
  const auto pendingAfterValid = controller.pendingDiagnostic();
  TEST_ASSERT_TRUE(pendingAfterValid.has_value());
  assertDiagnostic(*pendingAfterValid, halo::DiagnosticSeverity::Info,
                   halo::DiagnosticCode::LocalLimit, 30);

  const auto stillIncompatibleAfterValid =
      controller.handle({MessageType::Heartbeat, 34, {}}, 3050);
  assertResponse(stillIncompatibleAfterValid, MessageType::Nack, 34);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::InvalidState),
                          stillIncompatibleAfterValid.payload[1]);

  const auto invalid = controller.handle(
      {MessageType::Diagnostics, 35, {2, 5, 0, 0, 0, 0, 0}}, 3100);

  assertResponse(invalid, MessageType::Nack, 35);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::MalformedPayload),
                          invalid.payload[1]);
  TEST_ASSERT_EQUAL_UINT32(2, controller.pendingDiagnosticCount());
  TEST_ASSERT_EQUAL_UINT64(safeState.revision, controller.state().revision);
  TEST_ASSERT_EQUAL_UINT8(safeState.authoritative,
                          controller.state().authoritative);
  TEST_ASSERT_EQUAL_UINT8(safeState.disconnected,
                          controller.state().disconnected);

  const auto stillBlocked = controller.handle(
      {MessageType::Heartbeat, 36, {}}, 3200);
  assertResponse(stillBlocked, MessageType::Nack, 36);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(NackReason::InvalidState),
                          stillBlocked.payload[1]);
}

void test_protocol_errors_coalesce_bounded_crc_and_malformed_diagnostics() {
  halo::DeviceController controller;
  halo::DecodeResult crc;
  crc.error = halo::ProtocolError::CrcMismatch;
  controller.handleProtocolError(crc);
  controller.handleProtocolError(crc);

  TEST_ASSERT_EQUAL_UINT8(1, controller.pendingDiagnosticCount());
  assertDiagnostic(*controller.pendingDiagnostic(),
                   halo::DiagnosticSeverity::Warning,
                   halo::DiagnosticCode::CrcError, 2);
  controller.popPendingDiagnostic();

  halo::DecodeResult malformed;
  malformed.error = halo::ProtocolError::InvalidPayloadLength;
  malformed.context.rawMessageType =
      static_cast<uint8_t>(MessageType::Brightness);
  malformed.context.sequence = 32;
  malformed.context.respondable = true;
  const auto response = controller.handleProtocolError(malformed);

  assertResponse(response, MessageType::Nack, 32);
  assertDiagnostic(*controller.pendingDiagnostic(),
                   halo::DiagnosticSeverity::Warning,
                   halo::DiagnosticCode::InvalidPayload, 1);
  TEST_ASSERT_LESS_OR_EQUAL_UINT8(4, controller.pendingDiagnosticCount());
}

void test_watchdog_and_local_clamp_queue_safe_edge_diagnostics() {
  halo::DeviceController controller;
  establishSnapshot(controller);

  assertDiagnostic(*controller.pendingDiagnostic(), halo::DiagnosticSeverity::Info,
                   halo::DiagnosticCode::LocalLimit,
                   controller.state().effectiveBrightness);
  controller.popPendingDiagnostic();
  TEST_ASSERT_FALSE(controller.pendingDiagnostic().has_value());

  controller.tick(3101);
  TEST_ASSERT_TRUE(controller.state().disconnected);
  assertDiagnostic(*controller.pendingDiagnostic(),
                   halo::DiagnosticSeverity::Warning,
                   halo::DiagnosticCode::WatchdogDisconnected,
                   halo::kWatchdogTimeoutMs);
  controller.popPendingDiagnostic();
  controller.tick(6202);
  TEST_ASSERT_FALSE(controller.pendingDiagnostic().has_value());
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

void test_diagnostic_slot_index_maps_each_code_and_refuses_every_other_value() {
  constexpr halo::DiagnosticCode codes[] = {
      halo::DiagnosticCode::WatchdogDisconnected, halo::DiagnosticCode::CrcError,
      halo::DiagnosticCode::InvalidPayload, halo::DiagnosticCode::LocalLimit};
  static_assert(std::size(codes) == halo::kDiagnosticSlotCount,
                "every code that owns a slot must be listed here");

  for (size_t expected = 0; expected < std::size(codes); ++expected) {
    const auto index = halo::diagnosticSlotIndex(codes[expected]);
    TEST_ASSERT_TRUE(index.has_value());
    TEST_ASSERT_EQUAL_size_t(expected, *index);
  }

  // Sweep the whole representable domain rather than a sample, so no value
  // outside the slot range can be indexed however the enum later changes.
  // Zero is the dangerous one: subtracting one would underflow to SIZE_MAX.
  for (uint32_t raw = 0; raw <= 0xFFFF; ++raw) {
    const auto index =
        halo::diagnosticSlotIndex(static_cast<halo::DiagnosticCode>(raw));
    if (raw >= 1 && raw <= halo::kDiagnosticSlotCount) {
      TEST_ASSERT_TRUE(index.has_value());
      TEST_ASSERT_EQUAL_size_t(raw - 1, *index);
    } else {
      TEST_ASSERT_FALSE(index.has_value());
    }
  }
}

// The transmit pump reserves capacity from willRespondTo() before handling a
// frame. If the prediction says no frame is produced but handling emits one,
// the pump can already be full when the response is enqueued, and the caller
// drops out without consuming the frame — so the same frame is handled twice.
void assertPredictionMatchesHandling(halo::DeviceController& controller,
                                     const halo::Frame& frame,
                                     uint32_t nowMs) {
  halo::DecodeResult decoded;
  decoded.frame = frame;
  const bool predicted = controller.willRespondTo(decoded);
  const auto response = controller.handle(frame, nowMs);
  TEST_ASSERT_EQUAL_INT(static_cast<int>(response.shouldSend),
                        static_cast<int>(predicted));
}

void test_response_prediction_matches_handling_in_incompatible_state() {
  halo::DeviceController controller;
  establishSnapshot(controller);
  halo::DecodeResult versionError;
  versionError.error = halo::ProtocolError::UnsupportedVersion;
  versionError.context.rawMessageType =
      static_cast<uint8_t>(MessageType::FullSnapshot);
  versionError.context.sequence = 20;
  versionError.context.respondable = false;
  controller.handleProtocolError(versionError);

  // The incompatible-state gate NACKs everything except HELLO and DIAGNOSTICS,
  // and a heartbeat is neither.
  assertPredictionMatchesHandling(controller, {MessageType::Heartbeat, 21, {}},
                                  200);
}

void test_response_prediction_matches_handling_when_compatible() {
  halo::DeviceController controller;
  establishSnapshot(controller);

  assertPredictionMatchesHandling(controller, {MessageType::Heartbeat, 22, {}},
                                  200);
  assertPredictionMatchesHandling(controller, {MessageType::Brightness, 23, {40}},
                                  201);
  assertPredictionMatchesHandling(
      controller, {MessageType::Diagnostics, 24, {2, 2, 0, 7, 0, 0, 0}}, 202);
  assertPredictionMatchesHandling(
      controller, {MessageType::Diagnostics, 25, {9, 9, 9}}, 203);
}

void test_sentinel_diagnostic_code_is_refused_on_the_wire() {
  // The sentinel exists only to size the slot array. It must never be
  // accepted as a decoded wire code.
  std::vector<uint8_t> payload = {
      2, static_cast<uint8_t>(halo::DiagnosticCode::AfterLastCode), 0, 0, 0, 0,
      0};
  TEST_ASSERT_FALSE(halo::Diagnostic::decodePayload(payload).has_value());
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
  RUN_TEST(test_diagnostic_payload_is_exact_little_endian_and_semantically_validated);
  RUN_TEST(test_valid_desktop_diagnostic_is_silent_and_does_not_refresh_watchdog);
  RUN_TEST(test_invalid_desktop_diagnostic_is_nacked_and_queues_malformed_report);
  RUN_TEST(test_incompatible_state_consumes_valid_diagnostic_and_rejects_invalid);
  RUN_TEST(test_protocol_errors_coalesce_bounded_crc_and_malformed_diagnostics);
  RUN_TEST(test_watchdog_and_local_clamp_queue_safe_edge_diagnostics);
  RUN_TEST(test_device_to_host_message_is_unsupported);
  RUN_TEST(test_decoder_error_is_nacked_and_version_error_enters_safe_state);
  RUN_TEST(test_respondable_decoder_error_returns_nack);
  RUN_TEST(test_watchdog_enters_low_brightness_disconnected_state);
  RUN_TEST(test_local_current_limit_caps_effective_brightness);
  RUN_TEST(test_diagnostic_slot_index_maps_each_code_and_refuses_every_other_value);
  RUN_TEST(test_response_prediction_matches_handling_in_incompatible_state);
  RUN_TEST(test_response_prediction_matches_handling_when_compatible);
  RUN_TEST(test_sentinel_diagnostic_code_is_refused_on_the_wire);
  return UNITY_END();
}
