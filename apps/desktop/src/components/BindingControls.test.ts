import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { nextTick, reactive } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AdapterStatus,
  HaloSnapshot,
  RingSlot,
  TaskRecord,
} from "../types/halo";

const {
  createHaloStoreMock,
  loadMock,
  refreshAdapterStatusMock,
  startMock,
  stopMock,
  manualBindMock,
  swapSlotsMock,
  toggleLockMock,
  fakeState,
} = vi.hoisted(() => {
  const state = {
    snapshot: null as HaloSnapshot | null,
    adapterStatus: {
      state: "online",
      mode: "demo",
      message: null,
    } as AdapterStatus,
    loading: false,
    error: null as { operation: string; code: string; message: string } | null,
  };

  return {
    createHaloStoreMock: vi.fn(),
    loadMock: vi.fn(() => Promise.resolve()),
    refreshAdapterStatusMock: vi.fn(() => Promise.resolve()),
    startMock: vi.fn(() => Promise.resolve()),
    stopMock: vi.fn(() => Promise.resolve()),
    manualBindMock: vi.fn(() => Promise.resolve(state.snapshot)),
    swapSlotsMock: vi.fn(() => Promise.resolve(state.snapshot)),
    toggleLockMock: vi.fn(() => Promise.resolve(state.snapshot)),
    fakeState: state,
  };
});

vi.mock("../stores/haloStore", () => ({
  createHaloStore: createHaloStoreMock,
}));

import App from "../App.vue";
import BindingControls from "./BindingControls.vue";
import HaloPreview from "./HaloPreview.vue";
import TaskRail from "./TaskRail.vue";

enableAutoUnmount(afterEach);

const PRIVATE_TASK_KEY = "f15a5c619d774ed1";

function task(taskKey = PRIVATE_TASK_KEY): TaskRecord {
  return {
    taskKey,
    status: "running",
    source: "hook",
    confidence: "observed",
    lastActiveAtMs: 100_000,
  };
}

function slot(
  index: number,
  taskKey: string | null,
  locked = false,
): RingSlot {
  return {
    index,
    taskKey,
    status: taskKey ? "running" : "idle",
    source: taskKey ? "hook" : null,
    confidence: taskKey ? "observed" : null,
    bindingMode: taskKey ? "manual" : "auto",
    locked,
    effect: {
      brightness: 80,
      speedPercent: 100,
      direction: "clockwise",
      tailPercent: 35,
    },
  };
}

const selectedTask = task();
const initialSnapshot: HaloSnapshot = {
  deviceMode: "virtual",
  globalBrightness: 80,
  slots: [
    slot(0, PRIVATE_TASK_KEY),
    slot(1, "2222222222222222", true),
    slot(2, null),
    slot(3, null),
  ],
  tasks: [selectedTask, task("2222222222222222")],
  queue: [],
};

describe("BindingControls", () => {
  it("以匿名语义提供四个圈位按钮，并精确发出键盘绑定意图", async () => {
    const wrapper = mount(BindingControls, {
      props: {
        selectedTask,
        selectedSlot: initialSnapshot.slots[0],
        loading: false,
      },
    });

    const buttons = wrapper.findAll("[data-bind-slot]");
    expect(buttons).toHaveLength(4);
    expect(buttons.every((button) => button.element.tagName === "BUTTON")).toBe(
      true,
    );

    await buttons[2].trigger("keydown", { key: "Enter" });

    expect(wrapper.emitted("bind")).toEqual([[PRIVATE_TASK_KEY, 2]]);
    expect(wrapper.html()).not.toContain(PRIVATE_TASK_KEY);
    expect(wrapper.html()).not.toContain("taskKey");
  });

  it("锁按钮反映当前圈状态并精确发出 toggleLock 意图", async () => {
    const wrapper = mount(BindingControls, {
      props: {
        selectedTask,
        selectedSlot: initialSnapshot.slots[1],
        loading: false,
      },
    });

    const lock = wrapper.get("[data-lock-control]");
    expect(lock.text()).toContain("解除锁定");
    await lock.trigger("click");

    expect(wrapper.emitted("toggle-lock")).toEqual([[1]]);
  });

  it("没有任务、没有已绑定圈或命令执行中时合理禁用操作", async () => {
    const wrapper = mount(BindingControls, {
      props: {
        selectedTask: null,
        selectedSlot: null,
        loading: false,
      },
    });

    expect(
      wrapper
        .findAll("[data-bind-slot]")
        .every((button) => button.attributes("disabled") !== undefined),
    ).toBe(true);
    expect(wrapper.get("[data-lock-control]").attributes("disabled")).toBe("");

    await wrapper.setProps({
      selectedTask,
      selectedSlot: initialSnapshot.slots[0],
      loading: true,
    });
    expect(
      wrapper
        .findAll("button")
        .every((button) => button.attributes("disabled") !== undefined),
    ).toBe(true);
  });
});

