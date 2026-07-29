import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AdapterStatus,
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
  deviceMode: "virtual",
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
  state: "online",
  mode: "hook",
  message: null,
};

function createStubBridge(
  overrides: Partial<HaloBridge> = {},
): HaloBridge {
  return {
    getSnapshot: async () => EMPTY_SNAPSHOT,
    subscribeSnapshots: async () => () => undefined,
    getAdapterStatus: async () => ONLINE_STATUS,
    simulateSignal: async () => RUNNING_SNAPSHOT,
    manualBind: async () => RUNNING_SNAPSHOT,
    toggleLock: async () => RUNNING_SNAPSHOT,
    swapSlots: async () => RUNNING_SNAPSHOT,
    updateEffect: async () => RUNNING_SNAPSHOT,
    setGlobalBrightness: async () => RUNNING_SNAPSHOT,
    ...overrides,
  };
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

  it.each(["online", "degraded", "offline"] as const)(
    "适配器状态支持 %s",
    async (state) => {
      const bridge = createStubBridge({
        getAdapterStatus: async () => ({
          state,
          mode: "hook",
          message: state === "online" ? null : `${state} diagnostic`,
        }),
      });
      const store = createHaloStore(bridge);

      await store.refreshAdapterStatus();

      expect(store.state.adapterStatus.state).toBe(state);
    },
  );

  it("非 Tauri 环境使用确定性的演示 bridge 且不误调用 IPC", async () => {
    const bridge = createHaloBridge();
    const initial = await bridge.getSnapshot();
    const status = await bridge.getAdapterStatus();

    expect(initial.deviceMode).toBe("virtual");
    expect(initial.slots).toHaveLength(4);
    expect(initial.slots.every((slot) => slot.taskKey === null)).toBe(true);
    expect(status).toEqual({
      state: "degraded",
      mode: "demo",
      message: "浏览器演示模式：未连接 Codex Hook",
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
    await bridge.getAdapterStatus();
    await bridge.simulateSignal(signal);
    await bridge.manualBind(binding);
    await bridge.toggleLock(1);
    await bridge.swapSlots(1, 2);
    await bridge.updateEffect(effect);
    await bridge.setGlobalBrightness(75);

    expect(invokeMock.mock.calls).toEqual([
      ["get_snapshot"],
      ["get_adapter_status"],
      ["simulate_signal", { input: signal }],
      ["manual_bind", { input: binding }],
      ["toggle_lock", { slot: 1 }],
      ["swap_slots", { left: 1, right: 2 }],
      ["update_effect", { input: effect }],
      ["set_global_brightness", { value: 75 }],
    ]);
    expect(listenMock).toHaveBeenCalledWith(
      "halo://snapshot",
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
});
