import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { nextTick, reactive } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AdapterStatus,
  EffectProfile,
  HaloSnapshot,
  RingSlot,
  UpdateEffectInput,
} from "../types/halo";

const {
  createHaloStoreMock,
  loadMock,
  refreshAdapterStatusMock,
  startMock,
  stopMock,
  setGlobalBrightnessMock,
  updateEffectMock,
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
    startMock: vi.fn(() => Promise.resolve()),
    stopMock: vi.fn(() => Promise.resolve()),
    setGlobalBrightnessMock: vi.fn(),
    updateEffectMock: vi.fn(),
    fakeState: state,
  };
});

vi.mock("../stores/haloStore", () => ({
  createHaloStore: createHaloStoreMock,
}));

import App from "../App.vue";
import EffectEditor from "./EffectEditor.vue";
import HaloPreview from "./HaloPreview.vue";
import effectEditorSource from "./EffectEditor.vue?raw";

enableAutoUnmount(afterEach);

const DEFAULT_EFFECT: EffectProfile = {
  brightness: 80,
  speedPercent: 100,
  direction: "clockwise",
  tailPercent: 35,
};
let mountedState: {
  snapshot: HaloSnapshot | null;
  adapterStatus: AdapterStatus;
  loading: boolean;
  error: { operation: string; code: string; message: string } | null;
};

function slot(
  index: number,
  effect: EffectProfile = DEFAULT_EFFECT,
): RingSlot {
  return {
    index,
    taskKey: index === 0 ? "0123456789abcdef" : null,
    status: index === 0 ? "running" : "idle",
    source: index === 0 ? "simulator" : null,
    confidence: index === 0 ? "simulated" : null,
    bindingMode: "auto",
    locked: false,
    effect: { ...effect },
  };
}

