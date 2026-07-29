<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import ActivityStrip from "./components/ActivityStrip.vue";
import BindingControls from "./components/BindingControls.vue";
import CentralDisplay from "./components/CentralDisplay.vue";
import CrownControl from "./components/CrownControl.vue";
import EffectEditor from "./components/EffectEditor.vue";
import HaloPreview from "./components/HaloPreview.vue";
import TaskRail from "./components/TaskRail.vue";
import {
  createHaloStore,
  type HaloStoreError,
} from "./stores/haloStore";
import type {
  ActiveDrag,
  AdapterState,
  DisplayMode,
  EffectProfile,
  RingSlot,
  UpdateEffectInput,
} from "./types/halo";

const store = createHaloStore();
const selectedSlot = ref<number | null>(null);
const selectedTaskKey = ref<string | null>(null);
const displayMode = ref<DisplayMode>("ambient");
const renderedAtMs = ref(Date.now());
const activeDrag = ref<ActiveDrag | null>(null);
const bindingCommandPending = ref(false);
const effectCommandPending = ref(false);
const effectBatchError = ref<HaloStoreError | null>(null);
let activityClock: number | null = null;
let effectQueueRunning = false;
let effectQueueMounted = true;
let mountedGeneration = 0;
let pendingGlobalBrightness: number | null = null;
const pendingEffects = new Map<number, UpdateEffectInput>();

const ADAPTER_LABELS: Record<AdapterState, string> = {
  online: "ONLINE",
  degraded: "DEGRADED",
  offline: "OFFLINE",
};
const ADAPTER_CLASSES: Record<AdapterState, string> = {
  online: "adapter-online",
  degraded: "adapter-degraded",
  offline: "adapter-offline",
};
const adapterDiagnosticTone = computed(() =>
  store.state.adapterStatus.state === "online" ? "nominal" : "muted-blue",
);
const adapterDiagnosticLabel = computed(() => {
  const status = store.state.adapterStatus;
  const detail = status.message ? `，${status.message}` : "";
  return `适配器诊断：${ADAPTER_LABELS[status.state]}，${status.mode.toUpperCase()}${detail}`;
});

const EMPTY_EFFECT: EffectProfile = {
  brightness: 80,
  speedPercent: 100,
  direction: "clockwise",
  tailPercent: 35,
};

const emptySlots = Array.from({ length: 4 }, (_, index): RingSlot => ({
  index,
  taskKey: null,
  status: "idle",
  source: null,
  confidence: null,
  bindingMode: "auto",
  locked: false,
  effect: { ...EMPTY_EFFECT },
}));

const slots = computed(() => store.state.snapshot?.slots ?? emptySlots);
const tasks = computed(() => store.state.snapshot?.tasks ?? []);
const queue = computed(() => store.state.snapshot?.queue ?? []);
const selectedSlotRecord = computed(
  () =>
    slots.value.find((slot) => slot.index === selectedSlot.value) ?? null,
);
const selectedTask = computed(
  () =>
    tasks.value.find((task) => task.taskKey === selectedTaskKey.value) ?? null,
);
const occupiedCount = computed(
  () => slots.value.filter((slot) => slot.taskKey !== null).length,
);
const visibleAppError = computed(
  () => store.state.error ?? effectBatchError.value,
);

function selectSlot(slot: number) {
  if (slot < 0 || slot > 3) {
    return;
  }
  selectedSlot.value = slot;
  const taskKey =
    slots.value.find((candidate) => candidate.index === slot)?.taskKey ?? null;
  if (taskKey) {
    selectedTaskKey.value = taskKey;
  }
}

function selectTask(taskKey: string) {
  if (!tasks.value.some((task) => task.taskKey === taskKey)) {
    return;
  }
  selectedTaskKey.value = taskKey;
  selectedSlot.value =
    slots.value.find((slot) => slot.taskKey === taskKey)?.index ?? null;
}

function taskDragSourceExists(
  drag: Extract<ActiveDrag, { kind: "task" }>,
) {
  if (!tasks.value.some((task) => task.taskKey === drag.taskKey)) {
    return false;
  }
  if (drag.origin.kind === "queue") {
    return queue.value.some((task) => task.taskKey === drag.taskKey);
  }
  const originSlot = drag.origin.slot;
  return slots.value.some(
    (slot) =>
      slot.index === originSlot && slot.taskKey === drag.taskKey,
  );
}

function beginDrag(drag: ActiveDrag) {
  if (
    (drag.kind === "task" && !taskDragSourceExists(drag)) ||
    (drag.kind === "slot" &&
      !slots.value.some(
        (slot) =>
          slot.index === drag.slot &&
          slot.taskKey !== null &&
          slot.taskKey === drag.taskKey,
      ))
  ) {
    return;
  }
  activeDrag.value = drag;
}

function clearActiveDrag() {
  activeDrag.value = null;
}

function handleGlobalKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    clearActiveDrag();
  }
}

