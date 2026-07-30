import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AdapterStatus,
  DeviceStatus,
  HaloSnapshot,
  ManualBindInput,
  SimulateSignalInput,
  UpdateEffectInput,
} from "../types/halo";
import {
  createDemoHaloBridge,
  createHaloBridge,
  type HaloBridge,
} from "../services/haloBridge";
import { createHaloStore } from "./haloStore";

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

const EMPTY_SNAPSHOT: HaloSnapshot = {
  revision: 0,
  deviceMode: "virtual",
  displayMode: "ambient",
  selectedSlot: null,
  globalBrightness: 100,
  slots: Array.from({ length: 4 }, (_, index) => ({
    index,
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
  })),
  tasks: [],
  queue: [],
};

const RUNNING_SNAPSHOT: HaloSnapshot = {
  ...EMPTY_SNAPSHOT,
  revision: 1,
  slots: EMPTY_SNAPSHOT.slots.map((slot) =>
    slot.index === 0
      ? {
          ...slot,
          taskKey: "0123456789abcdef",
          status: "running",
          source: "simulator",
          confidence: "simulated",
        }
      : slot,
  ),
  tasks: [
    {
      taskKey: "0123456789abcdef",
      status: "running",
      source: "simulator",
      confidence: "simulated",
      lastActiveAtMs: 42,
    },
  ],
};

const ONLINE_STATUS: AdapterStatus = {
  revision: 1,
  state: "online",
  mode: "hook",
  message: null,
  acceptedEvents: 7,
  ignoredEvents: 1,
  rejectedEvents: 0,
};

const VIRTUAL_DEVICE_STATUS: DeviceStatus = {
  revision: 1,
  state: "virtual",
  transport: "simulator",
  message: null,
  firmwareVersion: "0.1.0",
  retryCount: 0,
};

