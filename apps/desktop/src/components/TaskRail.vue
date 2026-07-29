<script setup lang="ts">
import { computed, ref } from "vue";
import type {
  ActiveDrag,
  Confidence,
  RingSlot,
  SignalSource,
  TaskDragOrigin,
  TaskRecord,
  TaskStatus,
} from "../types/halo";

const props = withDefaults(
  defineProps<{
    slots: RingSlot[];
    tasks: TaskRecord[];
    queue: TaskRecord[];
    selectedSlot?: number | null;
    nowMs?: number;
  }>(),
  {
    selectedSlot: null,
    nowMs: () => Date.now(),
  },
);

const emit = defineEmits<{
  select: [slot: number];
  "select-task": [taskKey: string];
  dragstart: [drag: ActiveDrag];
  dragend: [];
}>();

const railExpanded = ref(false);

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

const EMPTY_SLOT: Omit<RingSlot, "index"> = {
  taskKey: null,
  status: "idle",
  source: null,
  confidence: null,
  bindingMode: "auto",
  locked: false,
  effect: {
    brightness: 80,
    speedPercent: 100,
    direction: "clockwise",
    tailPercent: 35,
  },
};

const normalizedSlots = computed(() =>
  Array.from({ length: 4 }, (_, index) => {
    const slot =
      props.slots.find((candidate) => candidate.index === index) ?? {
        ...EMPTY_SLOT,
        index,
        effect: { ...EMPTY_SLOT.effect },
      };
    const task = slot.taskKey
      ? props.tasks.find((candidate) => candidate.taskKey === slot.taskKey)
      : null;

    return {
      slot,
      lastActiveAtMs: task?.lastActiveAtMs ?? null,
    };
  }),
);

function statusClass(status: TaskStatus) {
  return `status-${status.replace(
    /[A-Z]/g,
    (letter) => `-${letter.toLowerCase()}`,
  )}`;
}

function activityLabel(lastActiveAtMs: number | null) {
  if (lastActiveAtMs === null) {
    return "暂无活动";
  }

  const elapsedMs = Math.max(0, props.nowMs - lastActiveAtMs);
  if (elapsedMs < 1_000) {
    return "刚刚";
  }
  if (elapsedMs < 60_000) {
    return `${Math.floor(elapsedMs / 1_000)} 秒前`;
  }
  if (elapsedMs < 3_600_000) {
    return `${Math.floor(elapsedMs / 60_000)} 分钟前`;
  }
  if (elapsedMs < 86_400_000) {
    return `${Math.floor(elapsedMs / 3_600_000)} 小时前`;
  }
  return `${Math.floor(elapsedMs / 86_400_000)} 天前`;
}

function selectSlot(slot: RingSlot) {
  if (slot.taskKey) {
    emit("select", slot.index);
    emit("select-task", slot.taskKey);
  }
}

function selectQueuedTask(taskKey: string) {
  emit("select-task", taskKey);
}

function startTaskDrag(
  event: DragEvent,
  taskKey: string,
  origin: TaskDragOrigin,
) {
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("application/x-codex-halo-drag", "task");
  }
  emit("dragstart", { kind: "task", taskKey, origin });
}

function finishTaskDrag() {
  emit("dragend");
}
</script>

