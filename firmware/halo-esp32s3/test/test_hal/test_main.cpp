#include <HaloHal.hpp>
#include <unity.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <vector>

namespace {

class FakeWriter final : public halo::ByteWriter {
 public:
  int availableForWrite() override { return available; }

  size_t write(const uint8_t* bytes, size_t length) override {
    ++writeCalls;
    const size_t written = std::min(length, maxWrite);
    output.insert(output.end(), bytes, bytes + written);
    return written;
  }

  int available{0};
  size_t maxWrite{std::numeric_limits<size_t>::max()};
  int writeCalls{0};
  std::vector<uint8_t> output;
};

halo::Frame ack(uint16_t sequence) {
  return {halo::MessageType::Ack, sequence,
          {static_cast<uint8_t>(halo::MessageType::FullSnapshot)}};
}

void test_backpressure_keeps_the_complete_frame_queued() {
  halo::TxPump pump;
  FakeWriter writer;
  const auto expected = halo::encode(ack(1));
  TEST_ASSERT_TRUE(pump.enqueue(ack(1)));

  writer.available = static_cast<int>(expected.size() - 1);
  pump.pump(writer);
  TEST_ASSERT_EQUAL(0, writer.writeCalls);
  TEST_ASSERT_FALSE(pump.empty());

  writer.available = static_cast<int>(expected.size());
  pump.pump(writer);
  TEST_ASSERT_EQUAL_UINT8_ARRAY(expected.data(), writer.output.data(),
                                expected.size());
  TEST_ASSERT_TRUE(pump.empty());
}

void test_short_write_retries_only_the_unsent_suffix() {
  halo::TxPump pump;
  FakeWriter writer;
  const auto expected = halo::encode(ack(2));
  TEST_ASSERT_TRUE(pump.enqueue(ack(2)));

  writer.available = static_cast<int>(expected.size());
  writer.maxWrite = 4;
  pump.pump(writer);
  TEST_ASSERT_FALSE(pump.empty());

  writer.maxWrite = expected.size();
  pump.pump(writer);
  TEST_ASSERT_EQUAL_UINT8_ARRAY(expected.data(), writer.output.data(),
                                expected.size());
  TEST_ASSERT_TRUE(pump.empty());
}

void test_multiple_frames_preserve_order_after_backpressure() {
  halo::TxPump pump;
  FakeWriter writer;
  const auto first = halo::encode(ack(3));
  const auto second = halo::encode(ack(4));
  std::vector<uint8_t> expected = first;
  expected.insert(expected.end(), second.begin(), second.end());
  TEST_ASSERT_TRUE(pump.enqueue(ack(3)));
  TEST_ASSERT_TRUE(pump.enqueue(ack(4)));

  writer.available = 0;
  pump.pump(writer);
  writer.available = static_cast<int>(first.size());
  pump.pump(writer);
  writer.available = static_cast<int>(second.size());
  pump.pump(writer);

  TEST_ASSERT_EQUAL_UINT8_ARRAY(expected.data(), writer.output.data(),
                                expected.size());
  TEST_ASSERT_TRUE(pump.empty());
}

void test_full_queue_rejects_a_whole_new_frame() {
  halo::TxPump pump;
  for (size_t index = 0; index < halo::TxPump::capacity(); ++index) {
    TEST_ASSERT_TRUE(pump.enqueue(ack(static_cast<uint16_t>(index))));
  }
  TEST_ASSERT_TRUE(pump.full());
  TEST_ASSERT_FALSE(pump.enqueue(ack(99)));
}

void test_device_event_sequence_advances_only_after_enqueue_and_orders_diagnostic_before_knob() {
  halo::TxPump pump;
  halo::DeviceEventSender sender;
  for (size_t index = 0; index < halo::TxPump::capacity(); ++index) {
    TEST_ASSERT_TRUE(pump.enqueue(ack(static_cast<uint16_t>(index))));
  }
  const halo::Diagnostic diagnostic{halo::DiagnosticSeverity::Warning,
                                    halo::DiagnosticCode::CrcError, 2};

  TEST_ASSERT_FALSE(sender.enqueue(pump, halo::MessageType::Diagnostics,
                                   diagnostic.encodePayload()));
  TEST_ASSERT_EQUAL_UINT16(0, sender.nextSequence());

  FakeWriter writer;
  writer.available = 64;
  while (!pump.empty()) {
    pump.pump(writer);
  }
  writer.output.clear();

  TEST_ASSERT_TRUE(sender.enqueue(pump, halo::MessageType::Diagnostics,
                                  diagnostic.encodePayload()));
  TEST_ASSERT_TRUE(sender.enqueue(
      pump, halo::MessageType::KnobEvent,
      {static_cast<uint8_t>(halo::KnobAction::ShortPress), 0}));
  TEST_ASSERT_EQUAL_UINT16(2, sender.nextSequence());
  while (!pump.empty()) {
    pump.pump(writer);
  }

  halo::Decoder decoder{halo::DecoderMode::StrictV01};
  const auto decoded = decoder.push(writer.output.data(), writer.output.size());
  TEST_ASSERT_EQUAL_UINT32(2, decoded.size());
  TEST_ASSERT_TRUE(decoded[0].ok());
  TEST_ASSERT_TRUE(decoded[1].ok());
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::MessageType::Diagnostics),
                          static_cast<uint8_t>(decoded[0].frame.type));
  TEST_ASSERT_EQUAL_UINT16(0, decoded[0].frame.sequence);
  TEST_ASSERT_EQUAL_UINT8(static_cast<uint8_t>(halo::MessageType::KnobEvent),
                          static_cast<uint8_t>(decoded[1].frame.type));
  TEST_ASSERT_EQUAL_UINT16(1, decoded[1].frame.sequence);
}

void test_full_tx_queue_keeps_controller_diagnostic_pending_until_successful_enqueue() {
  halo::TxPump pump;
  halo::DeviceEventSender sender;
  halo::DeviceController controller;
  halo::DecodeResult crc;
  crc.error = halo::ProtocolError::CrcMismatch;
  controller.handleProtocolError(crc);
  for (size_t index = 0; index < halo::TxPump::capacity(); ++index) {
    TEST_ASSERT_TRUE(pump.enqueue(ack(static_cast<uint16_t>(index))));
  }

  TEST_ASSERT_FALSE(sender.enqueuePendingDiagnostic(pump, controller));
  TEST_ASSERT_TRUE(controller.pendingDiagnostic().has_value());
  TEST_ASSERT_EQUAL_UINT16(0, sender.nextSequence());

  FakeWriter writer;
  writer.available = 64;
  pump.pump(writer);
  TEST_ASSERT_TRUE(sender.enqueuePendingDiagnostic(pump, controller));
  TEST_ASSERT_FALSE(controller.pendingDiagnostic().has_value());
  TEST_ASSERT_EQUAL_UINT16(1, sender.nextSequence());
}

}  // namespace

void setUp() {}

void tearDown() {}

int main(int, char**) {
  UNITY_BEGIN();
  RUN_TEST(test_backpressure_keeps_the_complete_frame_queued);
  RUN_TEST(test_short_write_retries_only_the_unsent_suffix);
  RUN_TEST(test_multiple_frames_preserve_order_after_backpressure);
  RUN_TEST(test_full_queue_rejects_a_whole_new_frame);
  RUN_TEST(test_device_event_sequence_advances_only_after_enqueue_and_orders_diagnostic_before_knob);
  RUN_TEST(test_full_tx_queue_keeps_controller_diagnostic_pending_until_successful_enqueue);
  return UNITY_END();
}
