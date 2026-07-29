<script setup lang="ts">
import { computed } from "vue";
import type {
  Confidence,
  DisplayMode,
  RingSlot,
  SignalSource,
  TaskStatus,
} from "../types/halo";

const props = withDefaults(
  defineProps<{
    mode: DisplayMode;
    slots: RingSlot[];
    selectedSlot?: number | null;
  }>(),
  {
    selectedSlot: null,
  },
);

const STATUS_LABELS: Record<TaskStatus, string> = {
  running: "执行中",
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

const normalizedSlots = computed(() =>
  Array.from({ length: 4 }, (_, index) => {
    const slot = props.slots.find((candidate) => candidate.index === index);

    return {
      index,
      status: slot?.status ?? ("idle" as const),
      source: slot?.source ?? null,
      confidence: slot?.confidence ?? null,
    };
  }),
);

const selected = computed(() => {
  if (
    props.selectedSlot === null ||
    props.selectedSlot < 0 ||
    props.selectedSlot > 3
  ) {
    return null;
  }

  return normalizedSlots.value[props.selectedSlot] ?? null;
});

function statusClass(status: TaskStatus) {
  return `status-${status.replace(
    /[A-Z]/g,
    (letter) => `-${letter.toLowerCase()}`,
  )}`;
}
</script>

<template>
  <section
    class="central-display"
    data-central-display
    :data-mode="mode"
    :aria-label="`中央 OLED，${mode} 模式`"
  >
    <div class="screen-noise" aria-hidden="true" />

    <div v-if="mode === 'ambient'" class="ambient-view">
      <svg
        class="halo-glyph"
        data-halo-glyph
        viewBox="0 0 120 120"
        role="img"
        aria-label="Halo 四节点状态图形"
      >
        <circle class="glyph-orbit orbit-outer" cx="60" cy="60" r="43" />
        <circle class="glyph-orbit orbit-inner" cx="60" cy="60" r="25" />
        <path class="glyph-axis" d="M60 17v86M17 60h86" />
        <circle class="glyph-core" cx="60" cy="60" r="7" />
        <circle
          v-for="slot in normalizedSlots"
          :key="slot.index"
          class="glyph-node"
          :class="statusClass(slot.status)"
          data-halo-node
          :cx="[60, 103, 60, 17][slot.index]"
          :cy="[17, 60, 103, 60][slot.index]"
          r="4.5"
        />
      </svg>
      <p class="ambient-wordmark">CODEX HALO</p>
      <p class="ambient-caption">FOUR CHANNEL STATUS ARRAY</p>
    </div>

    <div v-else-if="mode === 'overview'" class="overview-view">
      <header class="display-header">
        <span>RING ARRAY</span>
        <span>LIVE</span>
      </header>
      <ol class="overview-list" aria-label="四圈状态总览">
        <li
          v-for="slot in normalizedSlots"
          :key="slot.index"
          class="overview-row"
          :class="statusClass(slot.status)"
          :data-overview-slot="slot.index"
        >
          <span class="status-pip" aria-hidden="true" />
          <span class="anonymous-name">
            HALO {{ String(slot.index + 1).padStart(2, "0") }}
          </span>
          <span class="overview-status">{{ STATUS_LABELS[slot.status] }}</span>
        </li>
      </ol>
    </div>

    <div v-else class="detail-view">
      <header class="display-header">
        <span>CHANNEL DETAIL</span>
        <span>{{ selected ? `0${selected.index + 1}` : "--" }}</span>
      </header>

      <div
        v-if="selected"
        class="detail-readout"
        :class="statusClass(selected.status)"
        :data-detail-slot="selected.index"
      >
        <div class="detail-primary">
          <span class="status-pip" aria-hidden="true" />
          <strong>第 {{ selected.index + 1 }} 圈</strong>
        </div>
        <dl class="detail-grid">
          <div>
            <dt>本轮状态</dt>
            <dd>{{ STATUS_LABELS[selected.status] }}</dd>
          </div>
          <div>
            <dt>来源</dt>
            <dd>
              {{ selected.source ? SOURCE_LABELS[selected.source] : "无来源" }}
            </dd>
          </div>
          <div>
            <dt>可信度</dt>
            <dd>
              {{
                selected.confidence
                  ? CONFIDENCE_LABELS[selected.confidence]
                  : "无可信度"
              }}
            </dd>
          </div>
        </dl>
      </div>

      <div v-else class="detail-empty" data-detail-empty>
        <span class="empty-reticle" aria-hidden="true" />
        <strong>未选择圆环</strong>
        <small>旋转表冠选择 01—04</small>
      </div>
    </div>
  </section>
</template>

<style scoped>
.central-display {
  position: relative;
  isolation: isolate;
  width: clamp(9rem, 25%, 12.5rem);
  aspect-ratio: 1;
  overflow: hidden;
  border: 1px solid var(--halo-metal-dark);
  border-radius: 50%;
  color: var(--halo-text);
  background:
    radial-gradient(
      circle at 42% 32%,
      var(--halo-glass-highlight),
      var(--halo-canvas) 58%
    );
  box-shadow:
    inset 0 0 0 0.3rem var(--halo-canvas),
    inset 0 0 0 0.36rem var(--halo-hairline),
    inset 0 1.3rem 2rem var(--halo-shadow),
    0 0 1.8rem var(--halo-shadow);
}

.central-display::before {
  position: absolute;
  z-index: 4;
  inset: 3.5%;
  border: 1px solid var(--halo-glass-sheen);
  border-radius: inherit;
  background: linear-gradient(
    145deg,
    var(--halo-glass-sheen),
    transparent 38%
  );
  content: "";
  pointer-events: none;
}

.screen-noise {
  position: absolute;
  z-index: 3;
  inset: 0;
  background: repeating-linear-gradient(
    to bottom,
    transparent 0 2px,
    var(--halo-hairline) 3px
  );
  opacity: 0.18;
  pointer-events: none;
}

.ambient-view,
.overview-view,
.detail-view {
  position: absolute;
  inset: 13%;
  display: flex;
  flex-direction: column;
}

.ambient-view {
  align-items: center;
  justify-content: center;
}

.halo-glyph {
  width: 72%;
  overflow: visible;
  filter: drop-shadow(0 0 0.45rem var(--halo-unknown-glow));
}

.glyph-orbit,
.glyph-axis {
  fill: none;
  stroke: var(--halo-focus);
  stroke-width: 0.8;
  opacity: 0.38;
}

.glyph-axis {
  stroke-dasharray: 1 6;
}

.orbit-inner {
  stroke: var(--halo-running);
  stroke-dasharray: 3 5;
  opacity: 0.58;
}

.glyph-core {
  fill: var(--halo-canvas);
  stroke: var(--halo-focus);
  stroke-width: 1.2;
}

.glyph-node {
  fill: var(--halo-idle);
  stroke: var(--halo-canvas);
  stroke-width: 2;
}

.glyph-node.status-running,
.status-running .status-pip {
  fill: var(--halo-running);
  background: var(--halo-running);
}

.glyph-node.status-waiting,
.status-waiting .status-pip {
  fill: var(--halo-waiting);
  background: var(--halo-waiting);
}

.glyph-node.status-round-completed,
.status-round-completed .status-pip {
  fill: var(--halo-round-completed);
  background: var(--halo-round-completed);
}

.glyph-node.status-failed,
.status-failed .status-pip {
  fill: var(--halo-failed);
  background: var(--halo-failed);
}

.glyph-node.status-queued,
.status-queued .status-pip {
  fill: var(--halo-queued);
  background: var(--halo-queued);
}

.glyph-node.status-unknown,
.status-unknown .status-pip {
  fill: var(--halo-unknown);
  background: var(--halo-unknown);
}

.ambient-wordmark,
.ambient-caption {
  margin: 0;
  font-family: var(--halo-font-mono);
  text-align: center;
}

.ambient-wordmark {
  margin-top: -0.18rem;
  color: var(--halo-focus);
  font-size: clamp(0.52rem, 1.25vw, 0.72rem);
  font-weight: 600;
  letter-spacing: 0.18em;
}

.ambient-caption {
  margin-top: 0.24rem;
  color: var(--halo-text-muted);
  font-size: clamp(0.28rem, 0.66vw, 0.4rem);
  letter-spacing: 0.08em;
}

.display-header {
  display: flex;
  justify-content: space-between;
  padding-bottom: 0.38rem;
  border-bottom: 1px solid var(--halo-hairline);
  color: var(--halo-focus);
  font-family: var(--halo-font-mono);
  font-size: clamp(0.35rem, 0.8vw, 0.48rem);
  letter-spacing: 0.08em;
}

.overview-list {
  display: grid;
  flex: 1;
  gap: 0;
  margin: 0;
  padding: 0.3rem 0 0;
  list-style: none;
}

.overview-row {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 0.38rem;
  min-height: 0;
  border-bottom: 1px solid var(--halo-hairline);
  font-family: var(--halo-font-mono);
}

.overview-row:last-child {
  border-bottom: 0;
}

.status-pip {
  width: 0.34rem;
  aspect-ratio: 1;
  border: 1px solid var(--halo-metal);
  border-radius: 50%;
  background: var(--halo-idle);
  box-shadow: 0 0 0.36rem currentcolor;
}

.anonymous-name {
  color: var(--halo-text);
  font-size: clamp(0.4rem, 0.92vw, 0.56rem);
  letter-spacing: 0.04em;
}

.overview-status {
  color: var(--halo-text-muted);
  font-size: clamp(0.34rem, 0.78vw, 0.48rem);
  text-align: right;
}

.detail-readout,
.detail-empty {
  display: flex;
  flex: 1;
  flex-direction: column;
  justify-content: center;
}

.detail-primary {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.detail-primary strong {
  font-family: var(--halo-font-mono);
  font-size: clamp(0.64rem, 1.45vw, 0.9rem);
  letter-spacing: 0.05em;
}

.detail-grid {
  display: grid;
  gap: 0.28rem;
  margin: 0;
}

.detail-grid div {
  display: flex;
  justify-content: space-between;
  gap: 0.4rem;
  padding-top: 0.22rem;
  border-top: 1px solid var(--halo-hairline);
}

.detail-grid dt,
.detail-grid dd {
  margin: 0;
  font-size: clamp(0.36rem, 0.82vw, 0.5rem);
}

.detail-grid dt {
  color: var(--halo-text-muted);
}

.detail-grid dd {
  color: var(--halo-text);
  font-family: var(--halo-font-mono);
  text-align: right;
}

.detail-empty {
  align-items: center;
  color: var(--halo-text-muted);
  text-align: center;
}

.empty-reticle {
  width: 1.8rem;
  aspect-ratio: 1;
  margin-bottom: 0.5rem;
  border: 1px solid var(--halo-metal);
  border-radius: 50%;
  background: radial-gradient(
    circle,
    var(--halo-focus) 0 1px,
    transparent 2px
  );
}

.detail-empty strong {
  color: var(--halo-text);
  font-size: clamp(0.55rem, 1.2vw, 0.76rem);
}

.detail-empty small {
  margin-top: 0.25rem;
  font-family: var(--halo-font-mono);
  font-size: clamp(0.32rem, 0.72vw, 0.44rem);
}

@media (max-width: 520px) {
  .central-display {
    width: 32%;
  }

  .ambient-caption {
    display: none;
  }
}

@media (prefers-reduced-motion: reduce) {
  .halo-glyph,
  .status-pip {
    animation: none;
    transition: none;
  }
}
</style>
