<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

interface VirtualSnapshot {
  deviceMode: "virtual";
  globalBrightness: number;
  slots: Array<{
    index: number;
    taskKey: string | null;
  }>;
}

const snapshot = ref<VirtualSnapshot | null>(null);
const errorMessage = ref("");
const isLoading = ref(false);

const occupiedRings = computed(
  () => snapshot.value?.slots.filter((slot) => slot.taskKey !== null).length ?? 0,
);

async function loadSnapshot() {
  isLoading.value = true;
  errorMessage.value = "";

  try {
    snapshot.value = await invoke<VirtualSnapshot>("get_snapshot");
  } catch {
    errorMessage.value = "暂时无法连接虚拟设备";
  } finally {
    isLoading.value = false;
  }
}

onMounted(loadSnapshot);
</script>

<template>
  <main class="shell">
    <section class="device-card" aria-live="polite">
      <header>
        <span class="eyebrow">CODEX HALO</span>
        <span class="mode">{{ snapshot?.deviceMode ?? "virtual" }} device</span>
      </header>

      <div class="halo-mark" aria-hidden="true">
        <i v-for="ring in 4" :key="ring" />
      </div>

      <h1>VIRTUAL DEVICE</h1>
      <p v-if="snapshot" class="summary">
        {{ snapshot.slots.length }} 个光环已就绪 · {{ occupiedRings }} 个任务已绑定
      </p>
      <p v-else-if="errorMessage" class="error">{{ errorMessage }}</p>
      <p v-else class="summary">正在读取虚拟设备快照…</p>

      <footer>
        <span>全局亮度 {{ snapshot?.globalBrightness ?? "--" }}%</span>
        <button type="button" :disabled="isLoading" @click="loadSnapshot">
          {{ isLoading ? "读取中" : "刷新快照" }}
        </button>
      </footer>
    </section>
  </main>
</template>

<style scoped>
:global(*) {
  box-sizing: border-box;
}

:global(body) {
  min-width: 320px;
  min-height: 100vh;
  margin: 0;
  color: #e8ecec;
  background:
    radial-gradient(circle at 50% 42%, #202727 0, #0d1112 45%, #070909 100%);
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
}

button {
  border: 1px solid #3c4545;
  border-radius: 999px;
  padding: 0.6rem 1rem;
  color: #d7dddd;
  background: #151a1a;
  cursor: pointer;
}

button:disabled {
  cursor: wait;
  opacity: 0.55;
}

.shell {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 2rem;
}

.device-card {
  width: min(34rem, 100%);
  border: 1px solid #303737;
  border-radius: 1.5rem;
  padding: 1.25rem 1.5rem 1.5rem;
  text-align: center;
  background: rgb(12 15 15 / 88%);
  box-shadow: 0 1.8rem 5rem rgb(0 0 0 / 42%);
}

header,
footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}

footer {
  margin-top: 1.5rem;
  color: #849090;
  font-size: 0.82rem;
}

.eyebrow,
.mode {
  color: #879292;
  font-family: "JetBrains Mono", monospace;
  font-size: 0.67rem;
  letter-spacing: 0.16em;
  text-transform: uppercase;
}

.mode {
  color: #70c8c1;
}

.halo-mark {
  position: relative;
  width: 12rem;
  aspect-ratio: 1;
  margin: 2.5rem auto 1.75rem;
}

.halo-mark i {
  position: absolute;
  inset: calc((var(--ring) - 1) * 1.15rem);
  border: 2px solid rgb(242 185 74 / calc(1 - (var(--ring) - 1) * 0.16));
  border-radius: 50%;
  box-shadow: 0 0 1rem rgb(242 185 74 / 18%);
}

.halo-mark i:nth-child(1) {
  --ring: 1;
}

.halo-mark i:nth-child(2) {
  --ring: 2;
}

.halo-mark i:nth-child(3) {
  --ring: 3;
}

.halo-mark i:nth-child(4) {
  --ring: 4;
}

h1 {
  margin: 0;
  font-size: clamp(1.6rem, 5vw, 2.4rem);
  font-weight: 500;
  letter-spacing: 0.08em;
}

.summary,
.error {
  min-height: 1.5rem;
  margin: 0.65rem 0 0;
  color: #8f9999;
}

.error {
  color: #ff8a7c;
}
</style>