function createStubBridge(
  overrides: Partial<HaloBridge> = {},
): HaloBridge {
  return {
    getSnapshot: async () => EMPTY_SNAPSHOT,
    subscribeSnapshots: async () => () => undefined,
    subscribeAdapterStatus: async () => () => undefined,
    subscribeDeviceStatus: async () => () => undefined,
    getAdapterStatus: async () => ONLINE_STATUS,
    getDeviceStatus: async () => VIRTUAL_DEVICE_STATUS,
    simulateSignal: async () => RUNNING_SNAPSHOT,
    manualBind: async () => RUNNING_SNAPSHOT,
    toggleLock: async () => RUNNING_SNAPSHOT,
    swapSlots: async () => RUNNING_SNAPSHOT,
    updateEffect: async () => RUNNING_SNAPSHOT,
    setGlobalBrightness: async () => RUNNING_SNAPSHOT,
    setPresentation: async () => RUNNING_SNAPSHOT,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("createHaloStore", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    delete (
      globalThis as typeof globalThis & { __TAURI_INTERNALS__?: unknown }
    ).__TAURI_INTERNALS__;
  });

  it("load 获取快照并始终结束 loading", async () => {
    let resolveSnapshot: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      getSnapshot: () =>
        new Promise((resolve) => {
          resolveSnapshot = resolve;
        }),
    });
    const store = createHaloStore(bridge);

    const loading = store.load();
    expect(store.state.loading).toBe(true);

    resolveSnapshot?.(RUNNING_SNAPSHOT);
    await loading;

    expect(store.state.snapshot).toStrictEqual(RUNNING_SNAPSHOT);
    expect(store.state.loading).toBe(false);
    expect(store.occupiedSlotCount.value).toBe(1);
  });

  it("命令成功原子替换快照，失败保留旧快照并记录稳定错误", async () => {
    const bridge = createStubBridge({
      manualBind: async () => RUNNING_SNAPSHOT,
    });
    const store = createHaloStore(bridge);
    await store.load();

    await store.manualBind({
      taskKey: "0123456789abcdef",
      slot: 0,
      lock: true,
    });
    expect(store.state.snapshot).toStrictEqual(RUNNING_SNAPSHOT);
    expect(store.state.error).toBeNull();

    bridge.manualBind = async () => {
      throw { code: "slotOutOfBounds", slot: 4 };
    };
    const previous = store.state.snapshot;
    const result = await store.manualBind({
      taskKey: "0123456789abcdef",
      slot: 4,
      lock: true,
    });

    expect(result).toBeNull();
    expect(store.state.snapshot).toBe(previous);
    expect(store.state.error).toEqual({
      operation: "manualBind",
      code: "slotOutOfBounds",
      message: "manualBind 操作失败",
    });
  });

  it("start 订阅 halo://snapshot 并用事件快照更新状态", async () => {
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);

    await store.start();
    listener?.(RUNNING_SNAPSHOT);

    expect(store.state.snapshot).toStrictEqual(RUNNING_SNAPSHOT);
  });

  it("重复 start 不创建监听器且 stop 只调用一次 unlisten", async () => {
    let subscriptions = 0;
    let unlistens = 0;
    const bridge = createStubBridge({
      subscribeSnapshots: async () => {
        subscriptions += 1;
        return () => {
          unlistens += 1;
        };
      },
    });
    const store = createHaloStore(bridge);

    await Promise.all([store.start(), store.start()]);
    await store.stop();
    await store.stop();

    expect(subscriptions).toBe(1);
    expect(unlistens).toBe(1);
  });

  it("start 后立即 stop 会清理延迟建立的监听器且不会虚假保持 started", async () => {
    const pendingSubscription = deferred<() => void>();
    let subscriptions = 0;
    let unlistens = 0;
    const bridge = createStubBridge({
      subscribeSnapshots: () => {
        subscriptions += 1;
        return pendingSubscription.promise;
      },
    });
    const store = createHaloStore(bridge);

    const starting = store.start();
    const stopping = store.stop();
    pendingSubscription.resolve(() => {
      unlistens += 1;
    });
    await Promise.all([starting, stopping]);
    await store.stop();

    expect(subscriptions).toBe(1);
    expect(unlistens).toBe(1);
  });

  it("start-stop-start 交错时清理过期监听器并只保留最终监听器", async () => {
    const subscriptions = [
      deferred<() => void>(),
      deferred<() => void>(),
    ];
    let subscriptionIndex = 0;
    let firstUnlistens = 0;
    let secondUnlistens = 0;
    const bridge = createStubBridge({
      subscribeSnapshots: () =>
        subscriptions[subscriptionIndex++].promise,
    });
    const store = createHaloStore(bridge);

    const firstStart = store.start();
    const stopping = store.stop();
    const finalStart = store.start();
    subscriptions[0].resolve(() => {
      firstUnlistens += 1;
    });
    await vi.waitFor(() => {
      expect(subscriptionIndex).toBe(2);
    });
    subscriptions[1].resolve(() => {
      secondUnlistens += 1;
    });
    await Promise.all([firstStart, stopping, finalStart]);

    expect(firstUnlistens).toBe(1);
    expect(secondUnlistens).toBe(0);

    await store.stop();
    expect(secondUnlistens).toBe(1);
  });

  it("任一实时订阅失败时清理已注册的另一监听器", async () => {
    let snapshotUnlistens = 0;
    const bridge = createStubBridge({
      subscribeSnapshots: async () => () => {
        snapshotUnlistens += 1;
      },
      subscribeAdapterStatus: async () => {
        throw { code: "adapterSubscriptionFailed" };
      },
    });
    const store = createHaloStore(bridge);

    await store.start();
    await store.stop();

    expect(snapshotUnlistens).toBe(1);
    expect(store.state.error).toStrictEqual({
      operation: "subscribe",
      code: "adapterSubscriptionFailed",
      message: "subscribe 操作失败",
    });
  });

  it("cleans every successful subscription when a late third subscription fails", async () => {
    const pendingDevice = deferred<() => void>();
    const unlistenSnapshot = vi.fn();
    const unlistenAdapter = vi.fn();
    const unlistenDevice = vi.fn();
    const bridge = createStubBridge({
      subscribeSnapshots: async () => unlistenSnapshot,
      subscribeAdapterStatus: async () => unlistenAdapter,
      subscribeDeviceStatus: () => pendingDevice.promise,
    });
    const store = createHaloStore(bridge);

    const starting = store.start();
    const stopping = store.stop();
    pendingDevice.resolve(unlistenDevice);
    await Promise.all([starting, stopping]);
    await store.stop();

    expect(unlistenSnapshot).toHaveBeenCalledTimes(1);
    expect(unlistenAdapter).toHaveBeenCalledTimes(1);
    expect(unlistenDevice).toHaveBeenCalledTimes(1);
  });

  it("cleans late successes when a sibling subscription throws synchronously", async () => {
    const pendingSnapshot = deferred<() => void>();
    const unlistenSnapshot = vi.fn();
    const unlistenDevice = vi.fn();
    const bridge = createStubBridge({
      subscribeSnapshots: () => pendingSnapshot.promise,
      subscribeAdapterStatus: () => {
        throw { code: "adapterSubscriptionFailed" };
      },
      subscribeDeviceStatus: async () => unlistenDevice,
    });
    const store = createHaloStore(bridge);

    const starting = store.start();
    pendingSnapshot.resolve(unlistenSnapshot);

    await expect(starting).resolves.toBe(false);
    expect(unlistenSnapshot).toHaveBeenCalledTimes(1);
    expect(unlistenDevice).toHaveBeenCalledTimes(1);
  });

  it("fails startup promptly and cleans subscriptions that succeed later", async () => {
    const pendingDevice = deferred<() => void>();
    const unlistenSnapshot = vi.fn();
    const unlistenDevice = vi.fn();
    const bridge = createStubBridge({
      subscribeSnapshots: async () => unlistenSnapshot,
      subscribeAdapterStatus: () => {
        throw { code: "adapterSubscriptionFailed" };
      },
      subscribeDeviceStatus: () => pendingDevice.promise,
    });
    const store = createHaloStore(bridge);
    let settled = false;

    const starting = store.start().finally(() => {
      settled = true;
    });

    await vi.waitFor(() => {
      expect(settled).toBe(true);
      expect(unlistenSnapshot).toHaveBeenCalledTimes(1);
    });
    await expect(starting).resolves.toBe(false);

    pendingDevice.resolve(unlistenDevice);
    await vi.waitFor(() => {
      expect(unlistenDevice).toHaveBeenCalledTimes(1);
    });
  });

  it("stops promptly while subscriptions are pending and cleans late successes", async () => {
    const pendingDevice = deferred<() => void>();
    const unlistenSnapshot = vi.fn();
    const unlistenAdapter = vi.fn();
    const unlistenDevice = vi.fn();
    const bridge = createStubBridge({
      subscribeSnapshots: async () => unlistenSnapshot,
      subscribeAdapterStatus: async () => unlistenAdapter,
      subscribeDeviceStatus: () => pendingDevice.promise,
    });
    const store = createHaloStore(bridge);
    let stopped = false;

    const starting = store.start();
    const stopping = store.stop().finally(() => {
      stopped = true;
    });

    await vi.waitFor(() => {
      expect(stopped).toBe(true);
      expect(unlistenSnapshot).toHaveBeenCalledTimes(1);
      expect(unlistenAdapter).toHaveBeenCalledTimes(1);
    });
    await expect(stopping).resolves.toBe(false);
    await expect(starting).resolves.toBe(false);

    pendingDevice.resolve(unlistenDevice);
    await vi.waitFor(() => {
      expect(unlistenDevice).toHaveBeenCalledTimes(1);
    });
  });

  it("attempts every unlisten even when an earlier cleanup throws", async () => {
    const unlistenAdapter = vi.fn();
    const unlistenDevice = vi.fn();
    const bridge = createStubBridge({
      subscribeSnapshots: async () => () => {
        throw new Error("snapshot cleanup failed");
      },
      subscribeAdapterStatus: async () => unlistenAdapter,
      subscribeDeviceStatus: async () => unlistenDevice,
    });
    const store = createHaloStore(bridge);
    await store.start();

    await expect(store.stop()).resolves.toBe(false);
    expect(unlistenAdapter).toHaveBeenCalledTimes(1);
    expect(unlistenDevice).toHaveBeenCalledTimes(1);
  });

  it("refreshes device status and rejects stale or equal device revisions", async () => {
    let listener: ((status: DeviceStatus) => void) | undefined;
    const bridge = createStubBridge({
      getDeviceStatus: async () => ({ ...VIRTUAL_DEVICE_STATUS, revision: 2 }),
      subscribeDeviceStatus: async (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);

    await store.start();
    await store.refreshDeviceStatus();
    listener?.({
      ...VIRTUAL_DEVICE_STATUS,
      revision: 3,
      state: "online",
      transport: "serial",
    });
    listener?.({ ...VIRTUAL_DEVICE_STATUS, revision: 3, state: "error" });
    listener?.({ ...VIRTUAL_DEVICE_STATUS, revision: 2, state: "connecting" });

    expect(store.state.deviceStatus).toMatchObject({
      revision: 3,
      state: "online",
      transport: "serial",
    });
  });

  it("rejects equal snapshot revisions and applies setPresentation atomically", async () => {
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const presented = {
      ...RUNNING_SNAPSHOT,
      revision: 4,
      displayMode: "detail" as const,
      selectedSlot: 2,
    };
    const bridge = createStubBridge({
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
      setPresentation: async () => presented,
    });
    const store = createHaloStore(bridge);
    await store.start();
    listener?.({ ...RUNNING_SNAPSHOT, revision: 3 });
    listener?.({ ...RUNNING_SNAPSHOT, revision: 3, displayMode: "overview" });

    expect(store.state.snapshot?.displayMode).toBe("ambient");
    await store.setPresentation({ displayMode: "detail", selectedSlot: 2 });
    expect(store.state.snapshot).toStrictEqual(presented);
  });

  it("preserves the prior snapshot when setPresentation fails", async () => {
    const bridge = createStubBridge({
      getSnapshot: async () => RUNNING_SNAPSHOT,
      setPresentation: async () => {
        throw { code: "presentationRejected" };
      },
    });
    const store = createHaloStore(bridge);
    await store.load();
    const previous = store.state.snapshot;

    const result = await store.setPresentation({
      displayMode: "overview",
      selectedSlot: 1,
    });

    expect(result).toBeNull();
    expect(store.state.snapshot).toBe(previous);
    expect(store.state.error).toMatchObject({
      operation: "setPresentation",
      code: "presentationRejected",
    });
  });

  it("returns null for stale command snapshots and the authoritative snapshot for equal revisions", async () => {
    const commands = [
      deferred<HaloSnapshot>(),
      deferred<HaloSnapshot>(),
    ];
    let commandIndex = 0;
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
      setPresentation: () => commands[commandIndex++].promise,
    });
    const store = createHaloStore(bridge);
    await store.start();
    listener?.({ ...RUNNING_SNAPSHOT, revision: 5 });

    const staleCommand = store.setPresentation({
      displayMode: "detail",
      selectedSlot: 2,
    });
    const newerEvent = {
      ...RUNNING_SNAPSHOT,
      revision: 7,
      displayMode: "overview" as const,
      selectedSlot: 1,
    };
    listener?.(newerEvent);
    commands[0].resolve({
      ...RUNNING_SNAPSHOT,
      revision: 6,
      displayMode: "detail",
      selectedSlot: 2,
    });

    await expect(staleCommand).resolves.toBeNull();
    expect(store.state.snapshot).toMatchObject(newerEvent);

    const equalCommand = store.setPresentation({
      displayMode: "detail",
      selectedSlot: 3,
    });
    const equalEvent = {
      ...RUNNING_SNAPSHOT,
      revision: 8,
      displayMode: "overview" as const,
      selectedSlot: 0,
    };
    listener?.(equalEvent);
    commands[1].resolve({
      ...RUNNING_SNAPSHOT,
      revision: 8,
      displayMode: "detail",
      selectedSlot: 3,
    });

    await expect(equalCommand).resolves.toBe(store.state.snapshot);
    expect(store.state.snapshot).toMatchObject(equalEvent);
  });

  it("keeps newer command success or failure state when an older failure arrives late", async () => {
    const olderFailure = deferred<HaloSnapshot>();
    const newerSuccess = deferred<HaloSnapshot>();
    const bridge = createStubBridge({
      manualBind: () => olderFailure.promise,
      setPresentation: () => newerSuccess.promise,
    });
    const store = createHaloStore(bridge);
    await store.load();

    const olderCommand = store.manualBind({
      taskKey: "0123456789abcdef",
      slot: 0,
      lock: false,
    });
    const newerCommand = store.setPresentation({
      displayMode: "detail",
      selectedSlot: 0,
    });
    newerSuccess.resolve({
      ...RUNNING_SNAPSHOT,
      revision: 2,
      displayMode: "detail",
      selectedSlot: 0,
    });
    await expect(newerCommand).resolves.toMatchObject({ revision: 2 });

    olderFailure.reject({ code: "olderFailure" });
    await expect(olderCommand).resolves.toBeNull();
    expect(store.state.error).toBeNull();

    const nextOlderFailure = deferred<HaloSnapshot>();
    const newerFailure = deferred<HaloSnapshot>();
    bridge.manualBind = () => nextOlderFailure.promise;
    bridge.setPresentation = () => newerFailure.promise;

    const nextOlderCommand = store.manualBind({
      taskKey: "0123456789abcdef",
      slot: 1,
      lock: false,
    });
    const nextNewerCommand = store.setPresentation({
      displayMode: "overview",
      selectedSlot: 1,
    });
    newerFailure.reject({ code: "newerFailure" });
    await expect(nextNewerCommand).resolves.toBeNull();
    expect(store.state.error).toMatchObject({ code: "newerFailure" });

    nextOlderFailure.reject({ code: "olderFailure" });
    await expect(nextOlderCommand).resolves.toBeNull();
    expect(store.state.error).toMatchObject({ code: "newerFailure" });
  });

  it("并发 load 共享请求且事件新快照不会被旧 load 结果覆盖", async () => {
    const pendingLoad = deferred<HaloSnapshot>();
    let loadRequests = 0;
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      getSnapshot: () => {
        loadRequests += 1;
        return pendingLoad.promise;
      },
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);
    await store.start();

    const firstLoad = store.load();
    const secondLoad = store.load();
    listener?.(RUNNING_SNAPSHOT);
    pendingLoad.resolve(EMPTY_SNAPSHOT);

    expect(store.state.loading).toBe(true);
    await Promise.all([firstLoad, secondLoad]);

    expect(loadRequests).toBe(1);
    expect(store.state.loading).toBe(false);
    expect(store.state.snapshot).toStrictEqual(RUNNING_SNAPSHOT);
  });

  it("命令新快照不会被较早发起的 load 结果覆盖", async () => {
    const pendingLoad = deferred<HaloSnapshot>();
    const bridge = createStubBridge({
      getSnapshot: () => pendingLoad.promise,
      setGlobalBrightness: async () => RUNNING_SNAPSHOT,
    });
    const store = createHaloStore(bridge);

    const loading = store.load();
    await store.setGlobalBrightness(75);
    pendingLoad.resolve(EMPTY_SNAPSHOT);
    await loading;

    expect(store.state.snapshot).toStrictEqual(RUNNING_SNAPSHOT);
  });

  it("事件 revision 5 后成功返回的 load revision 6 仍会成为当前快照", async () => {
    const pendingLoad = deferred<HaloSnapshot>();
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      getSnapshot: () => pendingLoad.promise,
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);
    await store.start();

    const loading = store.load();
    listener?.({ ...RUNNING_SNAPSHOT, revision: 5 });
    const newest = {
      ...RUNNING_SNAPSHOT,
      revision: 6,
      globalBrightness: 60,
    };
    pendingLoad.resolve(newest);
    await loading;

    expect(store.state.snapshot).toStrictEqual(newest);
  });

  it("事件 revision 6 后成功返回的 load revision 5 由版本裁决拒绝", async () => {
    const pendingLoad = deferred<HaloSnapshot>();
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      getSnapshot: () => pendingLoad.promise,
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);
    await store.start();

    const loading = store.load();
    const newest = { ...RUNNING_SNAPSHOT, revision: 6 };
    listener?.(newest);
    pendingLoad.resolve({
      ...RUNNING_SNAPSHOT,
      revision: 5,
      globalBrightness: 50,
    });
    await loading;

    expect(store.state.snapshot).toStrictEqual(newest);
  });

  it("较早 load 失败不会在新事件快照后写入陈旧错误", async () => {
    const pendingLoad = deferred<HaloSnapshot>();
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      getSnapshot: () => pendingLoad.promise,
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);
    await store.start();

    const loading = store.load();
    listener?.({ ...RUNNING_SNAPSHOT, revision: 5 });
    pendingLoad.reject({ code: "offline" });
    await loading;

    expect(store.state.error).toBeNull();
  });

  it("实时事件的新 revision 不会被稍后返回的旧命令快照覆盖", async () => {
    const pendingCommand = deferred<HaloSnapshot>();
    let listener: ((snapshot: HaloSnapshot) => void) | undefined;
    const bridge = createStubBridge({
      subscribeSnapshots: async (next) => {
        listener = next;
        return () => undefined;
      },
      setGlobalBrightness: () => pendingCommand.promise,
    });
    const store = createHaloStore(bridge);
    await store.start();

    const command = store.setGlobalBrightness(50);
    const eventSnapshot = { ...RUNNING_SNAPSHOT, revision: 5 };
    listener?.(eventSnapshot);
    pendingCommand.resolve({
      ...RUNNING_SNAPSHOT,
      revision: 4,
      globalBrightness: 50,
    });
    await command;

    expect(store.state.snapshot).toStrictEqual(eventSnapshot);
  });

  it.each(["online", "degraded", "offline"] as const)(
    "适配器状态支持 %s",
    async (state) => {
      const bridge = createStubBridge({
        getAdapterStatus: async () => ({
          revision: 1,
          state,
          mode: "hook",
          message: state === "online" ? null : `${state} diagnostic`,
          acceptedEvents: 7,
          ignoredEvents: 1,
          rejectedEvents: state === "online" ? 0 : 2,
        }),
      });
      const store = createHaloStore(bridge);

      await store.refreshAdapterStatus();

      expect(store.state.adapterStatus).toStrictEqual({
        revision: 1,
        state,
        mode: "hook",
        message: state === "online" ? null : `${state} diagnostic`,
        acceptedEvents: 7,
        ignoredEvents: 1,
        rejectedEvents: state === "online" ? 0 : 2,
      });
    },
  );

  it("适配器事件优先于晚到命令状态并拒绝较旧 revision", async () => {
    let adapterListener: ((status: AdapterStatus) => void) | undefined;
    const staleStatus: AdapterStatus = {
      revision: 2,
      state: "offline",
      mode: "hook",
      message: "旧状态",
      acceptedEvents: 0,
      ignoredEvents: 0,
      rejectedEvents: 1,
    };
    const bridge = createStubBridge({
      getSnapshot: async () => RUNNING_SNAPSHOT,
      getAdapterStatus: async () => staleStatus,
      subscribeAdapterStatus: async (listener) => {
        adapterListener = listener;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);
    await store.load();
    const snapshotBeforeStatus = store.state.snapshot;
    await store.start();

    adapterListener?.({
      revision: 4,
      state: "online",
      mode: "hook",
      message: null,
      acceptedEvents: 4,
      ignoredEvents: 0,
      rejectedEvents: 1,
    });
    await store.refreshAdapterStatus();
    adapterListener?.({
      ...staleStatus,
      revision: 3,
      state: "degraded",
    });

    expect(store.state.adapterStatus).toMatchObject({
      revision: 4,
      state: "online",
    });
    expect(store.state.snapshot).toBe(snapshotBeforeStatus);
  });

  it("实时适配器状态不会被后续读取失败覆盖或污染同 revision 裁决", async () => {
    let adapterListener: ((status: AdapterStatus) => void) | undefined;
    const lockedSnapshot: HaloSnapshot = {
      ...RUNNING_SNAPSHOT,
      slots: RUNNING_SNAPSHOT.slots.map((slot) =>
        slot.index === 0
          ? {
              ...slot,
              bindingMode: "manual" as const,
              locked: true,
            }
          : slot,
      ),
    };
    const bridge = createStubBridge({
      getSnapshot: async () => lockedSnapshot,
      getAdapterStatus: async () => {
        throw { code: "adapterReadFailed" };
      },
      subscribeAdapterStatus: async (listener) => {
        adapterListener = listener;
        return () => undefined;
      },
    });
    const store = createHaloStore(bridge);
    await store.load();
    const snapshotBeforeStatus = store.state.snapshot;
    await store.start();

    adapterListener?.({
      revision: 4,
      state: "online",
      mode: "hook",
      message: null,
      acceptedEvents: 4,
      ignoredEvents: 0,
      rejectedEvents: 0,
    });
    await store.refreshAdapterStatus();

    expect(store.state.adapterStatus).toMatchObject({
      revision: 4,
      state: "online",
      acceptedEvents: 4,
    });
    expect(store.state.error).toMatchObject({
      operation: "adapterStatus",
      code: "adapterReadFailed",
    });
    expect(store.state.snapshot).toBe(snapshotBeforeStatus);
    expect(store.state.snapshot?.slots[0]).toMatchObject({
      bindingMode: "manual",
      locked: true,
    });

    adapterListener?.({
      revision: 4,
      state: "degraded",
      mode: "hook",
      message: "同 revision 冲突状态",
      acceptedEvents: 5,
      ignoredEvents: 0,
      rejectedEvents: 1,
    });
    expect(store.state.adapterStatus).toMatchObject({
      revision: 4,
      state: "online",
      acceptedEvents: 4,
    });
    expect(store.state.snapshot).toBe(snapshotBeforeStatus);
  });

  it("非 Tauri 环境使用确定性的演示 bridge 且不误调用 IPC", async () => {
    const bridge = createHaloBridge();
    const initial = await bridge.getSnapshot();
    const status = await bridge.getAdapterStatus();

    expect(initial.deviceMode).toBe("virtual");
    expect(initial.slots).toHaveLength(4);
    expect(initial.slots.every((slot) => slot.taskKey === null)).toBe(true);
    expect(status).toEqual({
      revision: 1,
      state: "degraded",
      mode: "demo",
      message: "浏览器演示模式：未连接 Codex Hook",
      acceptedEvents: 0,
      ignoredEvents: 0,
      rejectedEvents: 0,
    });

    const updated = await bridge.simulateSignal({
      taskKey: "0123456789abcdef",
      signalKind: "userPromptSubmit",
      receivedAtMs: 42,
    });

    expect(updated.slots[0]).toMatchObject({
      taskKey: "0123456789abcdef",
      status: "running",
    });
    expect(invokeMock).not.toHaveBeenCalled();
    expect(listenMock).not.toHaveBeenCalled();
  });

  it("演示 bridge 仅在实际变更或接受信号时递增 revision", async () => {
    const bridge = createDemoHaloBridge();
    expect((await bridge.getSnapshot()).revision).toBe(0);

    expect((await bridge.setGlobalBrightness(100)).revision).toBe(0);
    expect((await bridge.setGlobalBrightness(50)).revision).toBe(1);
    expect((await bridge.setGlobalBrightness(50)).revision).toBe(1);

    const accepted = await bridge.simulateSignal({
      taskKey: "0123456789abcdef",
      signalKind: "userPromptSubmit",
      receivedAtMs: 42,
    });
    expect(accepted.revision).toBe(2);
    const old = await bridge.simulateSignal({
      taskKey: "0123456789abcdef",
      signalKind: "stop",
      receivedAtMs: 41,
    });
    expect(old.revision).toBe(2);

    expect(
      (
        await bridge.updateEffect({
          slot: 0,
          brightness: 80,
          speedPercent: 100,
          direction: "clockwise",
          tailPercent: 35,
        })
      ).revision,
    ).toBe(2);
    expect(
      (
        await bridge.updateEffect({
          slot: 0,
          brightness: 50,
          speedPercent: 200,
          direction: "counterClockwise",
          tailPercent: 50,
        })
      ).revision,
    ).toBe(3);
  });

  it("演示 bridge 的 bound/queued 完全相同重放不递增，等时不同状态递增", async () => {
    const bridge = createDemoHaloBridge();
    const boundInput: SimulateSignalInput = {
      taskKey: "0000000000000001",
      signalKind: "userPromptSubmit",
      receivedAtMs: 100,
    };
    const bound = await bridge.simulateSignal(boundInput);
    const boundReplay = await bridge.simulateSignal(boundInput);
    expect(boundReplay.revision).toBe(bound.revision);

    for (let value = 2; value <= 4; value += 1) {
      await bridge.simulateSignal({
        taskKey: value.toString(16).padStart(16, "0"),
        signalKind: "userPromptSubmit",
        receivedAtMs: value * 100,
      });
    }
    const queuedInput: SimulateSignalInput = {
      taskKey: "0000000000000005",
      signalKind: "permissionRequest",
      receivedAtMs: 500,
    };
    const queued = await bridge.simulateSignal(queuedInput);
    const queuedReplay = await bridge.simulateSignal(queuedInput);
    expect(queuedReplay.revision).toBe(queued.revision);

    const changed = await bridge.simulateSignal({
      ...queuedInput,
      signalKind: "userPromptSubmit",
    });
    expect(changed.revision).toBe(queued.revision + 1);
  });
});

