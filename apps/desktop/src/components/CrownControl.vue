<script setup lang="ts">
import { onUnmounted, ref } from "vue";
import type { DisplayMode } from "../types/halo";

const props = withDefaults(
  defineProps<{
    mode: DisplayMode;
    selectedSlot?: number | null;
    longPressMs?: number;
  }>(),
  {
    selectedSlot: null,
    longPressMs: 650,
  },
);

const emit = defineEmits<{
  "update:mode": [mode: DisplayMode];
  select: [slot: number];
}>();

const MODE_SEQUENCE: DisplayMode[] = ["ambient", "overview", "detail"];

let holdTimer: ReturnType<typeof setTimeout> | null = null;
const pressActive = ref(false);
let longPressTriggered = false;

function clearHoldTimer() {
  if (holdTimer !== null) {
    clearTimeout(holdTimer);
    holdTimer = null;
  }
}

function beginPress() {
  if (pressActive.value) {
    return;
  }

  pressActive.value = true;
  longPressTriggered = false;
  holdTimer = setTimeout(() => {
    holdTimer = null;
    if (!pressActive.value) {
      return;
    }

    longPressTriggered = true;
    emit("update:mode", "ambient");
  }, props.longPressMs);
}

function nextMode() {
  const currentIndex = MODE_SEQUENCE.indexOf(props.mode);
  return MODE_SEQUENCE[(currentIndex + 1) % MODE_SEQUENCE.length];
}

function finishPress() {
  if (!pressActive.value) {
    return;
  }

  pressActive.value = false;
  clearHoldTimer();

  if (!longPressTriggered) {
    emit("update:mode", nextMode());
  }

  longPressTriggered = false;
}

function cancelPress() {
  pressActive.value = false;
  longPressTriggered = false;
  clearHoldTimer();
}

function rotate(direction: -1 | 1) {
  if (props.selectedSlot === null) {
    emit("select", direction === 1 ? 0 : 3);
    return;
  }

  const normalized = ((props.selectedSlot % 4) + 4) % 4;
  emit("select", (normalized + direction + 4) % 4);
}

function isActivationKey(event: KeyboardEvent) {
  return event.key === "Enter" || event.key === " ";
}

function handleKeyDown(event: KeyboardEvent) {
  if (!isActivationKey(event) || event.repeat) {
    return;
  }

  event.preventDefault();
  beginPress();
}

function handleKeyUp(event: KeyboardEvent) {
  if (!isActivationKey(event)) {
    return;
  }

  event.preventDefault();
  finishPress();
}

function handleKeyCancel(event: KeyboardEvent) {
  if (event.key === "Escape") {
    cancelPress();
  }
}

onUnmounted(cancelPress);
</script>

<template>
  <div
    class="crown-control"
    :data-crown-mode="mode"
    aria-label="Halo 表冠控制"
  >
    <span class="crown-index crown-index-left" aria-hidden="true">−</span>
    <button
      class="rotation-control rotation-left"
      data-crown-left
      type="button"
      aria-label="旋转到上一圈"
      @click="rotate(-1)"
    >
      <span aria-hidden="true">‹</span>
    </button>

    <button
      class="crown-button"
      data-crown-press
      type="button"
      aria-label="表冠：短按切换屏幕模式，长按返回环境模式"
      :aria-pressed="pressActive"
      @pointerdown="beginPress"
      @pointerup="finishPress"
      @pointercancel="cancelPress"
      @pointerleave="cancelPress"
      @keydown="handleKeyDown"
      @keyup="handleKeyUp"
      @keydown.esc="handleKeyCancel"
      @contextmenu.prevent
    >
      <span class="crown-knurl" aria-hidden="true" />
      <span class="crown-cap" aria-hidden="true">
        <i />
        <i />
        <i />
      </span>
    </button>

    <button
      class="rotation-control rotation-right"
      data-crown-right
      type="button"
      aria-label="旋转到下一圈"
      @click="rotate(1)"
    >
      <span aria-hidden="true">›</span>
    </button>
    <span class="crown-index crown-index-right" aria-hidden="true">+</span>
  </div>
