import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { nextTick, reactive } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AdapterStatus,
  DeviceStatus,
  HaloSnapshot,
  RingSlot,
  TaskRecord,
} from "./types/halo";

const {
  createHaloStoreMock,
  loadMock,
  refreshAdapterStatusMock,
  refreshDeviceStatusMock,
  setPresentationMock,
  manualBindMock,
  swapSlotsMock,
  startMock,
  stopMock,
  fakeState,
} = vi.hoisted(() => {
  const state = {
    snapshot: null as HaloSnapshot | null,
    adapterStatus: {
      state: "offline",
      mode: "hook",
      message: "适配器状态尚未读取",
      acceptedEvents: 0,
      ignoredEvents: 0,
      rejectedEvents: 0,
    } as AdapterStatus,
    deviceStatus: {
      revision: 1,
      state: "virtual",
      transport: "simulator",
      message: null,
      firmwareVersion: "0.1.0",
      retryCount: 0,
    } as DeviceStatus,
    loading: false,
    error: null as { operation: string; code: string; message: string } | null,
  };

  return {
    createHaloStoreMock: vi.fn(),
    loadMock: vi.fn(() => Promise.resolve()),
    refreshAdapterStatusMock: vi.fn(() => Promise.resolve()),
    refreshDeviceStatusMock: vi.fn(() => Promise.resolve()),
    setPresentationMock: vi.fn(),
    manualBindMock: vi.fn(),
    swapSlotsMock: vi.fn(),
    startMock: vi.fn(() => Promise.resolve(true)),
    stopMock: vi.fn(() => Promise.resolve()),
    fakeState: state,
  };
});

vi.mock("./stores/haloStore", () => ({
  createHaloStore: createHaloStoreMock,
}));

import App from "./App.vue";
import appSource from "./App.vue?raw";
import BindingControls from "./components/BindingControls.vue";
import CentralDisplay from "./components/CentralDisplay.vue";
import CrownControl from "./components/CrownControl.vue";
import HaloPreview from "./components/HaloPreview.vue";
import TaskRail from "./components/TaskRail.vue";
import taskRailSource from "./components/TaskRail.vue?raw";

const baseStyles = readFileSync(
  resolve(process.cwd(), "src/styles/base.css"),
  "utf8",
);
const haloTypesSource = readFileSync(
  resolve(process.cwd(), "src/types/halo.ts"),
  "utf8",
);
const haloStoreSource = readFileSync(
  resolve(process.cwd(), "src/stores/haloStore.ts"),
  "utf8",
);

enableAutoUnmount(afterEach);

let mountedState: { snapshot: HaloSnapshot | null };

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function task(
  index: number,
  status: TaskRecord["status"],
  lastActiveAtMs: number,
): TaskRecord {
  return {
    taskKey: `secret-task-${index}`,
    status,
    source: status === "failed" ? "simulator" : "hook",
    confidence:
      status === "waiting"
        ? "provisional"
        : status === "failed"
          ? "simulated"
          : "observed",
    lastActiveAtMs,
  };
}

function slot(index: number, record: TaskRecord): RingSlot {
  return {
    index,
    taskKey: record.taskKey,
    status: record.status,
    source: record.source,
    confidence: record.confidence,
    bindingMode: "auto",
    locked: false,
    effect: {
      brightness: 80,
      speedPercent: 100,
      direction: "clockwise",
      tailPercent: 35,
    },
  };
}

const records = [
  task(0, "running", 100_000),
  task(1, "waiting", 90_000),
  task(2, "roundCompleted", 80_000),
  task(3, "failed", 70_000),
  task(4, "queued", 110_000),
];

const snapshot: HaloSnapshot = {
  revision: 1,
  deviceMode: "virtual",
  displayMode: "ambient",
  selectedSlot: null,
  globalBrightness: 86,
  slots: records.slice(0, 4).map((record, index) => slot(index, record)),
  tasks: records,
  queue: [records[4]],
};