async function manualBind(taskKey: string, slot: number) {
  if (
    bindingCommandPending.value ||
    slot < 0 ||
    slot > 3 ||
    !tasks.value.some((task) => task.taskKey === taskKey)
  ) {
    return;
  }
  if (
    slots.value.some(
      (candidate) =>
        candidate.index === slot && candidate.taskKey === taskKey,
    )
  ) {
    return;
  }
  bindingCommandPending.value = true;
  try {
    const snapshot = await store.manualBind({ taskKey, slot, lock: false });
    if (snapshot) {
      selectedTaskKey.value = taskKey;
      selectedSlot.value = slot;
    }
  } finally {
    bindingCommandPending.value = false;
  }
}

async function toggleLock(slot: number) {
  if (bindingCommandPending.value || slot < 0 || slot > 3) {
    return;
  }
  bindingCommandPending.value = true;
  try {
    await store.toggleLock(slot);
  } finally {
    bindingCommandPending.value = false;
  }
}

async function dropOnSlot(slot: number) {
  const drag = activeDrag.value;
  activeDrag.value = null;
  if (!drag || bindingCommandPending.value || slot < 0 || slot > 3) {
    return;
  }

  if (drag.kind === "task") {
    await manualBind(drag.taskKey, slot);
    return;
  }
  if (drag.slot === slot) {
    return;
  }
  const source = slots.value.find((candidate) => candidate.index === drag.slot);
  if (!source?.taskKey || source.taskKey !== drag.taskKey) {
    return;
  }

  bindingCommandPending.value = true;
  try {
    const snapshot = await store.swapSlots(drag.slot, slot);
    if (snapshot) {
      selectedSlot.value = slot;
    }
  } finally {
    bindingCommandPending.value = false;
  }
}

function updateDisplayMode(mode: DisplayMode) {
  displayMode.value = mode;
}

function setGlobalBrightness(value: number) {
  beginEffectBatch();
  pendingGlobalBrightness = value;
  void drainEffectQueue();
}

function updateEffect(input: UpdateEffectInput) {
  beginEffectBatch();
  pendingEffects.set(input.slot, { ...input });
  void drainEffectQueue();
}

function beginEffectBatch() {
  if (
    !effectQueueRunning &&
    pendingGlobalBrightness === null &&
    pendingEffects.size === 0
  ) {
    effectBatchError.value = null;
  }
}

async function drainEffectQueue() {
  if (effectQueueRunning || !effectQueueMounted) {
    return;
  }
  effectQueueRunning = true;
  effectCommandPending.value = true;
  try {
    while (
      effectQueueMounted &&
      (pendingGlobalBrightness !== null || pendingEffects.size > 0)
    ) {
      if (pendingGlobalBrightness !== null) {
        const value = pendingGlobalBrightness;
        pendingGlobalBrightness = null;
        const result = await store.setGlobalBrightness(value);
        if (!result && store.state.error) {
          effectBatchError.value = { ...store.state.error };
        }
        continue;
      }

      const next = pendingEffects.entries().next().value as
        | [number, UpdateEffectInput]
        | undefined;
      if (!next) {
        continue;
      }
      pendingEffects.delete(next[0]);
      const result = await store.updateEffect(next[1]);
      if (!result && store.state.error) {
        effectBatchError.value = { ...store.state.error };
      }
    }
  } finally {
    effectQueueRunning = false;
    effectCommandPending.value = false;
    if (
      effectQueueMounted &&
      (pendingGlobalBrightness !== null || pendingEffects.size > 0)
    ) {
      void drainEffectQueue();
    }
  }
}

async function initializeStore(generation: number) {
  const subscribed = await store.start();
  if (
    !subscribed ||
    !effectQueueMounted ||
    mountedGeneration !== generation
  ) {
    return;
  }
  await Promise.all([store.load(), store.refreshAdapterStatus()]);
}

onMounted(() => {
  effectQueueMounted = true;
  mountedGeneration += 1;
  const generation = mountedGeneration;
  renderedAtMs.value = Date.now();
  activityClock = window.setInterval(() => {
    renderedAtMs.value = Date.now();
  }, 30_000);
  void initializeStore(generation);
  window.addEventListener("blur", clearActiveDrag);
  window.addEventListener("keydown", handleGlobalKeydown);
});

onUnmounted(() => {
  effectQueueMounted = false;
  mountedGeneration += 1;
  pendingGlobalBrightness = null;
  pendingEffects.clear();
  clearActiveDrag();
  window.removeEventListener("blur", clearActiveDrag);
  window.removeEventListener("keydown", handleGlobalKeydown);
  if (activityClock !== null) {
    window.clearInterval(activityClock);
    activityClock = null;
  }
  void store.stop();
});

