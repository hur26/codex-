<script setup lang="ts">
import type { RingSlot, TaskKey, TaskRecord } from "../types/halo";

const props = withDefaults(
  defineProps<{
    selectedTask: TaskRecord | null;
    selectedSlot: RingSlot | null;
    loading?: boolean;
  }>(),
  {
    loading: false,
  },
);

const emit = defineEmits<{
  bind: [taskKey: TaskKey, slot: number];
  "toggle-lock": [slot: number];
}>();

function bind(slot: number) {
  if (props.loading || !props.selectedTask || slot < 0 || slot > 3) {
    return;
  }
  emit("bind", props.selectedTask.taskKey, slot);
}

function toggleLock() {
  if (
    props.loading ||
    !props.selectedSlot ||
    props.selectedSlot.taskKey === null
  ) {
    return;
  }
  emit("toggle-lock", props.selectedSlot.index);
}
</script>

<template>
  <aside
    class="binding-controls"
    data-binding-controls
    aria-label="任务圈位绑定"
  >
    <header>
      <span>CHANNEL ROUTING</span>
      <strong>绑定控制</strong>
    </header>

    <div class="selection-readout" aria-live="polite">
      <i :class="{ active: selectedTask }" aria-hidden="true" />
      <span>
        <small>SELECTED TASK</small>
        <strong>{{ selectedTask ? "匿名任务已选择" : "尚未选择任务" }}</strong>
      </span>
    </div>

    <div class="bind-grid" aria-label="绑定到圈位">
      <button
        v-for="slot in 4"
        :key="slot"
        type="button"
        :data-bind-slot="slot - 1"
        :disabled="loading || !selectedTask"
        @click="bind(slot - 1)"
      >
        <span>R{{ String(slot).padStart(2, "0") }}</span>
        <small>绑定</small>
      </button>
    </div>

    <button
      class="lock-control"
      data-lock-control
      type="button"
      :disabled="loading || !selectedSlot || selectedSlot.taskKey === null"
      @click="toggleLock"
    >
      <span aria-hidden="true">{{ selectedSlot?.locked ? "◇" : "◆" }}</span>
      {{
        selectedSlot?.locked
          ? `解除锁定 R${String(selectedSlot.index + 1).padStart(2, "0")}`
          : selectedSlot?.taskKey
            ? `锁定 R${String(selectedSlot.index + 1).padStart(2, "0")}`
            : "选择已绑定圈位"
      }}
    </button>

    <p>{{ loading ? "正在执行命令" : "拖拽或使用键盘完成路由" }}</p>
  </aside>
</template>

<style scoped>
.binding-controls {
  position: absolute;
  z-index: 40;
  top: 1rem;
  right: 1rem;
  width: 13.25rem;
  padding: 0.8rem;
  border: 1px solid var(--halo-hairline);
  border-radius: 0.45rem;
  color: var(--halo-text);
  background:
    linear-gradient(135deg, var(--halo-glass-sheen), transparent 36%),
    rgb(12 16 17 / 92%);
  box-shadow: 0 1rem 2rem var(--halo-shadow);
  backdrop-filter: blur(0.8rem);
}

header,
.selection-readout {
  display: flex;
  align-items: center;
}

header {
  justify-content: space-between;
  padding-bottom: 0.55rem;
  border-bottom: 1px solid var(--halo-hairline);
}

header span,
small,
p {
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.48rem;
  letter-spacing: 0.09em;
}

header strong {
  font-size: 0.68rem;
  font-weight: 520;
}

.selection-readout {
  gap: 0.55rem;
  padding: 0.7rem 0;
}

.selection-readout i {
  width: 0.45rem;
  aspect-ratio: 1;
  border-radius: 50%;
  background: var(--halo-idle);
}

.selection-readout i.active {
  background: var(--halo-focus);
  box-shadow: 0 0 0.55rem var(--halo-unknown-glow);
}

.selection-readout span {
  display: grid;
  gap: 0.08rem;
}

.selection-readout strong {
  font-size: 0.64rem;
  font-weight: 500;
}

.bind-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.3rem;
}

.bind-grid button,
.lock-control {
  border: 1px solid var(--halo-hairline);
  color: var(--halo-text);
  background: var(--halo-glass-sheen);
  cursor: pointer;
}

.bind-grid button {
  display: grid;
  gap: 0.14rem;
  place-items: center;
  min-height: 2.9rem;
  padding: 0.35rem 0.15rem;
  border-radius: 0.3rem;
  font-family: var(--halo-font-mono);
  font-size: 0.62rem;
}

.bind-grid button:hover:not(:disabled),
.bind-grid button:focus-visible,
.lock-control:hover:not(:disabled),
.lock-control:focus-visible {
  border-color: var(--halo-focus);
  outline: none;
  box-shadow: inset 0 0 0 1px var(--halo-focus);
}

.lock-control {
  width: 100%;
  margin-top: 0.45rem;
  padding: 0.48rem;
  border-radius: 0.3rem;
  font-family: var(--halo-font-mono);
  font-size: 0.56rem;
  letter-spacing: 0.04em;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.38;
}

p {
  margin: 0.55rem 0 0;
  text-align: right;
}

@media (max-width: 1380px) {
  .binding-controls {
    top: auto;
    right: 0.7rem;
    bottom: 0.7rem;
  }
}

@media (max-width: 700px) {
  .binding-controls {
    right: 0.5rem;
    bottom: 0.5rem;
    width: min(12rem, calc(100% - 1rem));
  }
}

@media (forced-colors: active) {
  .binding-controls,
  .bind-grid button,
  .lock-control {
    border-color: ButtonText;
    background: Canvas;
  }

  .bind-grid button:focus-visible,
  .lock-control:focus-visible {
    outline: 2px solid Highlight;
    outline-offset: 2px;
  }
}
</style>
