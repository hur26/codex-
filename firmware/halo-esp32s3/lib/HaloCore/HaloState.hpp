#pragma once

#include <HaloProtocol.hpp>

#include <array>
#include <cstdint>

namespace halo {

constexpr uint8_t kRingCount = 4;
constexpr uint8_t kNoSelectedRing = 0xff;
constexpr uint32_t kWatchdogTimeoutMs = 3000;
constexpr uint8_t kDisconnectedBrightness = 16;

enum class DisplayMode : uint8_t {
  Ambient = 0,
  Overview = 1,
  Detail = 2,
};

enum class TaskStatus : uint8_t {
  Running = 1,
  Waiting = 2,
  RoundCompleted = 3,
  Failed = 4,
  Queued = 5,
  Idle = 6,
  Unknown = 7,
};

enum class Direction : uint8_t {
  Clockwise = 0,
  CounterClockwise = 1,
};

enum class NackReason : uint8_t {
  MalformedPayload = 1,
  UnsupportedMessage = 2,
  InvalidState = 3,
  Busy = 4,
  InternalError = 5,
};

enum class KnobAction : uint8_t {
  Rotate = 1,
  ShortPress = 2,
  LongPress = 3,
};

struct KnobEvent {
  KnobAction action{KnobAction::Rotate};
  int8_t delta{0};
};

struct RingState {
  uint8_t index{0};
  TaskStatus status{TaskStatus::Unknown};
  uint8_t brightness{0};
  uint16_t speedPercent{100};
  Direction direction{Direction::Clockwise};
  uint8_t tailPercent{0};
};

struct DeviceState {
  uint64_t revision{0};
  uint8_t globalBrightness{0};
  uint8_t effectiveBrightness{0};
  DisplayMode displayMode{DisplayMode::Ambient};
  uint8_t selectedRing{kNoSelectedRing};
  uint8_t ringCount{kRingCount};
  std::array<RingState, kRingCount> rings{};
  bool authoritative{false};
  bool disconnected{true};
};

struct PowerPolicy {
  uint16_t maxMilliAmps{500};
  uint8_t brightnessCeiling{30};

  uint8_t limitBrightness(uint8_t requestedBrightness,
                          uint16_t ledCount) const;
};

struct ControllerResponse : Frame {
  bool shouldSend{false};
  bool stateChanged{false};
};

class DeviceController {
 public:
  explicit DeviceController(PowerPolicy powerPolicy = {},
                            uint16_t estimatedLedCount = 20);

  ControllerResponse handle(const Frame& frame, uint32_t nowMs);
  ControllerResponse handleProtocolError(const DecodeResult& result);
  bool tick(uint32_t nowMs);

  const DeviceState& state() const { return state_; }

 private:
  ControllerResponse acknowledge(const Frame& frame, bool stateChanged);
  ControllerResponse reject(const Frame& frame, NackReason reason);
  bool markCommunication(uint32_t nowMs);
  void updateEffectiveBrightness();

  DeviceState state_{};
  PowerPolicy powerPolicy_{};
  uint16_t estimatedLedCount_{20};
  uint32_t lastCommunicationMs_{0};
  bool communicationStarted_{false};
  bool protocolCompatible_{true};
};

}  // namespace halo