watch(
  () => store.state.snapshot,
  () => {
    const drag = activeDrag.value;
    if (!drag) {
      return;
    }
    if (
      (drag.kind === "task" && !taskDragSourceExists(drag)) ||
      (drag.kind === "slot" &&
        !slots.value.some(
          (slot) =>
            slot.index === drag.slot && slot.taskKey === drag.taskKey,
        ))
    ) {
      clearActiveDrag();
    }
  },
  { flush: "sync" },
);
</script>

<template>
  <div class="app-shell">
    <header class="app-header" data-app-header>
      <div class="brand-lockup">
        <span class="brand-glyph" aria-hidden="true">
          <i />
          <i />
          <i />
          <i />
        </span>
        <div>
          <span class="system-name">CODEX HALO / CONTROL ARRAY</span>
          <h1>VIRTUAL DEVICE</h1>
        </div>
      </div>

      <div class="header-readouts" aria-label="虚拟设备摘要">
        <div>
          <span>CHANNELS</span>
          <strong>{{ occupiedCount }} / 4</strong>
        </div>
        <div>
          <span>BRIGHTNESS</span>
          <strong>{{ store.state.snapshot?.globalBrightness ?? "--" }}%</strong>
        </div>
      </div>

      <div
        class="adapter-state"
        :class="ADAPTER_CLASSES[store.state.adapterStatus.state]"
        :data-adapter-state="store.state.adapterStatus.state"
        :data-diagnostic-tone="adapterDiagnosticTone"
        :aria-label="adapterDiagnosticLabel"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        :title="store.state.adapterStatus.message ?? undefined"
      >
        <i aria-hidden="true" />
        <span>
          <small>ADAPTER</small>
          <strong>{{ ADAPTER_LABELS[store.state.adapterStatus.state] }}</strong>
        </span>
        <em>{{ store.state.adapterStatus.mode.toUpperCase() }}</em>
      </div>
    </header>

    <div
      v-if="store.state.loading"
      class="system-notice loading-notice"
      data-loading
      role="status"
    >
      <i aria-hidden="true" />
      正在同步虚拟设备快照
    </div>
    <div
      v-if="visibleAppError"
      class="system-notice error-notice"
      data-app-error
      role="alert"
    >
      <i aria-hidden="true" />
      {{ visibleAppError.message }}
    </div>

    <main class="app-main">
      <TaskRail
        :slots="slots"
        :tasks="tasks"
        :queue="queue"
        :selected-slot="selectedSlot"
        :now-ms="renderedAtMs"
        @select="selectSlot"
        @select-task="selectTask"
        @dragstart="beginDrag"
        @dragend="clearActiveDrag"
      />

      <section class="device-workspace" aria-label="虚拟设备工作区">
        <div class="workspace-axis axis-horizontal" aria-hidden="true" />
        <div class="workspace-axis axis-vertical" aria-hidden="true" />

        <div class="device-stage">
          <span class="device-coordinate coordinate-top" aria-hidden="true">
            000°
          </span>
          <span class="device-coordinate coordinate-right" aria-hidden="true">
            090°
          </span>
          <span class="device-coordinate coordinate-bottom" aria-hidden="true">
            180°
          </span>

          <HaloPreview
            :slots="slots"
            :global-brightness="
              store.state.snapshot?.globalBrightness ?? 100
            "
            :selected-slot="selectedSlot"
            :drag-active="activeDrag !== null"
            :drag-kind="activeDrag?.kind ?? null"
            @select="selectSlot"
            @dragstart="beginDrag"
            @drop="dropOnSlot"
            @dragend="clearActiveDrag"
          />

          <div class="device-display-layer" @click.stop>
            <CentralDisplay
              :mode="displayMode"
              :slots="slots"
              :selected-slot="selectedSlot"
            />
          </div>

          <CrownControl
            :mode="displayMode"
            :selected-slot="selectedSlot"
            @select="selectSlot"
            @update:mode="updateDisplayMode"
          />
        </div>

        <BindingControls
          :selected-task="selectedTask"
          :selected-slot="selectedSlotRecord"
          :loading="store.state.loading || bindingCommandPending"
          @bind="manualBind"
          @toggle-lock="toggleLock"
        />

        <EffectEditor
          :global-brightness="
            store.state.snapshot?.globalBrightness ?? 100
          "
          :selected-slot="selectedSlotRecord"
          :pending="effectCommandPending"
          @set-global-brightness="setGlobalBrightness"
          @update-effect="updateEffect"
        />

        <footer class="device-caption">
          <div>
            <span>SIMULATION SURFACE</span>
            <strong>四通道同心状态阵列</strong>
          </div>
          <p>
            中央屏 {{ displayMode.toUpperCase() }}
            <span aria-hidden="true">/</span>
            选中 {{ selectedSlot === null ? "--" : `R0${selectedSlot + 1}` }}
          </p>
        </footer>
      </section>
    </main>

    <ActivityStrip
      :tasks="tasks"
      :slots="slots"
      :queue="queue"
      :now-ms="renderedAtMs"
    />
  </div>
</template>
