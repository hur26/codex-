#include "HaloProtocol.hpp"

#include <algorithm>

namespace halo {
namespace {

constexpr size_t kHeaderBytes = 8;
constexpr size_t kCrcBytes = 2;

bool isKnownMessageType(uint8_t value) {
  switch (value) {
    case static_cast<uint8_t>(MessageType::Hello):
    case static_cast<uint8_t>(MessageType::Capabilities):
    case static_cast<uint8_t>(MessageType::FullSnapshot):
    case static_cast<uint8_t>(MessageType::RingUpdate):
    case static_cast<uint8_t>(MessageType::DisplayMode):
    case static_cast<uint8_t>(MessageType::Brightness):
    case static_cast<uint8_t>(MessageType::Heartbeat):
    case static_cast<uint8_t>(MessageType::Ack):
    case static_cast<uint8_t>(MessageType::Nack):
    case static_cast<uint8_t>(MessageType::KnobEvent):
    case static_cast<uint8_t>(MessageType::Diagnostics):
      return true;
    default:
      return false;
  }
}

uint16_t readUint16Le(const uint8_t* bytes) {
  return static_cast<uint16_t>(bytes[0]) |
         static_cast<uint16_t>(static_cast<uint16_t>(bytes[1]) << 8);
}

void appendUint16Le(std::vector<uint8_t>& bytes, uint16_t value) {
  bytes.push_back(static_cast<uint8_t>(value & 0xff));
  bytes.push_back(static_cast<uint8_t>(value >> 8));
}

DecodeResult failure(ProtocolError error) {
  DecodeResult result;
  result.error = error;
  return result;
}

}  // namespace

uint16_t crc16CcittFalse(const uint8_t* bytes, size_t length) {
  uint16_t crc = 0xffff;
  for (size_t index = 0; index < length; ++index) {
    crc = static_cast<uint16_t>(crc ^
                                static_cast<uint16_t>(bytes[index] << 8));
    for (uint8_t bit = 0; bit < 8; ++bit) {
      crc = (crc & 0x8000) != 0
                ? static_cast<uint16_t>((crc << 1) ^ 0x1021)
                : static_cast<uint16_t>(crc << 1);
    }
  }
  return crc;
}

std::vector<uint8_t> encode(const Frame& frame) {
  if (frame.payload.size() > kMaxPayload) {
    return {};
  }

  std::vector<uint8_t> encoded;
  encoded.reserve(kHeaderBytes + frame.payload.size() + kCrcBytes);
  encoded.insert(encoded.end(), kMagic.begin(), kMagic.end());
  encoded.push_back(kProtocolMajor);
  encoded.push_back(static_cast<uint8_t>(frame.type));
  appendUint16Le(encoded, frame.sequence);
  appendUint16Le(encoded, static_cast<uint16_t>(frame.payload.size()));
  encoded.insert(encoded.end(), frame.payload.begin(), frame.payload.end());
  const uint16_t crc =
      crc16CcittFalse(encoded.data() + kMagic.size(),
                     encoded.size() - kMagic.size());
  appendUint16Le(encoded, crc);
  return encoded;
}

std::vector<DecodeResult> Decoder::push(const uint8_t* bytes, size_t length) {
  std::vector<DecodeResult> decoded;
  if (bytes == nullptr) {
    return decoded;
  }

  for (size_t index = 0; index < length; ++index) {
    pushByte(bytes[index]);
    decodeReady(decoded);
  }
  return decoded;
}

void Decoder::pushByte(uint8_t byte) {
  if (used_ == 0) {
    if (byte == kMagic[0]) {
      buffer_[used_++] = byte;
    }
    return;
  }

  if (used_ == 1) {
    if (byte == kMagic[1]) {
      buffer_[used_++] = byte;
    } else if (byte != kMagic[0]) {
      used_ = 0;
    }
    return;
  }

  if (used_ < buffer_.size()) {
    buffer_[used_++] = byte;
  }
}

void Decoder::decodeReady(std::vector<DecodeResult>& decoded) {
  while (used_ >= kHeaderBytes) {
    const size_t payloadLength = readUint16Le(buffer_.data() + 6);
    if (payloadLength > kMaxPayload) {
      decoded.push_back(failure(ProtocolError::PayloadTooLarge));
      resynchronize();
      continue;
    }

    if (buffer_[2] != kProtocolMajor) {
      decoded.push_back(failure(ProtocolError::UnsupportedVersion));
      resynchronize();
      continue;
    }

    if (!isKnownMessageType(buffer_[3])) {
      decoded.push_back(failure(ProtocolError::UnknownMessageType));
      resynchronize();
      continue;
    }

    const size_t frameLength = kHeaderBytes + payloadLength + kCrcBytes;
    if (used_ < frameLength) {
      return;
    }

    const uint16_t expectedCrc =
        readUint16Le(buffer_.data() + frameLength - kCrcBytes);
    const uint16_t actualCrc = crc16CcittFalse(
        buffer_.data() + kMagic.size(), frameLength - kMagic.size() - kCrcBytes);
    if (actualCrc != expectedCrc) {
      decoded.push_back(failure(ProtocolError::CrcMismatch));
      discardPrefix(frameLength);
      continue;
    }

    DecodeResult result;
    result.frame.type = static_cast<MessageType>(buffer_[3]);
    result.frame.sequence = readUint16Le(buffer_.data() + 4);
    result.frame.payload.assign(buffer_.begin() + kHeaderBytes,
                                buffer_.begin() + kHeaderBytes + payloadLength);
    decoded.push_back(std::move(result));
    discardPrefix(frameLength);
  }
}

void Decoder::discardPrefix(size_t length) {
  if (length >= used_) {
    used_ = 0;
    return;
  }
  std::move(buffer_.begin() + length, buffer_.begin() + used_, buffer_.begin());
  used_ -= length;
}

void Decoder::resynchronize() {
  for (size_t index = 1; index + 1 < used_; ++index) {
    if (buffer_[index] == kMagic[0] && buffer_[index + 1] == kMagic[1]) {
      discardPrefix(index);
      return;
    }
  }

  if (used_ > 0 && buffer_[used_ - 1] == kMagic[0]) {
    buffer_[0] = kMagic[0];
    used_ = 1;
  } else {
    used_ = 0;
  }
}

}  // namespace halo
