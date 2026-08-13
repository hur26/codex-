#pragma once

#include <HaloProtocol.hpp>

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <vector>

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

enum class DiagnosticSeverity : uint8_t {
  Info = 1,
  Warning = 2,
  Error = 3,
};

enum class DiagnosticCode : uint16_t {
  WatchdogDisconnected = 1,
  CrcError = 2,
  InvalidPayload = 3,
  LocalLimit = 4,
  // Sentinel, never encoded and never valid on the wire. A new code must be
  // added directly above it so that kDiagnosticSlotCount grows with the enum;
  // that constant sizes the fixed slot array and bounds every slot lookup and
  // wire check, so a code added here cannot be silently dropped.
  AfterLastCode,
};

constexpr size_t kDiagnosticSlotCount =
    static_cast<size_t>(DiagnosticCode::AfterLastCode) - 1;

// The codes are contiguous from 1 and map one-to-one onto the fixed coalescing
// slots, so a code becomes a slot index by subtracting one. Pinning each code
// keeps a renumbering or a reused value from silently remapping the slots;
// the sentinel above keeps the count itself in step with the enum.
static_assert(static_cast<uint16_t>(DiagnosticCode::WatchdogDisconnected) == 1,
              "diagnostic codes must stay contiguous from 1");
static_assert(static_cast<uint16_t>(DiagnosticCode::CrcError) == 2,
              "diagnostic codes must stay contiguous from 1");
static_assert(static_cast<uint16_t>(DiagnosticCode::InvalidPayload) == 3,
              "diagnostic codes must stay contiguous from 1");
static_assert(static_cast<uint16_t>(DiagnosticCode::LocalLimit) == 4,
              "diagnostic codes must stay contiguous from 1");

constexpr std::optional<size_t> diagnosticSlotIndex(DiagnosticCode code) {
  const auto raw = static_cast<uint16_t>(code);
  if (raw < 1 || raw > kDiagnosticSlotCount) {
    return std::nullopt;
  }
  return static_cast<size_t>(raw) - 1;
}

struct Diagnostic {
  DiagnosticSeverity severity{DiagnosticSeverity::Info};
  DiagnosticCode code{DiagnosticCode::WatchdogDisconnected};
  uint32_t value{0};

  std::vector<uint8_t> encodePayload() const;
  static std::optional<Diagnostic> decodePayload(
      const std::vector<uint8_t>& payload);
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

  // Whether handling `decoded` may produce a frame to send. A caller reserves
  // transmit capacity with this instead of restating the dispatch rules, so
  // the reservation cannot drift away from what handling actually does.
  bool willRespondTo(const DecodeResult& decoded) const;

  const DeviceState& state() const { return state_; }
  std::optional<Diagnostic> pendingDiagnostic() const;
  size_t pendingDiagnosticCount() const;
  void popPendingDiagnostic();

 private:
  ControllerResponse acknowledge(const Frame& frame, bool stateChanged);
  ControllerResponse reject(const Frame& frame, NackReason reason);
  bool markCommunication(uint32_t nowMs);
  void updateEffectiveBrightness();
  void queueDiagnostic(Diagnostic diagnostic);
  void incrementDiagnostic(DiagnosticCode code);

  DeviceState state_{};
  PowerPolicy powerPolicy_{};
  uint16_t estimatedLedCount_{20};
  uint32_t lastCommunicationMs_{0};
  bool communicationStarted_{false};
  bool protocolCompatible_{true};
  bool localLimitActive_{false};
  uint8_t lastLocalLimitValue_{0};
  std::array<std::optional<Diagnostic>, kDiagnosticSlotCount>
      pendingDiagnostics_{};
};

}  // namespace halo
