<script setup lang="ts">
import { computed, ref, type CSSProperties } from "vue";
import type {
  ActiveDrag,
  Confidence,
  EffectProfile,
  RingSlot,
  SignalSource,
  TaskStatus,
} from "../types/halo";

const props = withDefaults(
  defineProps<{
    slots: RingSlot[];
    selectedSlot?: number | null;
  }>(),
  {
    selectedSlot: null,
  },
);

const emit = defineEmits<{
  select: [slot: number];
  dragstart: [drag: ActiveDrag];
  drop: [slot: number];
}>();

const dropTarget = ref<number | null>(null);
const EMPTY_EFFECT: EffectProfile = {
  brightness: 80,
  speedPercent: 100,
  direction: "clockwise",
  tailPercent: 35,
};

const STATUS_LABELS: Record<TaskStatus, string> = {
  running: "正在执行",
  waiting: "等待确认",
  roundCompleted: "本轮完成",
  failed: "模拟故障",
  queued: "排队等待",
  idle: "空闲",
  unknown: "状态未知",
};

const SOURCE_LABELS: Record<SignalSource, string> = {
  hook: "Hook",
  simulator: "模拟器",
};

const CONFIDENCE_LABELS: Record<Confidence, string> = {
  observed: "已观测",
  provisional: "候选信号",
  simulated: "模拟信号",
};

const MOTION_BASE_MS: Record<TaskStatus, number> = {
  running: 1800,
  waiting: 2800,
  roundCompleted: 1800,
  failed: 1050,
  queued: 5600,
  idle: 0,
  unknown: 3400,
};

const normalizedSlots = computed<RingSlot[]>(() =>
  Array.from({ length: 4 }, (_, index) => {
    const slot = props.slots.find((candidate) => candidate.index === index);

    return (
      slot ?? {
        index,
        taskKey: null,
        status: "idle",
        source: null,
        confidence: null,
        bindingMode: "auto",
        locked: false,
        effect: { ...EMPTY_EFFECT },
      }
    );
  }),
);

function isSimulatedFailure(slot: RingSlot) {
  return (
    slot.status === "failed" &&
    slot.source === "simulator" &&
    slot.confidence === "simulated"
  );
}

function visualStatus(slot: RingSlot): TaskStatus {
  if (slot.status === "failed" && !isSimulatedFailure(slot)) {
    return "unknown";
  }

  return slot.status;
}

function statusClass(slot: RingSlot) {
  const status = visualStatus(slot);
  return `status-${status.replace(
    /[A-Z]/g,
    (letter) => `-${letter.toLowerCase()}`,
  )}`;
}

function ringLabel(slot: RingSlot) {
  const position = slot.index === 0 ? "（最内圈）" : slot.index === 3 ? "（最外圈）" : "";
  const source = slot.source ? SOURCE_LABELS[slot.source] : "无来源";
  const confidence = slot.confidence
    ? CONFIDENCE_LABELS[slot.confidence]
    : "无可置信度";
  const identity = slot.taskKey ? "匿名任务已绑定" : "无任务";

  const dropHint = dropTarget.value === slot.index ? "，释放以完成绑定" : "";

  return `第 ${slot.index + 1} 圈${position}，${STATUS_LABELS[visualStatus(slot)]}，${identity}，来源 ${source}，${confidence}${dropHint}`;
}

function startSlotDrag(event: DragEvent, slot: RingSlot) {
  if (!slot.taskKey) {
    return;
  }
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("application/x-codex-halo-drag", "slot");
  }
  emit("dragstart", { kind: "slot", slot: slot.index });
}

function enterDropTarget(event: DragEvent, slot: number) {
  event.preventDefault();
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = "move";
  }
  dropTarget.value = slot;
}

function leaveDropTarget(event: DragEvent, slot: number) {
  const current = event.currentTarget;
  const next = event.relatedTarget;
  if (
    current instanceof Node &&
    next instanceof Node &&
    current.contains(next)
  ) {
    return;
  }
  if (dropTarget.value === slot) {
    dropTarget.value = null;
  }
}

function dropOnSlot(event: DragEvent, slot: number) {
  event.preventDefault();
  dropTarget.value = null;
  emit("drop", slot);
}

