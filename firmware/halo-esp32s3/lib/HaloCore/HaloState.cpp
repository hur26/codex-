#include "HaloState.hpp"

#include <algorithm>
#include <cstddef>
#include <limits>

namespace halo {
namespace {

constexpr size_t kRingPayloadBytes = 8;
constexpr size_t kSnapshotPrefixBytes = 12;
constexpr size_t kSnapshotPayloadBytes =
    kSnapshotPrefixBytes + kRingCount * kRingPayloadBytes;
constexpr uint16_t kMilliAmpsPerLedAtFullBrightness = 60;

uint16_t readUint16Le(const uint8_t* bytes) {
  return static_cast<uint16_t>(bytes[0]) |
         static_cast<uint16_t>(static_cast<uint16_t>(bytes[1]) << 8);
}

uint32_t readUint32Le(const uint8_t* bytes) {
  uint32_t value = 0;
  for (uint8_t index = 0; index < 4; ++index) {
    value |= static_cast<uint32_t>(bytes[index]) << (index * 8);
  }
  return value;
}

bool validDiagnosticSeverity(uint8_t value) { return value >= 1 && value <= 3; }

bool validDiagnosticCode(uint16_t value) { return value >= 1 && value <= 4; }

uint64_t readUint64Le(const uint8_t* bytes) {
  uint64_t value = 0;
  for (uint8_t index = 0; index < 8; ++index) {
    value |= static_cast<uint64_t>(bytes[index]) << (index * 8);
  }
  return value;
}

bool parseDisplayMode(uint8_t value, DisplayMode& mode) {
  if (value > static_cast<uint8_t>(DisplayMode::Detail)) {
    return false;
  }
  mode = static_cast<DisplayMode>(value);
  return true;
}

bool validSelectedRing(uint8_t value) {
  return value < kRingCount || value == kNoSelectedRing;
}

bool parseRing(const uint8_t* payload, size_t length, RingState& ring) {
  if (payload == nullptr || length != kRingPayloadBytes ||
      payload[0] >= kRingCount || payload[1] < 1 || payload[1] > 7 ||
      payload[2] > 100 || payload[5] > 1 || payload[6] > 100 ||
      payload[7] != 0) {
    return false;
  }

  const uint16_t speedPercent = readUint16Le(payload + 3);
  if (speedPercent < 25 || speedPercent > 300) {
    return false;
  }

  ring.index = payload[0];
  ring.status = static_cast<TaskStatus>(payload[1]);
  ring.brightness = payload[2];
  ring.speedPercent = speedPercent;
  ring.direction = static_cast<Direction>(payload[5]);
  ring.tailPercent = payload[6];
  return true;
}

ControllerResponse makeResponse(const Frame& request, MessageType type,
                                std::vector<uint8_t> payload,
                                bool stateChanged) {
  ControllerResponse response;
  response.type = type;
  response.sequence = request.sequence;
  response.payload = std::move(payload);
  response.shouldSend = true;
  response.stateChanged = stateChanged;
  return response;
}

}  // namespace

std::vector<uint8_t> Diagnostic::encodePayload() const {
  const uint16_t rawCode = static_cast<uint16_t>(code);
  return {
      static_cast<uint8_t>(severity),
      static_cast<uint8_t>(rawCode & 0xff),
      static_cast<uint8_t>(rawCode >> 8),
      static_cast<uint8_t>(value & 0xff),
      static_cast<uint8_t>((value >> 8) & 0xff),
      static_cast<uint8_t>((value >> 16) & 0xff),
      static_cast<uint8_t>((value >> 24) & 0xff),
  };
}

std::optional<Diagnostic> Diagnostic::decodePayload(
    const std::vector<uint8_t>& payload) {
  if (payload.size() != 7 || !validDiagnosticSeverity(payload[0])) {
    return std::nullopt;
  }
  const uint16_t rawCode = readUint16Le(payload.data() + 1);
  if (!validDiagnosticCode(rawCode)) {
    return std::nullopt;
  }
  return Diagnostic{static_cast<DiagnosticSeverity>(payload[0]),
                    static_cast<DiagnosticCode>(rawCode),
                    readUint32Le(payload.data() + 3)};
}

uint8_t PowerPolicy::limitBrightness(uint8_t requestedBrightness,
                                     uint16_t ledCount) const {
  if (ledCount == 0 || maxMilliAmps == 0 || brightnessCeiling == 0) {
    return 0;
  }

  const uint32_t fullBrightnessMilliAmps =
      static_cast<uint32_t>(ledCount) * kMilliAmpsPerLedAtFullBrightness;
  const uint32_t currentLimitedBrightness =
      static_cast<uint32_t>(maxMilliAmps) * 100 / fullBrightnessMilliAmps;
  return static_cast<uint8_t>(
      std::min<uint32_t>({requestedBrightness, brightnessCeiling,
                          std::min<uint32_t>(currentLimitedBrightness, 100)}));
}

DeviceController::DeviceController(PowerPolicy powerPolicy,
                                   uint16_t estimatedLedCount)
    : powerPolicy_(powerPolicy), estimatedLedCount_(estimatedLedCount) {
  for (uint8_t index = 0; index < kRingCount; ++index) {
    state_.rings[index].index = index;
  }
}

ControllerResponse DeviceController::handle(const Frame& frame,
                                            uint32_t nowMs) {
  if (!protocolCompatible_ && frame.type != MessageType::Hello &&
      frame.type != MessageType::Diagnostics) {
    return reject(frame, NackReason::InvalidState);
  }

  switch (frame.type) {
    case MessageType::Hello: {
      if (frame.payload != std::vector<uint8_t>{0}) {
        return reject(frame, NackReason::MalformedPayload);
      }
      protocolCompatible_ = true;
      const bool stateChanged = markCommunication(nowMs);
      return makeResponse(frame, MessageType::Capabilities,
                          {0, 0, 1, 0, kRingCount, 3, 0, 0, 2},
                          stateChanged);
    }

    case MessageType::FullSnapshot: {
      if (frame.payload.size() != kSnapshotPayloadBytes ||
          frame.payload[11] != kRingCount || frame.payload[8] > 100) {
        return reject(frame, NackReason::MalformedPayload);
      }

      DeviceState candidate = state_;
      candidate.revision = readUint64Le(frame.payload.data());
      if (!parseDisplayMode(frame.payload[9], candidate.displayMode) ||
          !validSelectedRing(frame.payload[10])) {
        return reject(frame, NackReason::MalformedPayload);
      }

      candidate.globalBrightness = frame.payload[8];
      candidate.selectedRing = frame.payload[10];
      candidate.ringCount = kRingCount;
      for (uint8_t index = 0; index < kRingCount; ++index) {
        RingState ring;
        const uint8_t* bytes =
            frame.payload.data() + kSnapshotPrefixBytes + index * kRingPayloadBytes;
        if (!parseRing(bytes, kRingPayloadBytes, ring) || ring.index != index) {
          return reject(frame, NackReason::MalformedPayload);
        }
        candidate.rings[index] = ring;
      }

      candidate.authoritative = true;
      state_ = candidate;
      markCommunication(nowMs);
      updateEffectiveBrightness();
      return acknowledge(frame, true);
    }

    case MessageType::RingUpdate: {
      RingState ring;
      if (!parseRing(frame.payload.data(), frame.payload.size(), ring)) {
        return reject(frame, NackReason::MalformedPayload);
      }
      if (!state_.authoritative) {
        return reject(frame, NackReason::InvalidState);
      }
      state_.rings[ring.index] = ring;
      markCommunication(nowMs);
      updateEffectiveBrightness();
      return acknowledge(frame, true);
    }

    case MessageType::DisplayMode: {
      DisplayMode mode;
      if (frame.payload.size() != 2 ||
          !parseDisplayMode(frame.payload[0], mode) ||
          !validSelectedRing(frame.payload[1])) {
        return reject(frame, NackReason::MalformedPayload);
      }
      if (!state_.authoritative) {
        return reject(frame, NackReason::InvalidState);
      }
      state_.displayMode = mode;
      state_.selectedRing = frame.payload[1];
      markCommunication(nowMs);
      return acknowledge(frame, true);
    }

    case MessageType::Brightness: {
      if (frame.payload.size() != 1 || frame.payload[0] > 100) {
        return reject(frame, NackReason::MalformedPayload);
      }
      if (!state_.authoritative) {
        return reject(frame, NackReason::InvalidState);
      }
      state_.globalBrightness = frame.payload[0];
      markCommunication(nowMs);
      updateEffectiveBrightness();
      return acknowledge(frame, true);
    }

    case MessageType::Heartbeat: {
      if (!frame.payload.empty()) {
        return reject(frame, NackReason::MalformedPayload);
      }
      ControllerResponse response;
      response.type = MessageType::Heartbeat;
      response.sequence = frame.sequence;
      response.stateChanged = markCommunication(nowMs);
      return response;
    }

    case MessageType::Diagnostics: {
      if (!Diagnostic::decodePayload(frame.payload).has_value()) {
        return reject(frame, NackReason::MalformedPayload);
      }
      return {};
    }

    case MessageType::Capabilities:
    case MessageType::Ack:
    case MessageType::Nack:
    case MessageType::KnobEvent:
      return reject(frame, NackReason::UnsupportedMessage);
  }

  return reject(frame, NackReason::UnsupportedMessage);
}

ControllerResponse DeviceController::handleProtocolError(
    const DecodeResult& result) {
  if (result.ok()) {
    return {};
  }

  if (result.error == ProtocolError::UnsupportedVersion) {
    const bool stateChanged = !state_.disconnected || state_.authoritative;
    protocolCompatible_ = false;
    state_.disconnected = true;
    state_.authoritative = false;
    updateEffectiveBrightness();
    ControllerResponse response;
    response.stateChanged = stateChanged;
    return response;
  }

  if (result.error == ProtocolError::CrcMismatch) {
    incrementDiagnostic(DiagnosticCode::CrcError);
    return {};
  }

  incrementDiagnostic(DiagnosticCode::InvalidPayload);

  if (!result.context.respondable) {
    return {};
  }

  const NackReason reason =
      result.error == ProtocolError::UnknownMessageType
          ? NackReason::UnsupportedMessage
          : NackReason::MalformedPayload;
  ControllerResponse response;
  response.type = MessageType::Nack;
  response.sequence = result.context.sequence;
  response.payload = {result.context.rawMessageType,
                      static_cast<uint8_t>(reason)};
  response.shouldSend = true;
  return response;
}

bool DeviceController::tick(uint32_t nowMs) {
  if (!communicationStarted_ || state_.disconnected ||
      nowMs - lastCommunicationMs_ <= kWatchdogTimeoutMs) {
    return false;
  }

  state_.disconnected = true;
  updateEffectiveBrightness();
  queueDiagnostic({DiagnosticSeverity::Warning,
                   DiagnosticCode::WatchdogDisconnected,
                   kWatchdogTimeoutMs});
  return true;
}

ControllerResponse DeviceController::acknowledge(const Frame& frame,
                                                 bool stateChanged) {
  return makeResponse(frame, MessageType::Ack,
                      {static_cast<uint8_t>(frame.type)}, stateChanged);
}

ControllerResponse DeviceController::reject(const Frame& frame,
                                             NackReason reason) {
  if (reason == NackReason::MalformedPayload) {
    incrementDiagnostic(DiagnosticCode::InvalidPayload);
  }
  return makeResponse(frame, MessageType::Nack,
                      {static_cast<uint8_t>(frame.type),
                       static_cast<uint8_t>(reason)},
                      false);
}

bool DeviceController::markCommunication(uint32_t nowMs) {
  const bool stateChanged = state_.disconnected;
  communicationStarted_ = true;
  lastCommunicationMs_ = nowMs;
  state_.disconnected = false;
  if (stateChanged) {
    updateEffectiveBrightness();
  }
  return stateChanged;
}

void DeviceController::updateEffectiveBrightness() {
  const uint8_t limited = powerPolicy_.limitBrightness(
      state_.globalBrightness, estimatedLedCount_);
  state_.effectiveBrightness =
      state_.disconnected ? std::min(limited, kDisconnectedBrightness) : limited;
  const bool localLimitActive = state_.authoritative && !state_.disconnected &&
                                limited < state_.globalBrightness;
  if (localLimitActive &&
      (!localLimitActive_ || lastLocalLimitValue_ != limited)) {
    queueDiagnostic({DiagnosticSeverity::Info, DiagnosticCode::LocalLimit,
                     limited});
  }
  localLimitActive_ = localLimitActive;
  lastLocalLimitValue_ = limited;
}

std::optional<Diagnostic> DeviceController::pendingDiagnostic() const {
  for (const auto& diagnostic : pendingDiagnostics_) {
    if (diagnostic.has_value()) {
      return diagnostic;
    }
  }
  return std::nullopt;
}

size_t DeviceController::pendingDiagnosticCount() const {
  return static_cast<size_t>(std::count_if(
      pendingDiagnostics_.begin(), pendingDiagnostics_.end(),
      [](const auto& diagnostic) { return diagnostic.has_value(); }));
}

void DeviceController::popPendingDiagnostic() {
  for (auto& diagnostic : pendingDiagnostics_) {
    if (diagnostic.has_value()) {
      diagnostic.reset();
      return;
    }
  }
}

void DeviceController::queueDiagnostic(Diagnostic diagnostic) {
  const size_t index = static_cast<size_t>(diagnostic.code) - 1;
  pendingDiagnostics_[index] = diagnostic;
}

void DeviceController::incrementDiagnostic(DiagnosticCode code) {
  const size_t index = static_cast<size_t>(code) - 1;
  const uint32_t previous = pendingDiagnostics_[index].has_value()
                                ? pendingDiagnostics_[index]->value
                                : 0;
  pendingDiagnostics_[index] = {
      DiagnosticSeverity::Warning, code,
      previous == std::numeric_limits<uint32_t>::max() ? previous
                                                       : previous + 1};
}

}  // namespace halo