function snapshot(
  globalBrightness = 80,
  effect: EffectProfile = DEFAULT_EFFECT,
): HaloSnapshot {
  return {
    revision: 1,
    deviceMode: "virtual",
    globalBrightness,
    slots: Array.from({ length: 4 }, (_, index) =>
      slot(index, index === 0 ? effect : DEFAULT_EFFECT),
    ),
    tasks: [
      {
        taskKey: "0123456789abcdef",
        status: "running",
        source: "simulator",
        confidence: "simulated",
        lastActiveAtMs: 100,
      },
    ],
    queue: [],
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function replaceEffect(
  current: HaloSnapshot,
  input: UpdateEffectInput,
): HaloSnapshot {
  return {
    ...current,
    slots: current.slots.map((candidate) =>
      candidate.index === input.slot
        ? {
            ...candidate,
            effect: {
              brightness: input.brightness,
              speedPercent: input.speedPercent,
              direction: input.direction,
              tailPercent: input.tailPercent,
            },
          }
        : candidate,
    ),
  };
}

describe("EffectEditor", () => {
  beforeEach(() => {
    fakeState.snapshot = snapshot();
    fakeState.loading = false;
    fakeState.error = null;
    for (const mock of [
      loadMock,
      refreshAdapterStatusMock,
      startMock,
      stopMock,
      setGlobalBrightnessMock,
      updateEffectMock,
    ]) {
      mock.mockReset();
    }
    loadMock.mockResolvedValue(undefined);
    refreshAdapterStatusMock.mockResolvedValue(undefined);
    startMock.mockResolvedValue(undefined);
    stopMock.mockResolvedValue(undefined);

    mountedState = reactive(fakeState);
    setGlobalBrightnessMock.mockImplementation(async (value: number) => {
      mountedState.snapshot = {
        ...mountedState.snapshot!,
        globalBrightness: value,
      };
      return mountedState.snapshot;
    });
    updateEffectMock.mockImplementation(async (input: UpdateEffectInput) => {
      mountedState.snapshot = replaceEffect(mountedState.snapshot!, input);
      return mountedState.snapshot;
    });
    createHaloStoreMock.mockReturnValue({
      state: mountedState,
      load: loadMock,
      refreshAdapterStatus: refreshAdapterStatusMock,
      start: startMock,
      stop: stopMock,
      manualBind: vi.fn(),
      swapSlots: vi.fn(),
      toggleLock: vi.fn(),
      setGlobalBrightness: setGlobalBrightnessMock,
      updateEffect: updateEffectMock,
    });
  });

  it("全局亮度使用带标签的原生刻度与数值输入，并只在提交时发出 0–100 数值", async () => {
    const wrapper = mount(EffectEditor, {
      props: {
        globalBrightness: 80,
        selectedSlot: null,
      },
    });

    const range = wrapper.get("[data-global-range]");
    const number = wrapper.get("[data-global-number]");
    expect(range.element.tagName).toBe("INPUT");
    expect(range.attributes()).toMatchObject({
      type: "range",
      min: "0",
      max: "100",
      "aria-label": "全局亮度",
    });
    expect(number.attributes("aria-describedby")).toContain(
      "global-brightness-unit",
    );
    expect(number.attributes("aria-describedby")).toContain(
      "global-brightness-range-error",
    );

    await number.setValue("64");

    expect(wrapper.emitted("set-global-brightness")).toEqual([[64]]);
    expect(wrapper.get("#global-brightness-unit").text()).toContain("%");
  });

  it("空物理圈也能编辑完整灯效，方向仅有顺时针/逆时针且没有颜色自由控件", async () => {
    const emptySlot = slot(2);
    const wrapper = mount(EffectEditor, {
      props: {
        globalBrightness: 80,
        selectedSlot: emptySlot,
      },
    });

    await wrapper.get("[data-effect-brightness-number]").setValue("55");
    await wrapper.get("[data-effect-speed-number]").setValue("225");
    await wrapper.get('[data-direction="counterClockwise"]').trigger("click");
    await wrapper.get("[data-effect-tail-number]").setValue("72");

    const updates = wrapper.emitted("update-effect") ?? [];
    expect(updates[updates.length - 1]).toEqual([
      {
        slot: 2,
        brightness: 55,
        speedPercent: 225,
        direction: "counterClockwise",
        tailPercent: 72,
      },
    ]);
    expect(wrapper.findAll("[data-direction]")).toHaveLength(2);
    expect(wrapper.find('[data-direction="clockwise"]').exists()).toBe(true);
    expect(
      wrapper.find('[data-direction="counterClockwise"]').exists(),
    ).toBe(true);
    expect(wrapper.find('input[type="color"]').exists()).toBe(false);
    expect(effectEditorSource).not.toMatch(/color\s*(picker|input)|自定义颜色/i);
  });

  it("NaN、非整数和越界值显示精确范围并阻止所有命令意图", async () => {
    const invalidCases = [
      ["[data-global-number]", "", "全局亮度必须是 0–100 的整数"],
      [
        "[data-effect-brightness-number]",
        "101",
        "单圈亮度必须是 0–100 的整数",
      ],
      [
        "[data-effect-speed-number]",
        "24",
        "追逐速度必须是 25–300 的整数",
      ],
      [
        "[data-effect-tail-number]",
        "1.5",
        "光尾长度必须是 1–100 的整数",
      ],
    ] as const;

    for (const [selector, value, error] of invalidCases) {
      const wrapper = mount(EffectEditor, {
        props: {
          globalBrightness: 80,
          selectedSlot: slot(0),
        },
      });
      await wrapper.get(selector).setValue(value);
      expect(wrapper.get("[data-validation-error]").text()).toBe(error);
      const field = wrapper.get(selector);
      const range = field
        .element.closest(".parameter-block, .editor-bank")
        ?.querySelector('input[type="range"]');
      expect(field.attributes("aria-invalid")).toBe("true");
      expect(field.attributes("aria-describedby")).toMatch(/range-error/);
      expect(range?.getAttribute("aria-invalid")).toBe("true");
      expect(range?.getAttribute("aria-describedby")).toMatch(/range-error/);
      expect(wrapper.emitted("set-global-brightness")).toBeUndefined();
      expect(wrapper.emitted("update-effect")).toBeUndefined();

      await field.setValue(
        selector.includes("speed")
          ? "25"
          : selector.includes("tail")
            ? "1"
            : "0",
      );
      expect(field.attributes("aria-invalid")).toBe("false");
      expect(wrapper.find("[data-validation-error]").exists()).toBe(false);
      wrapper.unmount();
    }
  });

  it("后端快照返回后预览立即采用全局/单圈亮度、速度、方向和光尾", async () => {
    const wrapper = mount(App);
    const preview = wrapper.findComponent(HaloPreview);
    await preview.vm.$emit("select", 0);
    await nextTick();

    const editor = wrapper.findComponent(EffectEditor);
    await editor.get("[data-global-number]").setValue("50");
    await flushPromises();
    await editor.get("[data-effect-brightness-number]").setValue("50");
    await flushPromises();
    await editor.get("[data-effect-speed-number]").setValue("300");
    await flushPromises();
    await editor
      .get('[data-direction="counterClockwise"]')
      .trigger("click");
    await flushPromises();
    await editor.get("[data-effect-tail-number]").setValue("100");
    await flushPromises();

    const style = preview.get('[data-slot="0"]').attributes("style");
    expect(setGlobalBrightnessMock).toHaveBeenLastCalledWith(50);
    expect(updateEffectMock).toHaveBeenLastCalledWith({
      slot: 0,
      brightness: 50,
      speedPercent: 300,
      direction: "counterClockwise",
      tailPercent: 100,
    });
    expect(style).toContain("--ring-opacity: 0.250");
    expect(style).toContain("--ring-motion-duration: 600ms");
    expect(style).toContain("--ring-motion-direction: reverse");
    expect(style).toContain("--ring-tail-start: 0%");
  });

  it("高频修改串行合并为最新意图，旧返回不抖动草稿，失败保留快照并显示 Store 错误", async () => {
    const first = deferred<HaloSnapshot | null>();
    const second = deferred<HaloSnapshot | null>();
    let call = 0;
    updateEffectMock.mockImplementation(() => {
      call += 1;
      const pending = call === 1 ? first : second;
      return pending.promise.then((result) => {
        if (result) {
          mountedState.snapshot = result;
        }
        return result;
      });
    });

    const wrapper = mount(App);
    await wrapper.findComponent(HaloPreview).vm.$emit("select", 0);
    await nextTick();
    const editor = wrapper.findComponent(EffectEditor);
    const speed = editor.get("[data-effect-speed-number]");

    await speed.setValue("150");
    await speed.setValue("200");
    await speed.setValue("250");
    expect(updateEffectMock).toHaveBeenCalledTimes(1);

    first.resolve(
      replaceEffect(snapshot(), {
        slot: 0,
        ...DEFAULT_EFFECT,
        speedPercent: 150,
      }),
    );
    await flushPromises();
    expect(updateEffectMock).toHaveBeenCalledTimes(2);
    expect(updateEffectMock).toHaveBeenLastCalledWith({
      slot: 0,
      ...DEFAULT_EFFECT,
      speedPercent: 250,
    });
    expect(
      (editor.get("[data-effect-speed-number]").element as HTMLInputElement)
        .value,
    ).toBe("250");

    second.resolve(
      replaceEffect(snapshot(), {
        slot: 0,
        ...DEFAULT_EFFECT,
        speedPercent: 250,
      }),
    );
    await flushPromises();
    expect(
      wrapper
        .findComponent(HaloPreview)
        .get('[data-slot="0"]')
        .attributes("style"),
    ).toContain("--ring-motion-duration: 720ms");

    const retained = mountedState.snapshot;
    const failure = deferred<HaloSnapshot | null>();
    updateEffectMock
      .mockImplementationOnce(() =>
        failure.promise.then((result) => {
          mountedState.error = {
            operation: "updateEffect",
            code: "engineRejected",
            message: "updateEffect 操作失败",
          };
          return result;
        }),
      )
      .mockImplementationOnce(async (input: UpdateEffectInput) => {
        mountedState.error = null;
        mountedState.snapshot = replaceEffect(mountedState.snapshot!, input);
        return mountedState.snapshot;
      });
    await editor.get("[data-effect-speed-number]").setValue("260");
    await editor.get("[data-effect-speed-number]").setValue("270");
    failure.resolve(null);
    await flushPromises();

    expect(mountedState.snapshot).not.toBe(retained);
    expect(mountedState.snapshot?.slots[0].effect.speedPercent).toBe(270);
    expect(wrapper.get("[data-app-error]").text()).toContain(
      "updateEffect 操作失败",
    );
  });
});
