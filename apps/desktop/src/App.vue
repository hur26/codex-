<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import ActivityStrip from "./components/ActivityStrip.vue";
import CentralDisplay from "./components/CentralDisplay.vue";
import CrownControl from "./components/CrownControl.vue";
import HaloPreview from "./components/HaloPreview.vue";
import TaskRail from "./components/TaskRail.vue";
import { createHaloStore } from "./stores/haloStore";
import type {
  AdapterState,
  DisplayMode,
  EffectProfile,
  RingSlot,
} from "./types/halo";

const store = createHaloStore();
const selectedSlot = ref<number | null>(null);
const displayMode = ref<DisplayMode>("ambient");
const renderedAtMs = ref(Date.now());
let activityClock: number | null = null;

const ADAPTER_LABELS: Record<AdapterState, string> = {
  online: "ONLINE",
  degraded: "DEGRADED",
  offline: "OFFLINE",
};

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
const occupiedCount = computed(
  () => slots.value.filter((slot) => slot.taskKey !== null).length,
);

function selectSlot(slot: number) {
  if (slot < 0 || slot > 3) {
    return;
  }
  selectedSlot.value = slot;
}

function updateDisplayMode(mode: DisplayMode) {
  displayMode.value = mode;
}

onMounted(() => {
  renderedAtMs.value = Date.now();
  activityClock = window.setInterval(() => {
    renderedAtMs.value = Date.now();
  }, 30_000);
  void store.load();
  void store.refreshAdapterStatus();
  void store.start();
});

onUnmounted(() => {
  if (activityClock !== null) {
    window.clearInterval(activityClock);
    activityClock = null;
  }
  void store.stop();
});
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
        :class="`adapter-${store.state.adapterStatus.state}`"
        :data-adapter-state="store.state.adapterStatus.state"
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
      v-if="store.state.error"
      class="system-notice error-notice"
      data-app-error
      role="alert"
    >
      <i aria-hidden="true" />
      {{ store.state.error.message }}
    </div>

    <main class="app-main">
      <TaskRail
        :slots="slots"
        :tasks="tasks"
        :queue="queue"
        :selected-slot="selectedSlot"
        :now-ms="renderedAtMs"
        @select="selectSlot"
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
            :selected-slot="selectedSlot"
            @select="selectSlot"
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
