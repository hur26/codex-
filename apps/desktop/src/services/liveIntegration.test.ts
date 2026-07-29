import { flushPromises, mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { nextTick } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AdapterStatus,
  HaloSnapshot,
  RingSlot,
  TaskRecord,
} from "../types/halo";

const {
  getSnapshotMock,
  subscribeSnapshotsMock,
  getAdapterStatusMock,
  unlistenMock,
  fakeBridge,
  subscription,
} = vi.hoisted(() => {
  const currentSubscription = {
    listener: undefined as ((snapshot: HaloSnapshot) => void) | undefined,
  };
  const stopListening = vi.fn();
  const getSnapshot = vi.fn();
  const getAdapterStatus = vi.fn();
  const subscribeSnapshots = vi.fn(
    async (listener: (snapshot: HaloSnapshot) => void) => {
      currentSubscription.listener = listener;
      return stopListening;
    },
  );

  return {
    getSnapshotMock: getSnapshot,
    subscribeSnapshotsMock: subscribeSnapshots,
    getAdapterStatusMock: getAdapterStatus,
    unlistenMock: stopListening,
    subscription: currentSubscription,
    fakeBridge: {
      getSnapshot,
      subscribeSnapshots,
      getAdapterStatus,
      simulateSignal: vi.fn(),
      manualBind: vi.fn(),
      toggleLock: vi.fn(),
      swapSlots: vi.fn(),
      updateEffect: vi.fn(),
      setGlobalBrightness: vi.fn(),
    },
  };
});

vi.mock("./haloBridge", () => ({
  haloBridge: fakeBridge,
}));

import App from "../App.vue";
import HaloPreview from "../components/HaloPreview.vue";
import { createHaloStore } from "../stores/haloStore";
import type { HaloBridge } from "./haloBridge";

const TASK_KEY = "private-session-fingerprint";
const EMPTY_EFFECT = {
  brightness: 80,
  speedPercent: 100,
  direction: "clockwise" as const,
  tailPercent: 35,
};

function task(
  status: TaskRecord["status"],
  confidence: TaskRecord["confidence"],
): TaskRecord {
  return {
    taskKey: TASK_KEY,
    status,
    source: "hook",
    confidence,
    lastActiveAtMs: 1_000,
  };
}

function emptySlot(index: number): RingSlot {
  return {
    index,
    taskKey: null,
    status: "idle",
    source: null,
    confidence: null,
    bindingMode: "auto",
    locked: false,
    effect: { ...EMPTY_EFFECT },
  };
}

function snapshot(
  revision: number,
  status: TaskRecord["status"],
  confidence: TaskRecord["confidence"],
): HaloSnapshot {
  const record = task(status, confidence);
  return {
    revision,
    deviceMode: "virtual",
    globalBrightness: 76,
    slots: [
      {
        ...emptySlot(0),
        taskKey: TASK_KEY,
        status,
        source: "hook",
        confidence,
        bindingMode: "manual",
        locked: true,
      },
      emptySlot(1),
      emptySlot(2),
      emptySlot(3),
    ],
    tasks: [record],
    queue: [],
  };
}

const INITIAL_SNAPSHOT = snapshot(7, "running", "observed");
const DEGRADED_STATUS: AdapterStatus = {
  state: "degraded",
  mode: "hook",
  message: "探针目录暂时不可读",
  acceptedEvents: 4,
  ignoredEvents: 1,
  rejectedEvents: 0,
};
const baseStyles = readFileSync(
  resolve(process.cwd(), "src/styles/base.css"),
  "utf8",
);

describe("Hook 到四环实时集成", () => {
  beforeEach(() => {
    subscription.listener = undefined;
    getSnapshotMock.mockReset();
    getSnapshotMock.mockResolvedValue(INITIAL_SNAPSHOT);
    getAdapterStatusMock.mockReset();
    getAdapterStatusMock.mockResolvedValue(DEGRADED_STATUS);
    subscribeSnapshotsMock.mockClear();
    unlistenMock.mockClear();
  });

  afterEach(() => {
    subscription.listener = undefined;
    vi.clearAllTimers();
    vi.useRealTimers();
    document.body.innerHTML = "";
  });

  it("挂载后加载、刷新诊断并订阅，卸载后解除监听", async () => {
    vi.useFakeTimers();
    const wrapper = mount(App);
    await flushPromises();

    expect(getSnapshotMock).toHaveBeenCalledTimes(1);
    expect(getAdapterStatusMock).toHaveBeenCalledTimes(1);
    expect(subscribeSnapshotsMock).toHaveBeenCalledTimes(1);
    expect(subscription.listener).toBeTypeOf("function");

    wrapper.unmount();
    await flushPromises();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("新 Hook 快照在一轮 Vue 微任务内更新正确圆环和语义", async () => {
    const wrapper = mount(App);
    await flushPromises();

    subscription.listener?.(snapshot(8, "waiting", "provisional"));
    await nextTick();

    const waitingRing = wrapper.get('[data-slot="0"]');
    expect(waitingRing.attributes()).toMatchObject({
      "data-status": "waiting",
      "data-source": "hook",
      "data-confidence": "provisional",
    });
    expect(waitingRing.get(".confidence-marker").isVisible()).toBe(true);
    expect(waitingRing.attributes("aria-label")).toContain("候选信号");

    subscription.listener?.(snapshot(9, "roundCompleted", "observed"));
    await nextTick();

    const completedRing = wrapper.get('[data-slot="0"]');
    expect(completedRing.attributes("data-status")).toBe("roundCompleted");
    expect(completedRing.attributes("aria-label")).toContain("本轮完成");

    wrapper.unmount();
  });

  it.each(["degraded", "offline"] as const)(
    "%s 适配器使用低饱和蓝诊断，刷新状态不改写锁定绑定",
    async (adapterState) => {
      const adapterStatus: AdapterStatus = {
        ...DEGRADED_STATUS,
        state: adapterState,
      };
      getAdapterStatusMock.mockResolvedValue(adapterStatus);
      const store = createHaloStore(fakeBridge as unknown as HaloBridge);
      await store.load();
      const bindingBeforeRefresh = JSON.parse(
        JSON.stringify(store.state.snapshot),
      ) as HaloSnapshot;

      await store.refreshAdapterStatus();

      expect(store.state.adapterStatus).toStrictEqual(adapterStatus);
      expect(store.state.snapshot).toStrictEqual(bindingBeforeRefresh);
      expect(store.state.snapshot?.slots[0]).toMatchObject({
        index: 0,
        taskKey: TASK_KEY,
        bindingMode: "manual",
        locked: true,
      });

      const wrapper = mount(App);
      await flushPromises();
      const diagnostic = wrapper.get("[data-adapter-state]");
      expect(diagnostic.attributes()).toMatchObject({
        "data-adapter-state": adapterState,
        "data-diagnostic-tone": "muted-blue",
      });
      expect(diagnostic.attributes("aria-label")).toContain("适配器诊断");
      expect(diagnostic.classes()).toContain(`adapter-${adapterState}`);
      expect(wrapper.findComponent(HaloPreview).props("slots")[0]).toMatchObject({
        index: 0,
        taskKey: TASK_KEY,
        bindingMode: "manual",
        locked: true,
      });
      expect(baseStyles).toMatch(
        /\.adapter-degraded,\s*\.adapter-offline\s*\{[^}]*--adapter-color:\s*var\(--halo-unknown\)/s,
      );

      wrapper.unmount();
    },
  );
});