function ringStyle(slot: RingSlot): CSSProperties {
  const speed = Math.max(25, Math.min(300, slot.effect.speedPercent));
  const duration = MOTION_BASE_MS[visualStatus(slot)];
  const opacity = 0.24 + Math.max(0, Math.min(100, slot.effect.brightness)) * 0.0076;
  const tail = Math.max(1, Math.min(100, slot.effect.tailPercent));

  return {
    "--ring-slot": slot.index,
    "--ring-tail-start": `${100 - tail}%`,
    "--ring-tail-soft-start": `${100 - tail * 0.58}%`,
    "--ring-opacity": opacity.toFixed(3),
    "--ring-opacity-low": Math.max(0.16, opacity * 0.48).toFixed(3),
    "--ring-motion-duration": `${Math.round((duration * 100) / speed)}ms`,
    "--ring-motion-direction":
      slot.effect.direction === "counterClockwise" ? "reverse" : "normal",
  } as CSSProperties;
}
</script>

<template>
  <section
    class="halo-preview"
    aria-label="Codex Halo 四环虚拟设备预览"
  >
    <div class="instrument-scale" aria-hidden="true" />
    <div class="glass-well" aria-hidden="true" />

    <button
      v-for="slot in normalizedSlots"
      :key="slot.index"
      class="halo-ring"
      :class="[statusClass(slot), { selected: selectedSlot === slot.index }]"
      type="button"
      :style="ringStyle(slot)"
      :data-slot="slot.index"
      :data-status="slot.status"
      :data-source="slot.source ?? 'none'"
      :data-confidence="slot.confidence ?? 'none'"
      :data-selected="selectedSlot === slot.index"
      :data-drop-active="dropTarget === slot.index"
      :draggable="slot.taskKey !== null"
      :aria-label="ringLabel(slot)"
      :aria-pressed="selectedSlot === slot.index"
      @click="emit('select', slot.index)"
      @dragstart="startSlotDrag($event, slot)"
      @dragenter="enterDropTarget($event, slot.index)"
      @dragover="enterDropTarget($event, slot.index)"
      @dragleave="leaveDropTarget($event, slot.index)"
      @drop.stop="dropOnSlot($event, slot.index)"
    >
      <span class="ring-light" aria-hidden="true" />
      <span class="slot-index" aria-hidden="true">
        {{ String(slot.index + 1).padStart(2, "0") }}
      </span>
      <span
        v-if="slot.confidence === 'provisional'"
        class="confidence-marker"
        aria-hidden="true"
      >
        PROVISIONAL
      </span>
    </button>
  </section>
</template>

<style scoped>
.halo-preview {
  position: relative;
  isolation: isolate;
  width: var(--halo-preview-size);
  max-width: 100%;
  aspect-ratio: 1;
  border: 1px solid var(--halo-hairline);
  border-radius: 50%;
  background:
    radial-gradient(
      circle at 48% 42%,
      var(--halo-glass-highlight) 0,
      var(--halo-glass) 22%,
      var(--halo-canvas-raised) 60%,
      var(--halo-canvas) 77%
    );
  box-shadow:
    inset 0 0 0 0.5rem var(--halo-shadow),
    inset 0 0 4rem var(--halo-shadow),
    0 2.8rem 6rem var(--halo-shadow);
}

.halo-preview::after {
  position: absolute;
  z-index: 20;
  inset: 7.2%;
  border: 1px solid var(--halo-glass-sheen);
  border-radius: 50%;
  background: linear-gradient(
    132deg,
    var(--halo-glass-sheen),
    transparent 32%,
    transparent 66%,
    var(--halo-glass-sheen)
  );
  box-shadow: inset 0 0 1.4rem var(--halo-shadow);
  content: "";
  pointer-events: none;
}

.instrument-scale {
  position: absolute;
  z-index: 1;
  inset: 2.5%;
  border-radius: 50%;
  background: repeating-conic-gradient(
    from -0.5deg,
    var(--halo-metal-bright) 0 0.28deg,
    transparent 0.28deg 4.5deg
  );
  opacity: 0.46;
  -webkit-mask: radial-gradient(
    farthest-side,
    transparent calc(100% - 0.55rem),
    #000 calc(100% - 0.54rem)
  );
  mask: radial-gradient(
    farthest-side,
    transparent calc(100% - 0.55rem),
    #000 calc(100% - 0.54rem)
  );
}

