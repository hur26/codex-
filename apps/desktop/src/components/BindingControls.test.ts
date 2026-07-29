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
import bindingControlsSource from "./BindingControls.vue?raw";
import HaloPreview from "./HaloPreview.vue";
import TaskRail from "./TaskRail.vue";

enableAutoUnmount(afterEach);

const PRIVATE_TASK_KEY = "f15a5c619d774ed1";
let mountedState: {
  snapshot: HaloSnapshot | null;
  adapterStatus: AdapterStatus;
  loading: boolean;
  error: { operation: string; code: string; message: string } | null;
};

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
  revision: 1,
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
  it("使用原生按钮完成键盘绑定，单次激活只发出一次意图", async () => {
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
    expect(wrapper.emitted("bind")).toBeUndefined();
    await buttons[2].trigger("click");

    expect(wrapper.emitted("bind")).toEqual([[PRIVATE_TASK_KEY, 2]]);
    expect(bindingControlsSource).not.toContain("@keydown");
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
    mountedState = reactive(fakeState);
    createHaloStoreMock.mockReturnValue({
      state: mountedState,
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
      origin: { kind: "slot", slot: 0 },
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
    await preview.vm.$emit("dragstart", {
      kind: "slot",
      slot: 0,
      taskKey: PRIVATE_TASK_KEY,
    });
    await preview.vm.$emit("drop", 0);
    await preview.vm.$emit("dragstart", {
      kind: "slot",
      slot: 0,
      taskKey: PRIVATE_TASK_KEY,
    });
    await preview.vm.$emit("drop", 3);
    await flushPromises();

    expect(swapSlotsMock).toHaveBeenCalledTimes(1);
    expect(swapSlotsMock).toHaveBeenCalledWith(0, 3);
  });

  it("拖拽途中实时快照替换源任务时拒绝陈旧的 slot 交换", async () => {
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);

    await preview.vm.$emit("dragstart", {
      kind: "slot",
      slot: 0,
      taskKey: PRIVATE_TASK_KEY,
    });
    mountedState.snapshot = {
      ...initialSnapshot,
      slots: [
        slot(0, "3333333333333333"),
        ...initialSnapshot.slots.slice(1),
      ],
    };
    await nextTick();
    await preview.vm.$emit("drop", 3);
    await flushPromises();

    expect(swapSlotsMock).not.toHaveBeenCalled();
  });

  it("队列任务被自动绑定后立即取消旧 queue 拖拽与放置反馈", async () => {
    const queueTask = task("4444444444444444");
    queueTask.status = "queued";
    mountedState.snapshot = {
      ...initialSnapshot,
      tasks: [...initialSnapshot.tasks, queueTask],
      queue: [queueTask],
    };
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const preview = wrapper.findComponent(HaloPreview);
    const target = preview.get('[data-slot="3"]');
    const dataTransfer = {
      effectAllowed: "none",
      dropEffect: "none",
      types: ["application/x-codex-halo-drag"],
      setData: vi.fn(),
      getData: vi.fn(() => "task"),
    };

    await rail.get("[data-queue-task] button").trigger("dragstart", {
      dataTransfer,
    });
    await target.trigger("dragenter", { dataTransfer });
    expect(preview.props("dragActive")).toBe(true);
    expect(target.attributes("data-drop-active")).toBe("true");
    expect(wrapper.html()).not.toContain(queueTask.taskKey);

    mountedState.snapshot = {
      ...initialSnapshot,
      slots: [
        ...initialSnapshot.slots.slice(0, 2),
        slot(2, queueTask.taskKey),
        initialSnapshot.slots[3],
      ],
      tasks: [...initialSnapshot.tasks, queueTask],
      queue: [],
    };
    await nextTick();

    expect(preview.props("dragActive")).toBe(false);
    expect(target.attributes("data-drop-active")).toBe("false");
    expect(target.attributes("aria-label")).not.toContain("释放以完成绑定");
    expect(wrapper.html()).not.toContain(queueTask.taskKey);
    await target.trigger("drop", { dataTransfer });
    await flushPromises();
    expect(manualBindMock).not.toHaveBeenCalled();
  });

  it("任务行来源圈位发生换位时取消旧 slot-origin 拖拽", async () => {
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const preview = wrapper.findComponent(HaloPreview);
    const dataTransfer = {
      effectAllowed: "none",
      dropEffect: "none",
      types: ["application/x-codex-halo-drag"],
      setData: vi.fn(),
      getData: vi.fn(() => "task"),
    };

    await rail.get('[data-task-slot="0"]').trigger("dragstart", {
      dataTransfer,
    });
    expect(preview.props("dragActive")).toBe(true);

    mountedState.snapshot = {
      ...initialSnapshot,
      slots: [
        slot(0, null),
        initialSnapshot.slots[1],
        slot(2, PRIVATE_TASK_KEY),
        initialSnapshot.slots[3],
      ],
    };
    await nextTick();

    expect(preview.props("dragActive")).toBe(false);
    await preview.get('[data-slot="3"]').trigger("drop", { dataTransfer });
    await flushPromises();
    expect(manualBindMock).not.toHaveBeenCalled();
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
    await button.trigger("click");
    await flushPromises();

    expect(manualBindMock).toHaveBeenCalledTimes(1);
    expect(manualBindMock).toHaveBeenCalledWith({
      taskKey: PRIVATE_TASK_KEY,
      slot: 3,
      lock: false,
    });
  });

  it("任务已在锁定目标圈时绑定为 no-op，不调用 manualBind 或解除锁定", async () => {
    mountedState.snapshot = {
      ...initialSnapshot,
      slots: [
        { ...initialSnapshot.slots[0], locked: true },
        ...initialSnapshot.slots.slice(1),
      ],
    };
    const lockedSnapshot = fakeState.snapshot;
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);

    await rail.vm.$emit("select-task", PRIVATE_TASK_KEY);
    await nextTick();
    await wrapper
      .findComponent(BindingControls)
      .get('[data-bind-slot="0"]')
      .trigger("click");
    await flushPromises();

    expect(manualBindMock).not.toHaveBeenCalled();
    expect(fakeState.snapshot).toBe(lockedSnapshot);
    expect(fakeState.snapshot?.slots[0].locked).toBe(true);
  });

  it("锁定圈拒绝队列任务绑定后保留选择与快照并显示错误", async () => {
    const queuedTaskKey = "3333333333333333";
    const queuedTask = task(queuedTaskKey);
    mountedState.snapshot = {
      ...initialSnapshot,
      revision: 2,
      slots: [
        { ...initialSnapshot.slots[0], locked: true },
        ...initialSnapshot.slots.slice(1),
      ],
      tasks: [...initialSnapshot.tasks, queuedTask],
      queue: [{ ...queuedTask, status: "queued" }],
    };
    const lockedSnapshot = mountedState.snapshot;
    manualBindMock.mockImplementation(async () => {
      mountedState.error = {
        operation: "manualBind",
        code: "slotLocked",
        message: "manualBind 操作失败",
      };
      return null;
    });
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const controls = wrapper.findComponent(BindingControls);

    await rail.vm.$emit("select-task", queuedTaskKey);
    await nextTick();
    expect(controls.props("selectedTask")).toMatchObject({
      taskKey: queuedTaskKey,
    });
    expect(controls.props("selectedSlot")).toBeNull();

    await controls.get('[data-bind-slot="0"]').trigger("click");
    await flushPromises();

    expect(manualBindMock).toHaveBeenCalledWith({
      taskKey: queuedTaskKey,
      slot: 0,
      lock: false,
    });
    expect(mountedState.snapshot).toBe(lockedSnapshot);
    expect(mountedState.snapshot?.slots[0]).toMatchObject({
      taskKey: PRIVATE_TASK_KEY,
      locked: true,
    });
    expect(controls.props("selectedTask")).toMatchObject({
      taskKey: queuedTaskKey,
    });
    expect(controls.props("selectedSlot")).toBeNull();
    expect(wrapper.get("[data-app-error]").text()).toContain(
      "manualBind 操作失败",
    );
  });

  it("dragend、Escape、窗口失焦和源节点移除都会取消活动拖拽", async () => {
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);
    const preview = wrapper.findComponent(HaloPreview);

    for (const cancel of [
      async () => rail.vm.$emit("dragend"),
      async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })),
      async () => window.dispatchEvent(new Event("blur")),
    ]) {
      await rail.vm.$emit("dragstart", {
        kind: "task",
        taskKey: PRIVATE_TASK_KEY,
        origin: { kind: "slot", slot: 0 },
      });
      await nextTick();
      expect(preview.props("dragActive")).toBe(true);
      await cancel();
      await nextTick();
      expect(preview.props("dragActive")).toBe(false);
      await preview.vm.$emit("drop", 3);
    }

    await rail.vm.$emit("dragstart", {
      kind: "task",
      taskKey: PRIVATE_TASK_KEY,
      origin: { kind: "slot", slot: 0 },
    });
    mountedState.snapshot = {
      ...initialSnapshot,
      slots: initialSnapshot.slots.map((candidate) =>
        candidate.taskKey === PRIVATE_TASK_KEY
          ? slot(candidate.index, null)
          : candidate,
      ),
      tasks: initialSnapshot.tasks.filter(
        (candidate) => candidate.taskKey !== PRIVATE_TASK_KEY,
      ),
    };
    await nextTick();
    expect(preview.props("dragActive")).toBe(false);
    await preview.vm.$emit("drop", 3);
    await flushPromises();

    expect(manualBindMock).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("卸载时移除全局取消监听，避免拖拽状态跨页面残留", async () => {
    const addSpy = vi.spyOn(window, "addEventListener");
    const removeSpy = vi.spyOn(window, "removeEventListener");
    const wrapper = mount(App);
    const rail = wrapper.findComponent(TaskRail);

    await rail.vm.$emit("dragstart", {
      kind: "task",
      taskKey: PRIVATE_TASK_KEY,
      origin: { kind: "slot", slot: 0 },
    });
    await nextTick();

    const blurHandler = addSpy.mock.calls.find(([type]) => type === "blur")?.[1];
    const keyHandler = addSpy.mock.calls.find(([type]) => type === "keydown")?.[1];
    expect(blurHandler).toBeTypeOf("function");
    expect(keyHandler).toBeTypeOf("function");

    wrapper.unmount();

    expect(removeSpy).toHaveBeenCalledWith("blur", blurHandler);
    expect(removeSpy).toHaveBeenCalledWith("keydown", keyHandler);
    addSpy.mockRestore();
    removeSpy.mockRestore();
  });

  it("操作失败保留旧快照并由 App 的 alert 显示错误", async () => {
    const oldSnapshot = mountedState.snapshot;
    manualBindMock.mockImplementation(async () => {
      mountedState.error = {
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
      origin: { kind: "slot", slot: 0 },
    });
    await preview.vm.$emit("drop", 2);
    await flushPromises();

    expect(mountedState.snapshot).toBe(oldSnapshot);
    expect(wrapper.get("[data-app-error]").attributes("role")).toBe("alert");
    expect(wrapper.get("[data-app-error]").text()).toContain(
      "manualBind 操作失败",
    );
  });
});