<template>
  <aside
    class="task-rail"
    :class="{ 'rail-expanded': railExpanded }"
    data-task-rail
    aria-label="任务与光环状态"
  >
    <button
      class="rail-toggle"
      data-rail-toggle
      type="button"
      :aria-expanded="railExpanded"
      aria-controls="task-rail-content"
      @click="railExpanded = !railExpanded"
    >
      <span>任务轨</span>
      <span aria-hidden="true">{{ railExpanded ? "←" : "→" }}</span>
    </button>

    <div id="task-rail-content" class="task-rail-content" data-rail-content>
      <header class="rail-header">
        <div>
          <span class="rail-kicker">CHANNEL MAP</span>
          <h2>任务轨</h2>
        </div>
        <span class="rail-count">{{ tasks.length.toString().padStart(2, "0") }}</span>
      </header>

      <ol class="bound-list" aria-label="四圈绑定状态">
        <li v-for="{ slot, lastActiveAtMs } in normalizedSlots" :key="slot.index">
          <button
            class="task-row"
            :class="[statusClass(slot.status), { selected: selectedSlot === slot.index }]"
            :data-task-slot="slot.index"
            type="button"
            :disabled="slot.taskKey === null"
            :draggable="slot.taskKey !== null"
            :aria-current="selectedSlot === slot.index ? 'true' : undefined"
            :aria-label="slot.taskKey ? `选择第 ${slot.index + 1} 圈` : `第 ${slot.index + 1} 圈未绑定`"
            @click="selectSlot(slot)"
            @dragstart="
              slot.taskKey &&
                startTaskDrag($event, slot.taskKey, {
                  kind: 'slot',
                  slot: slot.index,
                })
            "
            @dragend="finishTaskDrag"
          >
            <span class="row-index">{{ String(slot.index + 1).padStart(2, "0") }}</span>
            <span class="row-core">
              <span class="row-title">
                <strong>RING {{ String(slot.index + 1).padStart(2, "0") }}</strong>
                <span v-if="slot.locked" class="lock-mark">LOCKED</span>
              </span>
              <span class="row-status">
                <i aria-hidden="true" />
                {{ slot.taskKey ? STATUS_LABELS[slot.status] : "未绑定" }}
              </span>
              <span class="row-meta">
                <span>{{ slot.source ? SOURCE_LABELS[slot.source] : "无来源" }}</span>
                <span>
                  {{
                    slot.confidence
                      ? CONFIDENCE_LABELS[slot.confidence]
                      : "无可信度"
                  }}
                </span>
                <time>{{ activityLabel(lastActiveAtMs) }}</time>
              </span>
            </span>
          </button>
        </li>
      </ol>

      <section class="queue-section" aria-labelledby="queue-heading">
        <header>
          <h3 id="queue-heading">等待队列</h3>
          <span>{{ queue.length }} WAITING</span>
        </header>
        <ol class="queue-list">
          <li
            v-for="(queuedTask, index) in queue"
            :key="queuedTask.taskKey"
            class="queue-row status-queued"
            data-queue-task
          >
            <span class="queue-order">{{ String(index + 1).padStart(2, "0") }}</span>
            <button
              type="button"
              draggable="true"
              aria-label="选择等待队列中的匿名任务"
              @click="selectQueuedTask(queuedTask.taskKey)"
              @dragstart="
                startTaskDrag($event, queuedTask.taskKey, { kind: 'queue' })
              "
              @dragend="finishTaskDrag"
            >
              <strong>QUEUE {{ String(index + 1).padStart(2, "0") }}</strong>
              <small>
                排队等待 · {{ SOURCE_LABELS[queuedTask.source] }} ·
                {{ CONFIDENCE_LABELS[queuedTask.confidence] }}
              </small>
            </button>
            <time>{{ activityLabel(queuedTask.lastActiveAtMs) }}</time>
          </li>
        </ol>
        <p v-if="queue.length === 0" class="queue-empty">NO TASKS IN BUFFER</p>
      </section>
    </div>
  </aside>
</template>

<style scoped>
.task-rail {
  color: var(--halo-text);
  background:
    linear-gradient(90deg, var(--halo-glass-sheen), transparent 30%),
    var(--halo-canvas-raised);
}

.rail-toggle {
  position: absolute;
  z-index: 4;
  top: 1rem;
  right: -3.2rem;
  display: none;
  width: 3.2rem;
  min-height: 6.6rem;
  padding: 0.65rem 0.45rem;
  border: 1px solid var(--halo-hairline);
  border-left: 0;
  border-radius: 0 0.6rem 0.6rem 0;
  color: var(--halo-focus);
  background: var(--halo-canvas-raised);
  font-family: var(--halo-font-mono);
  font-size: 0.62rem;
  letter-spacing: 0.08em;
  writing-mode: vertical-rl;
  cursor: pointer;
}

.task-rail-content {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  padding: 1.25rem 1rem 1rem;
  overflow: auto;
  scrollbar-color: var(--halo-metal-dark) transparent;
}

.rail-header,
.queue-section header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 1rem;
}

.rail-header {
  padding: 0 0.35rem 1rem;
  border-bottom: 1px solid var(--halo-hairline);
}

.rail-kicker,
.rail-count,
.queue-section header span {
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.6rem;
  letter-spacing: 0.12em;
}

.rail-header h2,
.queue-section h3 {
  margin: 0;
  font-weight: 500;
}

.rail-header h2 {
  margin-top: 0.15rem;
  font-size: 1.1rem;
  letter-spacing: 0.08em;
}

.rail-count {
  color: var(--halo-focus);
  font-size: 0.72rem;
}

.bound-list,
.queue-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.bound-list {
  display: grid;
  gap: 0.35rem;
  padding: 0.8rem 0;
}