.glass-well {
  position: absolute;
  z-index: 2;
  inset: 34.5%;
  border: 1px solid var(--halo-hairline);
  border-radius: 50%;
  background: radial-gradient(
    circle at 42% 34%,
    var(--halo-glass-highlight),
    var(--halo-canvas) 72%
  );
  box-shadow:
    inset 0 0.8rem 1.7rem var(--halo-shadow),
    0 0 0 0.22rem var(--halo-metal-dark);
}

.halo-ring {
  --ring-color: var(--halo-idle);
  --ring-glow: var(--halo-idle-glow);

  position: absolute;
  z-index: calc(10 - var(--ring-slot));
  top: 50%;
  left: 50%;
  width: var(--ring-size);
  aspect-ratio: 1;
  margin: 0;
  padding: 0;
  border: 0;
  border-radius: 50%;
  outline: none;
  background: transparent;
  transform: translate(-50%, -50%);
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}

.halo-ring[data-slot="0"] {
  --ring-size: var(--halo-ring-inner-size);
}

.halo-ring[data-slot="1"] {
  --ring-size: calc(
    var(--halo-ring-inner-size) + var(--halo-ring-width) +
      var(--halo-ring-gap) + var(--halo-ring-width) + var(--halo-ring-gap)
  );
}

.halo-ring[data-slot="2"] {
  --ring-size: calc(
    var(--halo-ring-inner-size) + var(--halo-ring-width) +
      var(--halo-ring-gap) + var(--halo-ring-width) + var(--halo-ring-gap) +
      var(--halo-ring-width) + var(--halo-ring-gap) + var(--halo-ring-width) +
      var(--halo-ring-gap)
  );
}

.halo-ring[data-slot="3"] {
  --ring-size: calc(
    var(--halo-ring-inner-size) + var(--halo-ring-width) +
      var(--halo-ring-gap) + var(--halo-ring-width) + var(--halo-ring-gap) +
      var(--halo-ring-width) + var(--halo-ring-gap) + var(--halo-ring-width) +
      var(--halo-ring-gap) + var(--halo-ring-width) + var(--halo-ring-gap) +
      var(--halo-ring-width) + var(--halo-ring-gap)
  );
}

.ring-light {
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: conic-gradient(
    from -32deg,
    transparent 0 var(--ring-tail-start),
    var(--ring-color) var(--ring-tail-soft-start),
    var(--ring-color) 97%,
    var(--halo-text) 100%
  );
  filter: drop-shadow(0 0 0.7rem var(--ring-glow));
  opacity: var(--ring-opacity);
  -webkit-mask: radial-gradient(
    farthest-side,
    transparent calc(100% - var(--halo-ring-width)),
    #000 calc(100% - var(--halo-ring-width) + 1px)
  );
  mask: radial-gradient(
    farthest-side,
    transparent calc(100% - var(--halo-ring-width)),
    #000 calc(100% - var(--halo-ring-width) + 1px)
  );
}

.halo-ring::after {
  position: absolute;
  inset: calc(var(--halo-ring-width) * -0.45);
  border: 1px solid transparent;
  border-radius: inherit;
  content: "";
  transition:
    border-color 160ms ease,
    box-shadow 160ms ease;
}

.halo-ring:hover::after {
  border-color: var(--halo-hairline);
}

.halo-ring:focus-visible::after,
.halo-ring.selected::after,
.halo-ring[data-drop-active="true"]::after {
  border-color: var(--halo-focus);
  box-shadow: 0 0 0 0.18rem var(--halo-canvas), 0 0 0 0.24rem var(--halo-focus);
}

.halo-ring[draggable="true"] {
  cursor: grab;
}

.halo-ring[draggable="true"]:active {
  cursor: grabbing;
}

.status-running {
  --ring-color: var(--halo-running);
  --ring-glow: var(--halo-running-glow);
}

.status-running .ring-light {
  animation: halo-chase
    var(--ring-motion-duration, var(--halo-motion-running))
    linear infinite;
  animation-direction: var(--ring-motion-direction);
}

.status-waiting {
  --ring-color: var(--halo-waiting);
  --ring-glow: var(--halo-waiting-glow);
}

.status-waiting .ring-light {
  background: conic-gradient(
    from 0deg,
    var(--ring-color),
    var(--ring-color)
  );
  animation: halo-breathe
    var(--ring-motion-duration, var(--halo-motion-waiting))
    ease-in-out infinite;
}