describe("TauriHaloBridge", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    (
      globalThis as typeof globalThis & { __TAURI_INTERNALS__?: unknown }
    ).__TAURI_INTERNALS__ = {};
    invokeMock.mockResolvedValue(RUNNING_SNAPSHOT);
    listenMock.mockResolvedValue(() => undefined);
  });

  it("使用 Task 4 命令名、事件名和精确参数名", async () => {
    const bridge = createHaloBridge();
    const signal: SimulateSignalInput = {
      taskKey: "0123456789abcdef",
      signalKind: "stop",
      receivedAtMs: 43,
    };
    const binding: ManualBindInput = {
      taskKey: "0123456789abcdef",
      slot: 1,
      lock: true,
    };
    const effect: UpdateEffectInput = {
      slot: 1,
      brightness: 60,
      speedPercent: 175,
      direction: "counterClockwise",
      tailPercent: 50,
    };

    await bridge.getSnapshot();
    await bridge.subscribeSnapshots(() => undefined);
    await bridge.subscribeAdapterStatus(() => undefined);
    await bridge.subscribeDeviceStatus(() => undefined);
    await bridge.getAdapterStatus();
    await bridge.getDeviceStatus();
    await bridge.simulateSignal(signal);
    await bridge.manualBind(binding);
    await bridge.toggleLock(1);
    await bridge.swapSlots(1, 2);
    await bridge.updateEffect(effect);
    await bridge.setGlobalBrightness(75);
    await bridge.setPresentation({ displayMode: "detail", selectedSlot: 2 });

    expect(invokeMock.mock.calls).toEqual([
      ["get_snapshot"],
      ["get_adapter_status"],
      ["get_device_status"],
      ["simulate_signal", { input: signal }],
      ["manual_bind", { input: binding }],
      ["toggle_lock", { slot: 1 }],
      ["swap_slots", { left: 1, right: 2 }],
      ["update_effect", { input: effect }],
      ["set_global_brightness", { value: 75 }],
      ["set_presentation", { input: { displayMode: "detail", selectedSlot: 2 } }],
    ]);
    expect(listenMock).toHaveBeenCalledWith(
      "halo://snapshot",
      expect.any(Function),
    );
    expect(listenMock).toHaveBeenCalledWith(
      "halo://adapter-status",
      expect.any(Function),
    );
    expect(listenMock).toHaveBeenCalledWith(
      "halo://device-status",
      expect.any(Function),
    );
  });

  it("导出的 DemoHaloBridge 也能演示手动绑定和灯效命令", async () => {
    const bridge = createDemoHaloBridge();
    await bridge.simulateSignal({
      taskKey: "0123456789abcdef",
      signalKind: "userPromptSubmit",
      receivedAtMs: 42,
    });

    const bound = await bridge.manualBind({
      taskKey: "0123456789abcdef",
      slot: 2,
      lock: true,
    });
    expect(bound.slots[2]).toMatchObject({
      taskKey: "0123456789abcdef",
      bindingMode: "manual",
      locked: true,
    });

    const effected = await bridge.updateEffect({
      slot: 2,
      brightness: 60,
      speedPercent: 175,
      direction: "counterClockwise",
      tailPercent: 50,
    });
    expect(effected.slots[2].effect).toEqual({
      brightness: 60,
      speedPercent: 175,
      direction: "counterClockwise",
      tailPercent: 50,
    });

    const dimmed = await bridge.setGlobalBrightness(75);
    expect(dimmed.globalBrightness).toBe(75);
  });

  it("演示 bridge 拒绝覆盖或交换锁定圈且保持快照原子不变", async () => {
    const bridge = createDemoHaloBridge();
    for (let value = 1; value <= 5; value += 1) {
      await bridge.simulateSignal({
        taskKey: `000000000000000${value}`,
        signalKind: "preToolUse",
        receivedAtMs: value * 100,
      });
    }
    await bridge.manualBind({
      taskKey: "0000000000000001",
      slot: 0,
      lock: true,
    });
    const before = await bridge.getSnapshot();

    await expect(
      bridge.manualBind({
        taskKey: "0000000000000005",
        slot: 0,
        lock: false,
      }),
    ).rejects.toStrictEqual({ code: "slotLocked", slot: 0 });
    await expect(bridge.swapSlots(0, 1)).rejects.toStrictEqual({
      code: "slotLocked",
      slot: 0,
    });

    expect(await bridge.getSnapshot()).toStrictEqual(before);
    expect(before.slots[0]).toMatchObject({
      taskKey: "0000000000000001",
      locked: true,
    });
    expect(before.queue[0].taskKey).toBe("0000000000000005");
  });

  it("演示 bridge 的第五任务进入带可信来源的 queued 快照", async () => {
    const bridge = createDemoHaloBridge();
    for (let task = 1; task <= 5; task += 1) {
      await bridge.simulateSignal({
        taskKey: task.toString(16).padStart(16, "0"),
        signalKind: task === 5 ? "permissionRequest" : "userPromptSubmit",
        receivedAtMs: task * 100,
      });
    }

    const snapshot = await bridge.getSnapshot();
    expect(snapshot.slots.map((slot) => slot.taskKey)).toEqual([
      "0000000000000001",
      "0000000000000002",
      "0000000000000003",
      "0000000000000004",
    ]);
    expect(snapshot.queue).toEqual([
      {
        taskKey: "0000000000000005",
        status: "queued",
        source: "simulator",
        confidence: "simulated",
        lastActiveAtMs: 500,
      },
    ]);
  });

  it("演示 bridge 刷新排队任务时按活动时间与 taskKey 稳定重排", async () => {
    const bridge = createDemoHaloBridge();
    for (let task = 1; task <= 6; task += 1) {
      await bridge.simulateSignal({
        taskKey: task.toString(16).padStart(16, "0"),
        signalKind: "userPromptSubmit",
        receivedAtMs: task <= 4 ? task * 100 : 600,
      });
    }

    let snapshot = await bridge.getSnapshot();
    expect(snapshot.queue.map((task) => task.taskKey)).toEqual([
      "0000000000000005",
      "0000000000000006",
    ]);

    await bridge.simulateSignal({
      taskKey: "0000000000000006",
      signalKind: "permissionRequest",
      receivedAtMs: 700,
    });
    snapshot = await bridge.getSnapshot();
    expect(snapshot.queue.map((task) => task.taskKey)).toEqual([
      "0000000000000006",
      "0000000000000005",
    ]);
    expect(snapshot.queue[0]).toMatchObject({
      status: "queued",
      source: "simulator",
      confidence: "simulated",
      lastActiveAtMs: 700,
    });
  });

  it("演示 bridge 完全忽略绑定任务和排队任务的乱序旧信号", async () => {
    const bridge = createDemoHaloBridge();
    for (let task = 1; task <= 5; task += 1) {
      await bridge.simulateSignal({
        taskKey: task.toString(16).padStart(16, "0"),
        signalKind: task === 1 || task === 5 ? "permissionRequest" : "stop",
        receivedAtMs: task === 5 ? 1_100 : task * 1_000,
      });
    }
    const before = await bridge.getSnapshot();

    await bridge.simulateSignal({
      taskKey: "0000000000000001",
      signalKind: "userPromptSubmit",
      receivedAtMs: 900,
    });
    await bridge.simulateSignal({
      taskKey: "0000000000000005",
      signalKind: "userPromptSubmit",
      receivedAtMs: 1_000,
    });

    expect(await bridge.getSnapshot()).toEqual(before);
  });

  it("演示 bridge 移动已绑定任务时回填腾空槽并维持全局唯一", async () => {
    const bridge = createDemoHaloBridge();
    for (let task = 1; task <= 5; task += 1) {
      await bridge.simulateSignal({
        taskKey: task.toString(16).padStart(16, "0"),
        signalKind: "userPromptSubmit",
        receivedAtMs: task * 100,
      });
    }

    const snapshot = await bridge.manualBind({
      taskKey: "0000000000000001",
      slot: 2,
      lock: true,
    });

    expect(snapshot.slots.every((slot) => slot.taskKey !== null)).toBe(true);
    expect(
      snapshot.slots.filter(
        (slot) => slot.taskKey === "0000000000000001",
      ),
    ).toHaveLength(1);
    expect(snapshot.slots[2]).toMatchObject({
      taskKey: "0000000000000001",
      bindingMode: "manual",
      locked: true,
    });
    expect(snapshot.slots.some((slot) => slot.taskKey === "0000000000000005"))
      .toBe(true);
    expect(snapshot.queue.map((task) => task.taskKey)).toEqual([
      "0000000000000003",
    ]);
  });
});

describe("DemoHaloBridge device presentation", () => {
  it("exposes virtual device status and publishes revisioned presentation", async () => {
    const bridge = createDemoHaloBridge();
    const snapshots: HaloSnapshot[] = [];
    const unlistenSnapshot = await bridge.subscribeSnapshots((snapshot) => {
      snapshots.push(snapshot);
    });
    const unlistenDevice = await bridge.subscribeDeviceStatus(() => undefined);

    await expect(bridge.getDeviceStatus()).resolves.toStrictEqual(
      VIRTUAL_DEVICE_STATUS,
    );
    const presented = await bridge.setPresentation({
      displayMode: "overview",
      selectedSlot: 1,
    });

    expect(presented).toMatchObject({
      revision: 1,
      displayMode: "overview",
      selectedSlot: 1,
    });
    expect(snapshots).toStrictEqual([presented]);
    await expect(
      bridge.setPresentation({ displayMode: "overview", selectedSlot: 1 }),
    ).resolves.toMatchObject({ revision: 1 });

    unlistenSnapshot();
    unlistenDevice();
    unlistenSnapshot();
    unlistenDevice();
  });
});