describe("App control center", () => {
  beforeEach(() => {
    fakeState.snapshot = snapshot;
    fakeState.adapterStatus = {
      revision: 1,
      state: "degraded",
      mode: "demo",
      message: "浏览器演示模式",
      acceptedEvents: 0,
      ignoredEvents: 0,
      rejectedEvents: 0,
    };
    fakeState.deviceStatus = {
      revision: 1,
      state: "virtual",
      transport: "simulator",
      message: null,
      firmwareVersion: "0.1.0",
      retryCount: 0,
    };
    fakeState.loading = false;
    fakeState.error = null;
    loadMock.mockClear();
    refreshAdapterStatusMock.mockClear();
    refreshDeviceStatusMock.mockClear();
    setPresentationMock.mockReset();
    manualBindMock.mockReset();
    manualBindMock.mockResolvedValue(snapshot);
    swapSlotsMock.mockReset();
    swapSlotsMock.mockResolvedValue(snapshot);
    const reactiveState = reactive(fakeState);
    mountedState = reactiveState;
    setPresentationMock.mockImplementation(async (input) => {
      const next = {
        ...reactiveState.snapshot!,
        revision: reactiveState.snapshot!.revision + 1,
        ...input,
      };
      reactiveState.snapshot = next;
      return next;
    });
    startMock.mockClear();
    stopMock.mockClear();
    createHaloStoreMock.mockReturnValue({
      state: reactiveState,
      load: loadMock,
      refreshAdapterStatus: refreshAdapterStatusMock,
      refreshDeviceStatus: refreshDeviceStatusMock,
      setPresentation: setPresentationMock,
      manualBind: manualBindMock,
      swapSlots: swapSlotsMock,
      start: startMock,
      stop: stopMock,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = "";
  });

  it("挂载时加载快照、读取适配器并启动订阅，卸载时停止订阅", async () => {
    const wrapper = mount(App);
    await flushPromises();

    expect(loadMock).toHaveBeenCalledTimes(1);
    expect(refreshAdapterStatusMock).toHaveBeenCalledTimes(1);
    expect(refreshDeviceStatusMock).toHaveBeenCalledTimes(1);
    expect(startMock).toHaveBeenCalledTimes(1);

    wrapper.unmount();
    expect(stopMock).toHaveBeenCalledTimes(1);
  });

  it("每 30 秒老化最近活动时间，并在卸载时清理时钟", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);

    expect(rail.props("nowMs")).toBe(1_000_000);
    expect(vi.getTimerCount()).toBe(1);

    vi.advanceTimersByTime(30_000);
    await nextTick();
    expect(rail.props("nowMs")).toBe(1_030_000);

    wrapper.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("顶部明确显示虚拟设备与适配器状态，并组合完整控制中心", () => {
    const wrapper = mount(App);

    expect(wrapper.get("[data-device-status]").text()).toContain("VIRTUAL");
    expect(wrapper.get("[data-adapter-state]").attributes("data-adapter-state")).toBe(
      "degraded",
    );
    expect(wrapper.get("[data-adapter-state]").text()).toContain("DEGRADED");
    expect(wrapper.get("[data-adapter-state]").text()).toContain("DEMO");
    expect(wrapper.findComponent(HaloPreview).exists()).toBe(true);
    expect(wrapper.findComponent(CentralDisplay).exists()).toBe(true);
    expect(wrapper.findComponent(CrownControl).exists()).toBe(true);
    expect(wrapper.get("[data-task-rail]").element).toBeTruthy();
    expect(wrapper.get("[data-activity-strip]").element).toBeTruthy();
    expect(wrapper.get("[data-queue-task]").text()).toContain("QUEUE 01");
  });

  it.each([
    ["simulator", "virtual"],
    ["serial", "online"],
  ] as const)(
    "%s transport 使用不误导实体设备状态的通用界面文案",
    (transport, state) => {
      fakeState.deviceStatus = {
        ...fakeState.deviceStatus,
        transport,
        state,
      };
      fakeState.loading = true;
      const wrapper = mount(App);

      expect(wrapper.get(".header-readouts").attributes("aria-label")).toBe(
        "设备摘要",
      );
      expect(wrapper.get("[data-loading]").text()).toContain(
        "正在同步设备快照",
      );
      expect(wrapper.get(".device-workspace").attributes("aria-label")).toBe(
        "设备工作区",
      );
    },
  );

  it("routes ring and crown presentation through the authoritative store", async () => {
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    const display = wrapper.findComponent(CentralDisplay);
    const crown = wrapper.findComponent(CrownControl);

    expect(display.props("mode")).toBe("ambient");
    expect(preview.props("selectedSlot")).toBe(null);

    await preview.vm.$emit("select", 2);
    await flushPromises();
    expect(setPresentationMock).toHaveBeenLastCalledWith({
      displayMode: "ambient",
      selectedSlot: 2,
    });
    expect(preview.props("selectedSlot")).toBe(2);
    expect(display.props("selectedSlot")).toBe(2);

    await crown.vm.$emit("update:mode", "detail");
    await flushPromises();
    expect(setPresentationMock).toHaveBeenLastCalledWith({
      displayMode: "detail",
      selectedSlot: 2,
    });
    expect(display.props("mode")).toBe("detail");

    await crown.vm.$emit("select", 3);
    await flushPromises();
    expect(setPresentationMock).toHaveBeenLastCalledWith({
      displayMode: "detail",
      selectedSlot: 3,
    });
    expect(preview.props("selectedSlot")).toBe(3);

    await wrapper.get("[data-central-display]").trigger("click");
    expect(preview.props("selectedSlot")).toBe(3);
  });

  it("serializes presentation intents and builds each payload from the latest snapshot", async () => {
    const slotGate = deferred<void>();
    const modeGate = deferred<void>();
    const gates = [slotGate, modeGate];
    let commandIndex = 0;
    setPresentationMock.mockImplementation(async (input) => {
      await gates[commandIndex++].promise;
      const next = {
        ...mountedState.snapshot!,
        revision: mountedState.snapshot!.revision + 1,
        ...input,
      };
      mountedState.snapshot = next;
      return next;
    });
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    const display = wrapper.findComponent(CentralDisplay);
    const crown = wrapper.findComponent(CrownControl);

    preview.vm.$emit("select", 2);
    crown.vm.$emit("update:mode", "detail");
    await nextTick();

    modeGate.resolve(undefined);
    await flushPromises();
    expect(setPresentationMock).toHaveBeenCalledTimes(1);
    expect(setPresentationMock).toHaveBeenLastCalledWith({
      displayMode: "ambient",
      selectedSlot: 2,
    });
    expect(mountedState.snapshot).toMatchObject({
      revision: 1,
      displayMode: "ambient",
      selectedSlot: null,
    });

    slotGate.resolve(undefined);
    await flushPromises();

    expect(setPresentationMock.mock.calls).toEqual([
      [{ displayMode: "ambient", selectedSlot: 2 }],
      [{ displayMode: "detail", selectedSlot: 2 }],
    ]);
    expect(preview.props("selectedSlot")).toBe(2);
    expect(display.props()).toMatchObject({
      mode: "detail",
      selectedSlot: 2,
    });
  });

  it("derives task focus from the authoritative snapshot when an equal revision event wins", async () => {
    const authoritativeSnapshots = [
      {
        ...snapshot,
        revision: 2,
        selectedSlot: 1,
      },
      {
        ...snapshot,
        revision: 3,
        selectedSlot: 0,
      },
    ];
    let responseIndex = 0;
    setPresentationMock.mockImplementation(async () => {
      mountedState.snapshot = authoritativeSnapshots[responseIndex++];
      return mountedState.snapshot;
    });
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    const rail = wrapper.findComponent(TaskRail);
    const controls = wrapper.findComponent(BindingControls);

    preview.vm.$emit("select", 2);
    await flushPromises();
    expect(controls.props("selectedTask")).toMatchObject({
      taskKey: records[1].taskKey,
    });

    rail.vm.$emit("select-task", records[3].taskKey);
    await flushPromises();
    expect(controls.props("selectedTask")).toMatchObject({
      taskKey: records[0].taskKey,
    });
  });

  it("cancels queued presentation intents and ignores in-flight completion after unmount", async () => {
    const inFlight = deferred<HaloSnapshot | null>();
    const slotsRead = vi.fn();
    const lateSnapshot: HaloSnapshot = {
      ...snapshot,
      revision: 2,
      selectedSlot: 2,
      get slots() {
        slotsRead();
        return snapshot.slots;
      },
    };
    setPresentationMock.mockReturnValueOnce(inFlight.promise);
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    const crown = wrapper.findComponent(CrownControl);

    preview.vm.$emit("select", 2);
    crown.vm.$emit("update:mode", "detail");
    await nextTick();
    expect(setPresentationMock).toHaveBeenCalledTimes(1);

    wrapper.unmount();
    inFlight.resolve(lateSnapshot);
    await flushPromises();

    expect(setPresentationMock).toHaveBeenCalledTimes(1);
    expect(slotsRead).not.toHaveBeenCalled();
  });

  it("does not dispatch presentation when manualBind completes after unmount", async () => {
    const binding = deferred<HaloSnapshot | null>();
    manualBindMock.mockReturnValueOnce(binding.promise);
    const wrapper = mount(App);
    const controls = wrapper.findComponent(BindingControls);

    controls.vm.$emit("bind", records[2].taskKey, 3);
    await nextTick();
    expect(manualBindMock).toHaveBeenCalledWith({
      taskKey: records[2].taskKey,
      slot: 3,
      lock: false,
    });

    wrapper.unmount();
    binding.resolve({ ...snapshot, revision: 2 });
    await flushPromises();

    expect(setPresentationMock).not.toHaveBeenCalled();
  });

  it("does not dispatch presentation when swapSlots completes after unmount", async () => {
    const swapping = deferred<HaloSnapshot | null>();
    swapSlotsMock.mockReturnValueOnce(swapping.promise);
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);

    preview.vm.$emit("dragstart", {
      kind: "slot",
      slot: 0,
      taskKey: records[0].taskKey,
    });
    preview.vm.$emit("drop", 2);
    await nextTick();
    expect(swapSlotsMock).toHaveBeenCalledWith(0, 2);

    wrapper.unmount();
    swapping.resolve({ ...snapshot, revision: 2 });
    await flushPromises();

    expect(setPresentationMock).not.toHaveBeenCalled();
  });

  it("routes task selection through setPresentation without optimistic mutation", async () => {
    const pending = new Promise<HaloSnapshot>(() => undefined);
    setPresentationMock.mockReturnValueOnce(pending);
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const preview = wrapper.findComponent(HaloPreview);

    await rail.vm.$emit("select-task", records[2].taskKey);
    await nextTick();

    expect(setPresentationMock).toHaveBeenCalledWith({
      displayMode: "ambient",
      selectedSlot: 2,
    });
    expect(preview.props("selectedSlot")).toBe(null);
  });

  it("preserves presentation and task focus when every presentation command fails", async () => {
    fakeState.snapshot = {
      ...snapshot,
      displayMode: "overview",
      selectedSlot: 1,
    };
    setPresentationMock.mockResolvedValue(null);
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    const display = wrapper.findComponent(CentralDisplay);
    const crown = wrapper.findComponent(CrownControl);
    const rail = wrapper.findComponent(TaskRail);
    const controls = wrapper.findComponent(BindingControls);

    await preview.vm.$emit("select", 2);
    await crown.vm.$emit("update:mode", "detail");
    await rail.vm.$emit("select-task", records[2].taskKey);
    await flushPromises();

    expect(setPresentationMock.mock.calls).toEqual([
      [{ displayMode: "overview", selectedSlot: 2 }],
      [{ displayMode: "detail", selectedSlot: 1 }],
      [{ displayMode: "overview", selectedSlot: 2 }],
    ]);
    expect(preview.props("selectedSlot")).toBe(1);
    expect(display.props()).toMatchObject({
      mode: "overview",
      selectedSlot: 1,
    });
    expect(controls.props("selectedTask")).toBeNull();
  });

  it("requires authoritative snapshot and device APIs without compatibility guards", () => {
    expect(haloTypesSource).toContain("displayMode: DisplayMode;");
    expect(haloTypesSource).toContain("selectedSlot: number | null;");
    expect(haloTypesSource).not.toContain("displayMode?: DisplayMode;");
    expect(haloTypesSource).not.toContain("selectedSlot?: number | null;");
    expect(appSource).not.toContain('"selectedSlot" in snapshot');
    expect(appSource).not.toContain('typeof store.setPresentation');
    expect(appSource).not.toContain('typeof store.refreshDeviceStatus');
    expect(haloStoreSource).not.toContain('typeof bridge.getDeviceStatus');
    expect(haloStoreSource).not.toContain('typeof bridge.subscribeDeviceStatus');
  });

  it("keeps ring and adapter UI visible for incompatible devices", () => {
    fakeState.deviceStatus = {
      ...fakeState.deviceStatus,
      revision: 2,
      state: "incompatible",
      transport: "serial",
      message: "Protocol version mismatch",
    };
    const wrapper = mount(App);

    expect(wrapper.get("[data-device-status]").text()).toContain("INCOMPATIBLE");
    expect(wrapper.get("[data-adapter-state]").element).toBeTruthy();
    expect(wrapper.findComponent(HaloPreview).exists()).toBe(true);
  });

  it.each([
    ["online", "ONLINE"],
    ["degraded", "DEGRADED"],
    ["offline", "OFFLINE"],
  ] as const)("可辨认适配器 %s 状态", (state, label) => {
    fakeState.adapterStatus = {
      revision: 2,
      state,
      mode: "hook",
      message: null,
      acceptedEvents: 4,
      ignoredEvents: 1,
      rejectedEvents: 0,
    };
    const wrapper = mount(App);

    expect(wrapper.get("[data-adapter-state]").text()).toContain(label);
  });

  it("loading、error、offline 和 demo 都有明确且不阻断设备 DOM 的反馈", () => {
    fakeState.loading = true;
    fakeState.error = {
      operation: "load",
      code: "bridgeFailure",
      message: "load 操作失败",
    };
    fakeState.adapterStatus = {
      revision: 1,
      state: "offline",
      mode: "demo",
      message: "未连接 Hook",
      acceptedEvents: 0,
      ignoredEvents: 0,
      rejectedEvents: 1,
    };
    fakeState.snapshot = null;

    const wrapper = mount(App);

    expect(wrapper.get("[data-loading]").text()).toContain("正在同步");
    expect(wrapper.get("[data-app-error]").text()).toContain("load 操作失败");
    expect(wrapper.get("[data-adapter-state]").text()).toContain("OFFLINE");
    expect(wrapper.get("[data-adapter-state]").text()).toContain("DEMO");
    expect(wrapper.findComponent(HaloPreview).exists()).toBe(true);
    expect(wrapper.get("[data-task-rail]").element).toBeTruthy();
  });

  it("控制中心所有可见 DOM 均保持匿名", () => {
    const wrapper = mount(App);

    for (const record of records) {
      expect(wrapper.html()).not.toContain(record.taskKey);
    }
  });

  it("中央屏位于内圈安全区且独立于圆环点击层，表冠锚定设备容器", () => {
    const wrapper = mount(App);
    const stage = wrapper.get(".device-stage");
    const preview = wrapper.get(".halo-preview");
    const displayLayer = wrapper.get(".device-display-layer");
    const centralDisplay = wrapper.get("[data-central-display]");

    expect(stage.element.contains(preview.element)).toBe(true);
    expect(stage.element.contains(displayLayer.element)).toBe(true);
    expect(preview.element.contains(centralDisplay.element)).toBe(false);
    expect(displayLayer.element.contains(centralDisplay.element)).toBe(true);

    expect(baseStyles).toMatch(
      /\.device-stage\s*\{[\s\S]*position:\s*relative/,
    );
    expect(baseStyles).toMatch(
      /\.device-display-layer\s*\{[\s\S]*position:\s*absolute[\s\S]*z-index:\s*25[\s\S]*pointer-events:\s*none/,
    );
    expect(baseStyles).toMatch(
      /\.app-shell \.device-display-layer \.central-display\s*\{[\s\S]*width:\s*calc\(\s*var\(--halo-ring-inner-size\)\s*-\s*var\(--halo-ring-width\)\s*-\s*var\(--halo-ring-width\)\s*-\s*0\.25rem\s*\)[\s\S]*min-width:\s*0[\s\S]*max-width:\s*calc\(\s*var\(--halo-ring-inner-size\)\s*-\s*var\(--halo-ring-width\)\s*-\s*var\(--halo-ring-width\)\s*-\s*0\.25rem\s*\)[\s\S]*pointer-events:\s*auto/,
    );
    expect(baseStyles).not.toMatch(
      /\.device-display-layer \.central-display\s*\{[\s\S]*width:\s*\d+%/,
    );
    expect(baseStyles).toMatch(
      /\.device-stage > \.crown-control\s*\{[\s\S]*z-index:\s*30/,
    );
  });

  it("1440×900 使用视口网格和内部滚动契约，不让页面本身裁切滚动", () => {
    expect(baseStyles).toMatch(
      /html\s*\{[\s\S]*overflow:\s*hidden/,
    );
    expect(baseStyles).toMatch(
      /#app\s*\{[\s\S]*height:\s*100dvh/,
    );
    expect(baseStyles).toMatch(
      /\.app-shell\s*\{[\s\S]*height:\s*100%[\s\S]*grid-template-rows:\s*auto minmax\(0, 1fr\) auto[\s\S]*overflow:\s*hidden/,
    );
    expect(taskRailSource).toMatch(
      /\.task-rail-content\s*\{[\s\S]*overflow:\s*auto/,
    );
  });
});
