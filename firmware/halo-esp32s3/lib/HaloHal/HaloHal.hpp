#pragma once

#include <HaloState.hpp>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

namespace halo {

class RingRenderer {
 public:
  virtual ~RingRenderer() = default;
  virtual void apply(const DeviceState& state) = 0;
  virtual void tick(uint32_t nowMs) = 0;
};

class DisplayRenderer {
 public:
  virtual ~DisplayRenderer() = default;
  virtual void apply(const DeviceState& state) = 0;
};

class KnobInput {
 public:
  virtual ~KnobInput() = default;

  // Returns the next pending knob event, or nullopt when none is pending.
  //
  // The main loop skips polling on any iteration that still has a diagnostic
  // queued or a full transmit pump, so an implementation must latch every
  // event it observes and hold it until a poll consumes it. An implementation
  // that only samples the hardware at the instant of the call will silently
  // drop presses and rotations that happen between polls.
  virtual std::optional<KnobEvent> poll(uint32_t nowMs) = 0;
};

class NullRingRenderer final : public RingRenderer {
 public:
  void apply(const DeviceState&) override {}
  void tick(uint32_t) override {}
};

class NullDisplayRenderer final : public DisplayRenderer {
 public:
  void apply(const DeviceState&) override {}
};

class NullKnobInput final : public KnobInput {
 public:
  std::optional<KnobEvent> poll(uint32_t) override { return std::nullopt; }
};

class ByteWriter {
 public:
  virtual ~ByteWriter() = default;
  virtual int availableForWrite() = 0;
  virtual size_t write(const uint8_t* bytes, size_t length) = 0;
};

class TxPump {
 public:
  static constexpr size_t kCapacity = 8;
  static constexpr size_t kMaxFrameBytes = 19;

  static constexpr size_t capacity() { return kCapacity; }
  static constexpr size_t maxFrameBytes() { return kMaxFrameBytes; }

  bool enqueue(const Frame& frame) {
    const auto encoded = encode(frame);
    if (full() || encoded.empty() || encoded.size() > maxFrameBytes()) {
      return false;
    }

    PendingFrame& pending = frames_[(head_ + count_) % capacity()];
    std::copy(encoded.begin(), encoded.end(), pending.bytes.begin());
    pending.length = encoded.size();
    pending.offset = 0;
    ++count_;
    return true;
  }

  void pump(ByteWriter& writer) {
    if (empty()) {
      return;
    }

    PendingFrame& pending = frames_[head_];
    const size_t remaining = pending.length - pending.offset;
    if (writer.availableForWrite() < static_cast<int>(remaining)) {
      return;
    }

    const size_t written = std::min(
        remaining, writer.write(pending.bytes.data() + pending.offset, remaining));
    pending.offset += written;
    if (pending.offset == pending.length) {
      head_ = (head_ + 1) % capacity();
      --count_;
    }
  }

  bool empty() const { return count_ == 0; }
  bool full() const { return count_ == capacity(); }
  size_t size() const { return count_; }

 private:
  struct PendingFrame {
    std::array<uint8_t, kMaxFrameBytes> bytes{};
    size_t length{0};
    size_t offset{0};
  };

  std::array<PendingFrame, kCapacity> frames_{};
  size_t head_{0};
  size_t count_{0};
};

class DeviceEventSender {
 public:
  bool enqueue(TxPump& pump, MessageType type,
               const std::vector<uint8_t>& payload) {
    Frame frame{type, nextSequence_, payload};
    if (!pump.enqueue(frame)) {
      return false;
    }
    nextSequence_ = static_cast<uint16_t>(nextSequence_ + 1);
    return true;
  }

  bool enqueuePendingDiagnostic(TxPump& pump,
                                DeviceController& controller) {
    const auto diagnostic = controller.pendingDiagnostic();
    if (!diagnostic.has_value() ||
        !enqueue(pump, MessageType::Diagnostics,
                 diagnostic->encodePayload())) {
      return false;
    }
    controller.popPendingDiagnostic();
    return true;
  }

  uint16_t nextSequence() const { return nextSequence_; }

 private:
  uint16_t nextSequence_{0};
};

}  // namespace halo
