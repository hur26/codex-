<script setup lang="ts">
import { computed } from "vue";
import type {
  DeviceConnectionState,
  DeviceStatus,
} from "../types/halo";

const props = defineProps<{
  status: DeviceStatus;
}>();

const LABELS: Record<DeviceConnectionState, string> = {
  virtual: "VIRTUAL",
  connecting: "CONNECTING",
  online: "ONLINE",
  incompatible: "INCOMPATIBLE",
  error: "ERROR",
};
const SAFE_DIAGNOSTIC_MESSAGES = new Set([
  "Device endpoint was not found",
  "Device discovery failed",
  "Device connection failed",
  "Protocol major is incompatible",
  "Device read failed",
  "Device capabilities are incompatible",
  "Device rejected state update",
  "Device snapshot was invalid",
  "Device update was invalid",
  "Device frame could not be encoded",
  "Device write failed",
  "Device response timed out",
  "Device retry failed",
  "Device heartbeat failed",
  "Device worker could not start",
  "Virtual device state is unavailable",
]);

const diagnosticMessage = computed(() => {
  const message = props.status.message?.trim();
  if (!message) {
    return null;
  }

  return SAFE_DIAGNOSTIC_MESSAGES.has(message)
    ? message
    : "设备诊断信息已隐藏";
});
const accessibleLabel = computed(() => {
  const base = `Device ${LABELS[props.status.state]}, ${props.status.transport}`;
  return diagnosticMessage.value ? `${base}, ${diagnosticMessage.value}` : base;
});
</script>

<template>
  <div
    class="device-status"
    :class="`device-status-${status.state}`"
    :data-device-state="status.state"
    data-device-status
    role="status"
    aria-live="polite"
    aria-atomic="true"
    :aria-label="accessibleLabel"
    :title="diagnosticMessage ?? undefined"
  >
    <i aria-hidden="true" />
    <span>
      <small>DEVICE</small>
      <strong>{{ LABELS[status.state] }}</strong>
      <span
        v-if="diagnosticMessage"
        class="device-status-message"
        data-device-message
        aria-hidden="true"
      >
        {{ diagnosticMessage }}
      </span>
    </span>
    <span class="device-status-meta" aria-hidden="true">
      <em>{{ status.transport.toUpperCase() }}</em>
      <b>{{ status.firmwareVersion ?? "--" }}</b>
    </span>
  </div>
</template>
