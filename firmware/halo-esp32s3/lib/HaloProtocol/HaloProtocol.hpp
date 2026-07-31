#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <vector>

namespace halo {

constexpr std::array<uint8_t, 2> kMagic{0x43, 0x48};
constexpr uint8_t kProtocolMajor = 1;
constexpr size_t kMaxPayload = 512;

enum class MessageType : uint8_t {
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
};

enum class ProtocolError : uint8_t {
  None,
  UnsupportedVersion,
  UnknownMessageType,
  PayloadTooLarge,
  CrcMismatch,
};

struct Frame {
  MessageType type{MessageType::Hello};
  uint16_t sequence{0};
  std::vector<uint8_t> payload;
};

struct DecodeResult {
  Frame frame;
  ProtocolError error{ProtocolError::None};

  bool ok() const { return error == ProtocolError::None; }
};

uint16_t crc16CcittFalse(const uint8_t* bytes, size_t length);
std::vector<uint8_t> encode(const Frame& frame);

class Decoder {
 public:
  static constexpr size_t kBufferCapacity = kMaxPayload + 10;

  std::vector<DecodeResult> push(const uint8_t* bytes, size_t length);

  size_t bufferedSize() const { return used_; }
  static constexpr size_t bufferCapacity() { return kBufferCapacity; }

 private:
  void pushByte(uint8_t byte);
  void decodeReady(std::vector<DecodeResult>& decoded);
  void discardPrefix(size_t length);
  void resynchronize();

  std::array<uint8_t, kBufferCapacity> buffer_{};
  size_t used_{0};
};

}  // namespace halo