.status-round-completed {
  --ring-color: var(--halo-round-completed);
  --ring-glow: var(--halo-round-completed-glow);
}

.status-round-completed .ring-light {
  background: conic-gradient(
    from 0deg,
    var(--ring-color),
    var(--ring-color)
  );
  animation: halo-confirm
    var(--ring-motion-duration, var(--halo-motion-round-completed))
    ease-out 2;
}

.status-failed {
  --ring-color: var(--halo-failed);
  --ring-glow: var(--halo-failed-glow);
}

.status-failed .ring-light {
  background: conic-gradient(
    from 0deg,
    var(--ring-color),
    var(--ring-color)
  );
  animation: halo-fault
    var(--ring-motion-duration, var(--halo-motion-failed))
    steps(2, end) infinite;
}

.status-queued {
  --ring-color: var(--halo-queued);
  --ring-glow: var(--halo-queued-glow);
}

.status-queued .ring-light {
  animation: halo-chase
    var(--ring-motion-duration, var(--halo-motion-queued))
    linear infinite;
  animation-direction: var(--ring-motion-direction);
}

.status-unknown {
  --ring-color: var(--halo-unknown);
  --ring-glow: var(--halo-unknown-glow);
}

.status-unknown .ring-light {
  background: conic-gradient(
    from 0deg,
    var(--ring-color),
    var(--ring-color)
  );
  animation: halo-unknown
    var(--ring-motion-duration, var(--halo-motion-unknown))
    ease-in-out infinite;
}

.status-idle {
  --ring-color: var(--halo-idle);
  --ring-glow: var(--halo-idle-glow);
}

.status-idle .ring-light {
  background: conic-gradient(
    from 0deg,
    var(--ring-color),
    var(--ring-color)
  );
  opacity: 0.3;
}

.slot-index,
.confidence-marker {
  position: absolute;
  z-index: 2;
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  line-height: 1;
  pointer-events: none;
}

.slot-index {
  top: 50%;
  left: calc(var(--halo-ring-width) * -2.7);
  font-size: clamp(0.48rem, 1vw, 0.62rem);
  letter-spacing: 0.1em;
  transform: translate(-100%, -50%);
}

.confidence-marker {
  top: calc(var(--halo-ring-width) * -1.45);
  left: 50%;
  padding: 0.2rem 0.34rem;
  border: 1px solid var(--halo-waiting);
  border-radius: 999px;
  color: var(--halo-waiting);
  background: var(--halo-canvas-raised);
  font-size: clamp(0.38rem, 0.8vw, 0.51rem);
  letter-spacing: 0.09em;
  transform: translate(-50%, -100%);
}

@keyframes halo-chase {
  to {
    transform: rotate(1turn);
  }
}

@keyframes halo-breathe {
  0%,
  100% {
    filter: drop-shadow(0 0 0.3rem var(--ring-glow));
    opacity: var(--ring-opacity-low);
  }

  50% {
    filter: drop-shadow(0 0 1rem var(--ring-glow));
    opacity: var(--ring-opacity);
  }
}

@keyframes halo-confirm {
  0% {
    filter: drop-shadow(0 0 0.2rem var(--ring-glow));
    opacity: 0.35;
  }

  42%,
  100% {
    filter: drop-shadow(0 0 1.1rem var(--ring-glow));
    opacity: var(--ring-opacity);
  }
}

@keyframes halo-fault {
  0%,
  35% {
    opacity: var(--ring-opacity);
  }

  36%,
  100% {
    opacity: 0.2;
  }
}

@keyframes halo-unknown {
  0%,
  100% {
    opacity: 0.26;
  }

  50% {
    opacity: var(--ring-opacity);
  }
}

@media (max-width: 520px) {
  .slot-index,
  .confidence-marker {
    font-size: clamp(0.36rem, 1.75vw, 0.46rem);
    letter-spacing: 0.035em;
  }

  .slot-index {
    left: calc(var(--halo-ring-width) * -0.7);
    transform: translate(-100%, -50%);
  }

  .confidence-marker {
    top: calc(var(--halo-ring-width) * -0.65);
    padding: 0.12rem 0.2rem;
  }
}

@media (prefers-reduced-motion: reduce) {
  .halo-ring .ring-light {
    animation: none;
  }
}
</style>
