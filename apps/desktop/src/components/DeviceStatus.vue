<script setup lang="ts">
import type {
  DeviceConnectionState,
  DeviceStatus,
} from "../types/halo";

defineProps<{
  status: DeviceStatus;
}>();

const LABELS: Record<DeviceConnectionState, string> = {
  virtual: "VIRTUAL",
  connecting: "CONNECTING",
  online: "ONLINE",
  incompatible: "INCOMPATIBLE",
  error: "ERROR",
};
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
    :aria-label="`Device ${LABELS[status.state]}, ${status.transport}`"
  >
    <i aria-hidden="true" />
    <span>
      <small>DEVICE</small>
      <strong>{{ LABELS[status.state] }}</strong>
    </span>
    <span class="device-status-meta" aria-hidden="true">
      <em>{{ status.transport.toUpperCase() }}</em>
      <b>{{ status.firmwareVersion ?? "--" }}</b>
    </span>
  </div>
</template>
