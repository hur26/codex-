import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { nextTick, reactive } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AdapterStatus, HaloSnapshot, RingSlot, TaskRecord } from "./types/halo";

const {
  createHaloStoreMock,
  loadMock,
  refreshAdapterStatusMock,
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
    loading: false,
    error: null as { operation: string; code: string; message: string } | null,
  };

  return {
    createHaloStoreMock: vi.fn(),
    loadMock: vi.fn(() => Promise.resolve()),
    refreshAdapterStatusMock: vi.fn(() => Promise.resolve()),
    startMock: vi.fn(() => Promise.resolve(true)),
    stopMock: vi.fn(() => Promise.resolve()),
    fakeState: state,
  };
});

vi.mock("./stores/haloStore", () => ({
  createHaloStore: createHaloStoreMock,
}));

import App from "./App.vue";
import CentralDisplay from "./components/CentralDisplay.vue";
import CrownControl from "./components/CrownControl.vue";
import HaloPreview from "./components/HaloPreview.vue";
import TaskRail from "./components/TaskRail.vue";
import taskRailSource from "./components/TaskRail.vue?raw";

const baseStyles = readFileSync(
  resolve(process.cwd(), "src/styles/base.css"),
  "utf8",
);

enableAutoUnmount(afterEach);

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
    fakeState.loading = false;
    fakeState.error = null;
    loadMock.mockClear();
    refreshAdapterStatusMock.mockClear();
    startMock.mockClear();
    stopMock.mockClear();
    createHaloStoreMock.mockReturnValue({
      state: reactive(fakeState),
      load: loadMock,
      refreshAdapterStatus: refreshAdapterStatusMock,
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

    expect(wrapper.get("[data-app-header]").text()).toContain("VIRTUAL DEVICE");
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

  it("圆环与表冠共同管理 selectedSlot/displayMode，中央点击不会误选内圈", async () => {
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    const display = wrapper.findComponent(CentralDisplay);
    const crown = wrapper.findComponent(CrownControl);

    expect(display.props("mode")).toBe("ambient");
    expect(preview.props("selectedSlot")).toBe(null);

    await preview.vm.$emit("select", 2);
    expect(preview.props("selectedSlot")).toBe(2);
    expect(display.props("selectedSlot")).toBe(2);

    await crown.vm.$emit("update:mode", "detail");
    expect(display.props("mode")).toBe("detail");

    await crown.vm.$emit("select", 3);
    expect(preview.props("selectedSlot")).toBe(3);

    await wrapper.get("[data-central-display]").trigger("click");
    expect(preview.props("selectedSlot")).toBe(3);
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