.task-row {
  position: relative;
  display: grid;
  width: 100%;
  grid-template-columns: 1.65rem 1fr;
  gap: 0.72rem;
  padding: 0.7rem 0.65rem;
  border: 1px solid transparent;
  border-radius: 0.45rem;
  color: inherit;
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition:
    border-color 140ms ease,
    background 140ms ease;
}

.task-row:not(:disabled):hover,
.task-row:not(:disabled):focus-visible,
.task-row.selected {
  border-color: var(--halo-hairline);
  outline: none;
  background: var(--halo-glass-sheen);
}

.task-row.selected::before {
  position: absolute;
  top: 0.65rem;
  bottom: 0.65rem;
  left: -1px;
  width: 2px;
  background: var(--halo-focus);
  box-shadow: 0 0 0.5rem var(--halo-unknown-glow);
  content: "";
}

.task-row:disabled {
  cursor: default;
  opacity: 0.5;
}

.row-index,
.queue-order {
  color: var(--halo-metal);
  font-family: var(--halo-font-mono);
  font-size: 0.68rem;
}

.row-index {
  padding-top: 0.1rem;
  border-right: 1px solid var(--halo-hairline);
}

.row-core {
  display: grid;
  gap: 0.28rem;
  min-width: 0;
}

.row-title,
.row-status,
.row-meta {
  display: flex;
  align-items: center;
}

.row-title {
  justify-content: space-between;
  gap: 0.4rem;
}

.row-title strong,
.queue-row strong {
  font-family: var(--halo-font-mono);
  font-size: 0.72rem;
  font-weight: 560;
  letter-spacing: 0.08em;
}

.lock-mark {
  color: var(--halo-waiting);
  font-family: var(--halo-font-mono);
  font-size: 0.46rem;
  letter-spacing: 0.08em;
}

.row-status {
  gap: 0.4rem;
  color: var(--halo-text);
  font-size: 0.72rem;
}

.row-status i {
  width: 0.36rem;
  aspect-ratio: 1;
  border-radius: 50%;
  background: var(--status-color, var(--halo-idle));
  box-shadow: 0 0 0.4rem var(--status-color, var(--halo-idle));
}

.row-meta {
  flex-wrap: wrap;
  gap: 0.25rem 0.45rem;
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.52rem;
}

.row-meta span + span::before,
.row-meta time::before {
  margin-right: 0.45rem;
  color: var(--halo-metal);
  content: "/";
}

.status-running {
  --status-color: var(--halo-running);
}

.status-waiting {
  --status-color: var(--halo-waiting);
}

.status-round-completed {
  --status-color: var(--halo-round-completed);
}

.status-failed {
  --status-color: var(--halo-failed);
}

.status-queued {
  --status-color: var(--halo-queued);
}

.status-unknown {
  --status-color: var(--halo-unknown);
}

.status-idle {
  --status-color: var(--halo-idle);
}

.queue-section {
  margin-top: auto;
  padding: 0.9rem 0.35rem 0;
  border-top: 1px solid var(--halo-hairline);
}

.queue-section h3 {
  font-size: 0.76rem;
  letter-spacing: 0.08em;
}

.queue-section header span {
  color: var(--halo-queued);
}

.queue-list {
  display: grid;
  gap: 0.35rem;
  padding-top: 0.65rem;
}

.queue-row {
  display: grid;
  grid-template-columns: 1.65rem 1fr auto;
  align-items: center;
  gap: 0.55rem;
  padding: 0.58rem;
  border-left: 2px solid var(--status-color);
  background: var(--halo-glass-sheen);
}

.queue-row button {
  display: grid;
  gap: 0.15rem;
  min-width: 0;
  padding: 0;
  border: 0;
  color: inherit;
  background: transparent;
  text-align: left;
  cursor: grab;
}

.queue-row button:focus-visible {
  outline: 1px solid var(--halo-focus);
  outline-offset: 0.2rem;
}

.queue-row small,
.queue-row time {
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.48rem;
}

.queue-empty {
  margin: 0.75rem 0 0;
  color: var(--halo-metal);
  font-family: var(--halo-font-mono);
  font-size: 0.54rem;
  letter-spacing: 0.08em;
}

@media (forced-colors: active) {
  .task-row,
  .rail-toggle {
    border-color: ButtonText;
    color: ButtonText;
    background: Canvas;
  }

  .task-row:focus-visible,
  .rail-toggle:focus-visible {
    outline: 2px solid Highlight;
    outline-offset: 2px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .task-row,
  .rail-toggle {
    transition: none;
  }
}
</style>
