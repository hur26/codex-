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
  subscribeAdapterStatusMock,
  getAdapterStatusMock,
  unlistenMock,
  adapterUnlistenMock,
  fakeBridge,
  subscription,
  adapterSubscription,
} = vi.hoisted(() => {
  const currentSubscription = {
    listener: undefined as ((snapshot: HaloSnapshot) => void) | undefined,
  };
  const currentAdapterSubscription = {
    listener: undefined as ((status: AdapterStatus) => void) | undefined,
  };
  const stopListening: () => void = vi.fn();
  const stopAdapterListening: () => void = vi.fn();
  const getSnapshot = vi.fn();
  const getAdapterStatus = vi.fn();
  const subscribeSnapshots = vi.fn(
    async (listener: (snapshot: HaloSnapshot) => void) => {
      currentSubscription.listener = listener;
      return stopListening;
    },
  );
  const subscribeAdapterStatus = vi.fn(
    async (listener: (status: AdapterStatus) => void) => {
      currentAdapterSubscription.listener = listener;
      return stopAdapterListening;
    },
  );

  return {
    getSnapshotMock: getSnapshot,
    subscribeSnapshotsMock: subscribeSnapshots,
    subscribeAdapterStatusMock: subscribeAdapterStatus,
    getAdapterStatusMock: getAdapterStatus,
    unlistenMock: stopListening,
    adapterUnlistenMock: stopAdapterListening,
    subscription: currentSubscription,
    adapterSubscription: currentAdapterSubscription,
    fakeBridge: {
      getSnapshot,
      subscribeSnapshots,
      subscribeAdapterStatus,
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
import appSource from "../App.vue?raw";
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
  revision: 2,
  state: "degraded",
  mode: "hook",
  message: "探针目录暂时不可读",
  acceptedEvents: 4,
  ignoredEvents: 1,
  rejectedEvents: 0,
} as AdapterStatus;
const baseStyles = readFileSync(
  resolve(process.cwd(), "src/styles/base.css"),
  "utf8",
);

describe("Hook 到四环实时集成", () => {
  beforeEach(() => {
    subscription.listener = undefined;
    adapterSubscription.listener = undefined;
    getSnapshotMock.mockReset();
    getSnapshotMock.mockResolvedValue(INITIAL_SNAPSHOT);
    getAdapterStatusMock.mockReset();
    getAdapterStatusMock.mockResolvedValue(DEGRADED_STATUS);
    subscribeSnapshotsMock.mockClear();
    subscribeAdapterStatusMock.mockClear();
    vi.mocked(unlistenMock).mockClear();
    vi.mocked(adapterUnlistenMock).mockClear();
  });

  afterEach(() => {
    subscription.listener = undefined;
    adapterSubscription.listener = undefined;
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
    expect(subscribeAdapterStatusMock).toHaveBeenCalledTimes(1);
    expect(subscription.listener).toBeTypeOf("function");

    wrapper.unmount();
    await flushPromises();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
    expect(adapterUnlistenMock).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("监听器全部注册后才读取初始状态，期间事件优先于晚到的旧快照", async () => {
    const snapshotSubscription = deferred<() => void>();
    const adapterStatusSubscription = deferred<() => void>();
    const pendingLoad = deferred<HaloSnapshot>();
    subscribeSnapshotsMock.mockImplementationOnce(async (listener) => {
      subscription.listener = listener;
      return snapshotSubscription.promise;
    });
    subscribeAdapterStatusMock.mockImplementationOnce(async (listener) => {
      adapterSubscription.listener = listener;
      return adapterStatusSubscription.promise;
    });
    getSnapshotMock.mockReturnValueOnce(pendingLoad.promise);

    const wrapper = mount(App);
    await flushPromises();
    expect(getSnapshotMock).not.toHaveBeenCalled();
    expect(getAdapterStatusMock).not.toHaveBeenCalled();

    snapshotSubscription.resolve(unlistenMock);
    await flushPromises();
    expect(getSnapshotMock).not.toHaveBeenCalled();

    adapterStatusSubscription.resolve(adapterUnlistenMock);
    await flushPromises();
    expect(getSnapshotMock).toHaveBeenCalledTimes(1);
    expect(getAdapterStatusMock).toHaveBeenCalledTimes(1);

    subscription.listener?.(snapshot(8, "waiting", "provisional"));
    pendingLoad.resolve(INITIAL_SNAPSHOT);
    await flushPromises();

    expect(wrapper.get('[data-slot="0"]').attributes()).toMatchObject({
      "data-status": "waiting",
      "data-source": "hook",
      "data-confidence": "provisional",
    });

    wrapper.unmount();
    await flushPromises();
    expect(unlistenMock).toHaveBeenCalledTimes(1);
    expect(adapterUnlistenMock).toHaveBeenCalledTimes(1);
  });

  it("订阅尚未完成便卸载时清理每个迟到监听器且不再加载", async () => {
    const snapshotSubscription = deferred<() => void>();
    const adapterStatusSubscription = deferred<() => void>();
    subscribeSnapshotsMock.mockImplementationOnce(async (listener) => {
      subscription.listener = listener;
      return snapshotSubscription.promise;
    });
    subscribeAdapterStatusMock.mockImplementationOnce(async (listener) => {
      adapterSubscription.listener = listener;
      return adapterStatusSubscription.promise;
    });

    const wrapper = mount(App);
    await flushPromises();
    wrapper.unmount();

    snapshotSubscription.resolve(unlistenMock);
    adapterStatusSubscription.resolve(adapterUnlistenMock);
    await flushPromises();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
    expect(adapterUnlistenMock).toHaveBeenCalledTimes(1);
    expect(getSnapshotMock).not.toHaveBeenCalled();
    expect(getAdapterStatusMock).not.toHaveBeenCalled();
  });

  it("任一监听注册失败时清理已注册监听器且不读取无保护的初始状态", async () => {
    subscribeAdapterStatusMock.mockRejectedValueOnce({
      code: "adapterSubscriptionFailed",
    });
    const wrapper = mount(App);
    await flushPromises();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
    expect(getSnapshotMock).not.toHaveBeenCalled();
    expect(getAdapterStatusMock).not.toHaveBeenCalled();

    wrapper.unmount();
    await flushPromises();
    expect(unlistenMock).toHaveBeenCalledTimes(1);
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
        role: "status",
        "aria-live": "polite",
        "aria-atomic": "true",
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
      expect(baseStyles).toMatch(/@media\s*\(forced-colors:\s*active\)/);
      expect(appSource).not.toContain(
        ':class="`adapter-${store.state.adapterStatus.state}`"',
      );

      wrapper.unmount();
    },
  );

  it("适配器事件实时更新诊断并按 revision 拒绝旧状态，不改写锁定绑定", async () => {
    const pendingStatus = deferred<AdapterStatus>();
    getAdapterStatusMock.mockReturnValueOnce(pendingStatus.promise);
    const wrapper = mount(App);
    await flushPromises();

    const onlineStatus = {
      ...DEGRADED_STATUS,
      revision: 4,
      state: "online" as const,
      message: null,
    };
    adapterSubscription.listener?.(onlineStatus);
    await nextTick();
    expect(
      wrapper.get("[data-adapter-state]").attributes("data-adapter-state"),
    ).toBe("online");

    pendingStatus.resolve({
      ...DEGRADED_STATUS,
      revision: 3,
      state: "offline",
    } as AdapterStatus);
    await flushPromises();
    expect(
      wrapper.get("[data-adapter-state]").attributes("data-adapter-state"),
    ).toBe("online");

    const degradedStatus = {
      ...DEGRADED_STATUS,
      revision: 5,
      state: "degraded" as const,
    };
    adapterSubscription.listener?.(degradedStatus);
    await nextTick();
    expect(
      wrapper.get("[data-adapter-state]").attributes("data-adapter-state"),
    ).toBe("degraded");
    expect(wrapper.findComponent(HaloPreview).props("slots")[0]).toMatchObject({
      index: 0,
      taskKey: TASK_KEY,
      bindingMode: "manual",
      locked: true,
    });

    wrapper.unmount();
  });
});

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