describe("App binding orchestration", () => {
  beforeEach(() => {
    fakeState.snapshot = initialSnapshot;
    fakeState.loading = false;
    fakeState.error = null;
    for (const mock of [
      loadMock,
      refreshAdapterStatusMock,
      startMock,
      stopMock,
      manualBindMock,
      swapSlotsMock,
      toggleLockMock,
    ]) {
      mock.mockClear();
    }
    manualBindMock.mockImplementation(() => Promise.resolve(fakeState.snapshot));
    swapSlotsMock.mockImplementation(() => Promise.resolve(fakeState.snapshot));
    toggleLockMock.mockImplementation(() => Promise.resolve(fakeState.snapshot));
    createHaloStoreMock.mockReturnValue({
      state: reactive(fakeState),
      load: loadMock,
      refreshAdapterStatus: refreshAdapterStatusMock,
      start: startMock,
      stop: stopMock,
      manualBind: manualBindMock,
      swapSlots: swapSlotsMock,
      toggleLock: toggleLockMock,
    });
  });

  it("任务拖到圆环调用 manualBind，内部标识不进入 HTML 或 DataTransfer 文本", async () => {
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const preview = wrapper.findComponent(HaloPreview);

    await rail.vm.$emit("dragstart", {
      kind: "task",
      taskKey: PRIVATE_TASK_KEY,
    });
    await preview.vm.$emit("drop", 2);
    await flushPromises();

    expect(manualBindMock).toHaveBeenCalledTimes(1);
    expect(manualBindMock).toHaveBeenCalledWith({
      taskKey: PRIVATE_TASK_KEY,
      slot: 2,
      lock: false,
    });
    expect(wrapper.html()).not.toContain(PRIVATE_TASK_KEY);

    const dataTransfer = {
      effectAllowed: "none",
      setData: vi.fn(),
    };
    await rail.get('[data-task-slot="0"]').trigger("dragstart", {
      dataTransfer,
    });
    expect(dataTransfer.setData).not.toHaveBeenCalledWith(
      "text/plain",
      expect.anything(),
    );
    expect(
      dataTransfer.setData.mock.calls.flat().join(" "),
    ).not.toContain(PRIVATE_TASK_KEY);
  });

  it("已绑定圆环拖到另一圈调用 swapSlots，同圈和无 payload 安全忽略", async () => {
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);

    await preview.vm.$emit("drop", 3);
    await preview.vm.$emit("dragstart", { kind: "slot", slot: 0 });
    await preview.vm.$emit("drop", 0);
    await preview.vm.$emit("dragstart", { kind: "slot", slot: 0 });
    await preview.vm.$emit("drop", 3);
    await flushPromises();

    expect(swapSlotsMock).toHaveBeenCalledTimes(1);
    expect(swapSlotsMock).toHaveBeenCalledWith(0, 3);
  });

  it("锁按钮连接 store.toggleLock 且命令参数精确", async () => {
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);

    await preview.vm.$emit("select", 1);
    await nextTick();
    await wrapper
      .findComponent(BindingControls)
      .vm.$emit("toggle-lock", 1);
    await flushPromises();

    expect(toggleLockMock).toHaveBeenCalledTimes(1);
    expect(toggleLockMock).toHaveBeenCalledWith(1);
  });

  it("键盘菜单对选中匿名任务绑定到指定圈位，不依赖拖拽", async () => {
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);

    await rail.vm.$emit("select-task", PRIVATE_TASK_KEY);
    await nextTick();
    const button = wrapper
      .findComponent(BindingControls)
      .get('[data-bind-slot="3"]');
    await button.trigger("keydown", { key: "Enter" });
    await flushPromises();

    expect(manualBindMock).toHaveBeenCalledTimes(1);
    expect(manualBindMock).toHaveBeenCalledWith({
      taskKey: PRIVATE_TASK_KEY,
      slot: 3,
      lock: false,
    });
  });

  it("操作失败保留旧快照并由 App 的 alert 显示错误", async () => {
    const oldSnapshot = fakeState.snapshot;
    manualBindMock.mockImplementation(async () => {
      fakeState.error = {
        operation: "manualBind",
        code: "engineRejected",
        message: "manualBind 操作失败",
      };
      return null;
    });
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const preview = wrapper.findComponent(HaloPreview);

    await rail.vm.$emit("dragstart", {
      kind: "task",
      taskKey: PRIVATE_TASK_KEY,
    });
    await preview.vm.$emit("drop", 2);
    await flushPromises();

    expect(fakeState.snapshot).toBe(oldSnapshot);
    expect(wrapper.get("[data-app-error]").attributes("role")).toBe("alert");
    expect(wrapper.get("[data-app-error]").text()).toContain(
      "manualBind 操作失败",
    );
  });
});
