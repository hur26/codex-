<script setup lang="ts">
import { computed } from "vue";
import type { RingSlot, TaskRecord, TaskStatus } from "../types/halo";

const props = withDefaults(
  defineProps<{
    tasks: TaskRecord[];
    slots: RingSlot[];
    queue: TaskRecord[];
    nowMs?: number;
  }>(),
  {
    nowMs: () => Date.now(),
  },
);

const STATUS_LABELS: Record<TaskStatus, string> = {
  running: "正在执行",
  waiting: "等待确认",
  roundCompleted: "本轮完成",
  failed: "模拟故障",
  queued: "排队等待",
  idle: "空闲",
  unknown: "状态未知",
};

const recentEvents = computed(() =>
  [...props.tasks]
    .sort(
      (left, right) =>
        right.lastActiveAtMs - left.lastActiveAtMs ||
        left.taskKey.localeCompare(right.taskKey),
    )
    .slice(0, 6)
    .map((task, anonymousIndex) => {
      const slot = props.slots.find(
        (candidate) => candidate.taskKey === task.taskKey,
      );
      const queueIndex = props.queue.findIndex(
        (candidate) => candidate.taskKey === task.taskKey,
      );
      const label = slot
        ? `R${String(slot.index + 1).padStart(2, "0")}`
        : queueIndex >= 0
          ? `Q${String(queueIndex + 1).padStart(2, "0")}`
          : `U${String(anonymousIndex + 1).padStart(2, "0")}`;

      return {
        label,
        status: queueIndex >= 0 ? ("queued" as const) : task.status,
        source: task.source === "hook" ? "HOOK" : "SIM",
        lastActiveAtMs: task.lastActiveAtMs,
      };
    }),
);

function statusClass(status: TaskStatus) {
  return `status-${status.replace(
    /[A-Z]/g,
    (letter) => `-${letter.toLowerCase()}`,
  )}`;
}

function activityLabel(lastActiveAtMs: number) {
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
</script>

<template>
  <footer
    class="activity-strip"
    data-activity-strip
    aria-label="最近任务活动"
    aria-live="polite"
  >
    <div class="activity-title">
      <span>ACTIVITY BUS</span>
      <strong>最近活动</strong>
    </div>

    <ol v-if="recentEvents.length" class="activity-events">
      <li
        v-for="(event, index) in recentEvents"
        :key="index"
        class="activity-event"
        :class="statusClass(event.status)"
        data-activity-event
      >
        <i aria-hidden="true" />
        <span class="event-channel">{{ event.label }}</span>
        <span class="event-status">{{ STATUS_LABELS[event.status] }}</span>
        <span class="event-source">{{ event.source }}</span>
        <time>{{ activityLabel(event.lastActiveAtMs) }}</time>
      </li>
    </ol>

    <div v-else class="activity-empty">
      <i aria-hidden="true" />
      <span>等待状态信号</span>
    </div>

    <span class="activity-clock" aria-hidden="true">LIVE / 250 MS</span>
  </footer>
</template>

<style scoped>
.activity-strip {
  position: relative;
  z-index: 60;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 1.4rem;
  min-height: 4.2rem;
  padding: 0.55rem 1.3rem;
  border-top: 1px solid var(--halo-hairline);
  background:
    linear-gradient(90deg, var(--halo-glass-sheen), transparent 24%),
    var(--halo-canvas-raised);
}

.activity-title {
  display: grid;
  min-width: 8.5rem;
  gap: 0.12rem;
}

.activity-title span,
.activity-clock,
.event-channel,
.event-source,
.activity-event time {
  font-family: var(--halo-font-mono);
}

.activity-title span,
.activity-clock {
  color: var(--halo-text-muted);
  font-size: 0.52rem;
  letter-spacing: 0.12em;
}

.activity-title strong {
  font-size: 0.72rem;
  font-weight: 500;
  letter-spacing: 0.08em;
}

.activity-events {
  position: relative;
  display: flex;
  min-width: 0;
  gap: 0.4rem;
  margin: 0;
  padding: 0;
  overflow: hidden;
  list-style: none;
}

.activity-events::before {
  position: absolute;
  z-index: -1;
  top: 0.6rem;
  right: 0;
  left: 0;
  height: 1px;
  background: var(--halo-hairline);
  content: "";
}

.activity-event {
  --status-color: var(--halo-idle);

  display: grid;
  min-width: 7.4rem;
  grid-template-columns: auto 1fr;
  grid-template-areas:
    "pip channel"
    "pip status"
    "pip meta";
  gap: 0.05rem 0.4rem;
  padding: 0.22rem 0.55rem 0.22rem 0;
  background: linear-gradient(90deg, var(--halo-canvas-raised) 82%, transparent);
}

.activity-event i,
.activity-empty i {
  width: 0.4rem;
  aspect-ratio: 1;
  border-radius: 50%;
  background: var(--status-color);
  box-shadow: 0 0 0.45rem var(--status-color);
}

.activity-event i {
  grid-area: pip;
  align-self: start;
  margin-top: 0.18rem;
}

.event-channel {
  grid-area: channel;
  color: var(--halo-text);
  font-size: 0.6rem;
  font-weight: 600;
  letter-spacing: 0.08em;
}

.event-status {
  grid-area: status;
  color: var(--halo-text-muted);
  font-size: 0.58rem;
}

.event-source {
  grid-area: meta;
  color: var(--halo-metal);
  font-size: 0.45rem;
}

.activity-event time {
  grid-area: meta;
  justify-self: end;
  color: var(--halo-text-muted);
  font-size: 0.45rem;
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

.activity-empty {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  color: var(--halo-text-muted);
  font-size: 0.66rem;
  letter-spacing: 0.04em;
}

.activity-empty i {
  --status-color: var(--halo-unknown);
}

@media (max-width: 760px) {
  .activity-strip {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .activity-title {
    display: none;
  }

  .activity-clock {
    font-size: 0.44rem;
  }
}
</style>
