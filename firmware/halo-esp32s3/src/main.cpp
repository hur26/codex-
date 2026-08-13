#include <Arduino.h>
#include <HaloHal.hpp>
#include <HaloProtocol.hpp>
#include <HaloState.hpp>

#include <cstddef>
#include <vector>

namespace {

class SerialWriter final : public halo::ByteWriter {
 public:
  int availableForWrite() override { return Serial.availableForWrite(); }

  size_t write(const uint8_t* bytes, size_t length) override {
    return Serial.write(bytes, length);
  }
};

halo::Decoder decoder{halo::DecoderMode::StrictV01};
halo::DeviceController controller({500, 30});
halo::NullRingRenderer ringRenderer;
halo::NullDisplayRenderer displayRenderer;
halo::NullKnobInput knobInput;
halo::TxPump transmitPump;
halo::DeviceEventSender eventSender;
SerialWriter serialWriter;
std::vector<halo::DecodeResult> pendingDecoded;
size_t pendingDecodedIndex = 0;

void applyState() {
  ringRenderer.apply(controller.state());
  displayRenderer.apply(controller.state());
}

void processDecoded(uint32_t nowMs) {
  while (pendingDecodedIndex < pendingDecoded.size()) {
    const auto& decoded = pendingDecoded[pendingDecodedIndex];
    if (controller.willRespondTo(decoded) && transmitPump.full()) {
      return;
    }

    const auto response = decoded.ok()
                              ? controller.handle(decoded.frame, nowMs)
                              : controller.handleProtocolError(decoded);
    if (response.shouldSend && !transmitPump.enqueue(response)) {
      return;
    }
    if (response.stateChanged) {
      applyState();
    }
    ++pendingDecodedIndex;
  }

  pendingDecoded.clear();
  pendingDecodedIndex = 0;
}

void readSerial() {
  uint8_t bytes[64];
  size_t length = 0;
  while (length < sizeof(bytes) && Serial.available() > 0) {
    bytes[length++] = static_cast<uint8_t>(Serial.read());
  }

  if (length == 0) {
    return;
  }

  pendingDecoded = decoder.push(bytes, length);
  pendingDecodedIndex = 0;
}

void reportKnob(uint32_t nowMs) {
  const auto event = knobInput.poll(nowMs);
  if (!event.has_value()) {
    return;
  }

  eventSender.enqueue(transmitPump, halo::MessageType::KnobEvent,
                      {static_cast<uint8_t>(event->action),
                       static_cast<uint8_t>(event->delta)});
}

void reportDiagnostic() {
  eventSender.enqueuePendingDiagnostic(transmitPump, controller);
}

}  // namespace

void setup() {
  Serial.begin(115200);
  Serial.setTxTimeoutMs(0);
}

void loop() {
  const uint32_t nowMs = millis();
  transmitPump.pump(serialWriter);
  processDecoded(nowMs);
  if (pendingDecoded.empty() && transmitPump.empty()) {
    readSerial();
    processDecoded(nowMs);
  }
  if (controller.tick(nowMs)) {
    applyState();
  }
  ringRenderer.tick(nowMs);
  reportDiagnostic();
  if (!controller.pendingDiagnostic().has_value() && !transmitPump.full()) {
    reportKnob(nowMs);
  }
  transmitPump.pump(serialWriter);
  delay(1);
}