</template>

<style scoped>
.crown-control {
  position: absolute;
  z-index: 30;
  right: 3.2%;
  bottom: 9%;
  display: grid;
  grid-template-columns: 1.2rem clamp(2.8rem, 8vw, 4rem) 1.2rem;
  align-items: center;
  transform: rotate(-35deg);
}

.crown-control::before {
  position: absolute;
  z-index: -1;
  top: 50%;
  left: 50%;
  width: 150%;
  height: 54%;
  border: 1px solid var(--halo-hairline);
  border-radius: 999px;
  background: var(--halo-canvas-raised);
  box-shadow: 0 0.8rem 1.5rem var(--halo-shadow);
  content: "";
  transform: translate(-50%, -50%);
}

.crown-button,
.rotation-control {
  margin: 0;
  border: 0;
  outline: none;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}

.crown-button {
  position: relative;
  width: clamp(2.8rem, 8vw, 4rem);
  aspect-ratio: 1;
  padding: 0;
  border: 1px solid var(--halo-metal);
  border-radius: 50%;
  background:
    repeating-conic-gradient(
      var(--halo-metal-dark) 0 3deg,
      var(--halo-metal) 3deg 5deg
    );
  box-shadow:
    inset 0 0 0 0.2rem var(--halo-metal-dark),
    inset 0 0 0 0.28rem var(--halo-metal-bright),
    0 0.5rem 1rem var(--halo-shadow);
  touch-action: none;
  transform: rotate(35deg);
}

.crown-button::after {
  position: absolute;
  inset: -0.42rem;
  border: 1px solid transparent;
  border-radius: 50%;
  content: "";
  transition:
    border-color 160ms ease,
    box-shadow 160ms ease;
}

.crown-button:hover::after,
.crown-button:focus-visible::after {
  border-color: var(--halo-focus);
  box-shadow: 0 0 0.55rem var(--halo-unknown-glow);
}

.crown-button:active .crown-cap,
.crown-button[aria-pressed="true"] .crown-cap {
  transform: scale(0.94);
}

.crown-knurl {
  position: absolute;
  inset: 0.28rem;
  border-radius: 50%;
  background: radial-gradient(
    circle at 40% 32%,
    var(--halo-metal-bright),
    var(--halo-metal-dark) 64%
  );
}

.crown-cap {
  position: absolute;
  inset: 0.64rem;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.16rem;
  border: 1px solid var(--halo-hairline);
  border-radius: 50%;
  background: radial-gradient(
    circle at 42% 35%,
    var(--halo-glass-highlight),
    var(--halo-canvas)
  );
  box-shadow: inset 0 0.4rem 0.7rem var(--halo-shadow);
  transition: transform 120ms ease;
}

.crown-cap i {
  width: 0.16rem;
  height: 0.16rem;
  border-radius: 50%;
  background: var(--halo-running);
  box-shadow: 0 0 0.35rem var(--halo-running-glow);
}

.rotation-control {
  z-index: 2;
  display: grid;
  width: 1.3rem;
  aspect-ratio: 1;
  padding: 0;
  place-items: center;
  border: 1px solid var(--halo-hairline);
  border-radius: 50%;
  color: var(--halo-text-muted);
  background: var(--halo-canvas);
  font-family: var(--halo-font-mono);
  transform: rotate(35deg);
}

.rotation-control:hover,
.rotation-control:focus-visible {
  border-color: var(--halo-focus);
  color: var(--halo-focus);
}

.crown-index {
  position: absolute;
  top: -0.4rem;
  color: var(--halo-metal-bright);
  font-family: var(--halo-font-mono);
  font-size: 0.5rem;
  pointer-events: none;
}

.crown-index-left {
  left: 0.5rem;
}

.crown-index-right {
  right: 0.5rem;
}

@media (max-width: 520px) {
  .crown-control {
    right: 0;
    bottom: 6%;
    grid-template-columns: 1rem 2.8rem 1rem;
  }
}

@media (prefers-reduced-motion: reduce) {
  .crown-button::after,
  .crown-cap {
    transition: none;
  }
}
</style>
