import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import type {
  Confidence,
  RingSlot,
  SignalSource,
  TaskStatus,
} from "../types/halo";
import HaloPreview from "./HaloPreview.vue";
import componentSource from "./HaloPreview.vue?raw";

function createSlot(
  index: number,
  status: TaskStatus = "idle",
  source: SignalSource | null = null,
  confidence: Confidence | null = null,
): RingSlot {
  return {
    index,
    taskKey: status === "idle" ? null : `${index}`.padStart(16, "0"),
    status,
    source,
    confidence,
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

describe("HaloPreview", () => {
  it("即使快照缺槽也始终按内到外顺序渲染四圈", () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [
          createSlot(3, "unknown", "hook", "observed"),
          createSlot(1, "running", "hook", "observed"),
        ],
        selectedSlot: null,
      },
    });

    const rings = wrapper.findAll("[data-slot]");
    expect(rings).toHaveLength(4);
    expect(rings.map((ring) => ring.attributes("data-slot"))).toEqual([
      "0",
      "1",
      "2",
      "3",
    ]);
    expect(rings.map((ring) => ring.attributes("data-status"))).toEqual([
      "idle",
      "running",
      "idle",
      "unknown",
    ]);
  });

  it("每圈可访问地暴露状态、来源、可置信度与选中态", () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [
          createSlot(0, "waiting", "hook", "provisional"),
          createSlot(1),
          createSlot(2),
          createSlot(3),
        ],
        selectedSlot: 0,
      },
    });

    const innerRing = wrapper.get('[data-slot="0"]');
    expect(innerRing.attributes()).toMatchObject({
      "data-status": "waiting",
      "data-source": "hook",
      "data-confidence": "provisional",
      "data-selected": "true",
      "aria-pressed": "true",
    });
    expect(innerRing.attributes("aria-label")).toContain("第 1 圈（最内圈）");
    expect(innerRing.attributes("aria-label")).toContain("等待确认");
    expect(innerRing.attributes("aria-label")).toContain("候选信号");

    const idleRing = wrapper.get('[data-slot="2"]');
    expect(idleRing.attributes()).toMatchObject({
      "data-status": "idle",
      "data-source": "none",
      "data-confidence": "none",
      "data-selected": "false",
      "aria-pressed": "false",
    });
  });

  it.each([
    ["running", "hook", "observed", "status-running"],
    ["waiting", "hook", "provisional", "status-waiting"],
    ["roundCompleted", "hook", "observed", "status-round-completed"],
    ["failed", "simulator", "simulated", "status-failed"],
    ["queued", "simulator", "simulated", "status-queued"],
    ["unknown", "hook", "observed", "status-unknown"],
    ["idle", null, null, "status-idle"],
  ] as const)(
    "%s 状态应用对应且可检验的光效语义",
    (status, source, confidence, className) => {
      const wrapper = mount(HaloPreview, {
        props: {
          slots: [createSlot(0, status, source, confidence)],
          selectedSlot: null,
        },
      });

      expect(wrapper.get('[data-slot="0"]').classes()).toContain(className);
    },
  );

  it("只有模拟且 simulated 的 failed 才显示红色故障脉冲", () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [createSlot(0, "failed", "hook", "observed")],
        selectedSlot: null,
      },
    });

    const ring = wrapper.get('[data-slot="0"]');
    expect(ring.attributes("data-status")).toBe("failed");
    expect(ring.classes()).not.toContain("status-failed");
    expect(ring.classes()).toContain("status-unknown");
  });

  it("provisional 状态显示克制的候选标记", () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [createSlot(0, "waiting", "hook", "provisional")],
        selectedSlot: null,
      },
    });

    const marker = wrapper.get('[data-slot="0"] .confidence-marker');
    expect(marker.text()).toBe("PROVISIONAL");
    expect(marker.attributes("aria-hidden")).toBe("true");
  });

  it("点击圆环触发 select(slot)", async () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [createSlot(0), createSlot(1), createSlot(2), createSlot(3)],
        selectedSlot: null,
      },
    });

    await wrapper.get('[data-slot="2"]').trigger("click");

    expect(wrapper.emitted("select")).toEqual([[2]]);
  });

  it("只有已绑定圆环可拖动，拖拽载荷仅写安全 marker", async () => {
    const occupied = createSlot(0, "running", "hook", "observed");
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [occupied, createSlot(1)],
        selectedSlot: null,
        dragActive: true,
      },
    });
    const dataTransfer = {
      effectAllowed: "none",
      setData: vi.fn(),
    };

    expect(wrapper.get('[data-slot="0"]').attributes("draggable")).toBe("true");
    expect(wrapper.get('[data-slot="1"]').attributes("draggable")).toBe("false");
    await wrapper.get('[data-slot="0"]').trigger("dragstart", {
      dataTransfer,
    });

    expect(wrapper.emitted("dragstart")).toEqual([
      [{ kind: "slot", slot: 0, taskKey: occupied.taskKey }],
    ]);
    expect(dataTransfer.setData).toHaveBeenCalledWith(
      "application/x-codex-halo-drag",
      "slot",
    );
    expect(dataTransfer.setData).not.toHaveBeenCalledWith(
      "text/plain",
      expect.anything(),
    );
    expect(dataTransfer.setData.mock.calls.flat().join(" ")).not.toContain(
      occupied.taskKey,
    );
  });

  it("拖入圈位提供可访问反馈并在释放时只发出目标圈位", async () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [createSlot(0), createSlot(1)],
        selectedSlot: null,
        dragActive: true,
        dragKind: "task",
      },
    });
    const ring = wrapper.get('[data-slot="1"]');
    const dataTransfer = {
      dropEffect: "none",
      getData: vi.fn(() => "task"),
    };

    await ring.trigger("dragenter", { dataTransfer });
    expect(ring.attributes("data-drop-active")).toBe("true");
    expect(ring.attributes("aria-label")).toContain("释放以完成绑定");
    expect(dataTransfer.dropEffect).toBe("move");

    await ring.trigger("drop", { dataTransfer });
    expect(wrapper.emitted("drop")).toEqual([[1]]);
    expect(ring.attributes("data-drop-active")).toBe("false");
  });

  it("外部或 marker 非法的拖拽不显示放置反馈也不发出 drop", async () => {
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [createSlot(0)],
        selectedSlot: null,
        dragActive: true,
        dragKind: "task",
      },
    });
    const ring = wrapper.get('[data-slot="0"]');
    const dataTransfer = {
      dropEffect: "none",
      getData: vi.fn(() => "external-file"),
    };

    await ring.trigger("dragenter", { dataTransfer });
    expect(ring.attributes("data-drop-active")).toBe("false");
    expect(ring.attributes("aria-label")).not.toContain("释放以完成绑定");
    expect(dataTransfer.dropEffect).toBe("none");

    await ring.trigger("drop", { dataTransfer });
    expect(wrapper.emitted("drop")).toBeUndefined();
  });

  it("dragend 与父级取消信号都会清理放置反馈和 aria 提示", async () => {
    const occupied = createSlot(0, "running", "hook", "observed");
    const wrapper = mount(HaloPreview, {
      props: {
        slots: [occupied, createSlot(1)],
        selectedSlot: null,
        dragActive: true,
        dragKind: "slot",
      },
    });
    const source = wrapper.get('[data-slot="0"]');
    const target = wrapper.get('[data-slot="1"]');
    const dataTransfer = {
      effectAllowed: "none",
      dropEffect: "none",
      setData: vi.fn(),
      getData: vi.fn(() => "slot"),
    };

    await source.trigger("dragstart", { dataTransfer });
    await target.trigger("dragenter", { dataTransfer });
    expect(target.attributes("data-drop-active")).toBe("true");

    await source.trigger("dragend", { dataTransfer });
    expect(wrapper.emitted("dragend")).toEqual([[]]);
    expect(target.attributes("data-drop-active")).toBe("false");

    await target.trigger("dragenter", { dataTransfer });
    expect(target.attributes("data-drop-active")).toBe("true");
    await wrapper.setProps({ dragActive: false });
    expect(target.attributes("data-drop-active")).toBe("false");
    expect(target.attributes("aria-label")).not.toContain("释放以完成绑定");
  });

  it("按 25% 到 300% 的完整灯效速度范围换算追逐周期", () => {
    const fastest = createSlot(0, "running", "hook", "observed");
    fastest.effect.speedPercent = 300;
    const slowest = createSlot(1, "running", "hook", "observed");
    slowest.effect.speedPercent = 25;

    const wrapper = mount(HaloPreview, {
      props: {
        slots: [fastest, slowest],
        selectedSlot: null,
      },
    });

    expect(wrapper.get('[data-slot="0"]').attributes("style")).toContain(
      "--ring-motion-duration: 600ms",
    );
    expect(wrapper.get('[data-slot="1"]').attributes("style")).toContain(
      "--ring-motion-duration: 7200ms",
    );
  });

  it("使用变量驱动的矢量光环，并在减少动态时停止动画但保留状态色", () => {
    expect(componentSource).toMatch(/(?:mask|conic-gradient)/);
    expect(componentSource).toContain("var(--halo-");
    expect(componentSource).toContain(
      "@media (prefers-reduced-motion: reduce)",
    );
    expect(componentSource).toMatch(
      /prefers-reduced-motion:[\s\S]*animation:\s*none/,
    );
    expect(componentSource).toContain("var(--halo-running)");
    expect(componentSource).toContain("var(--halo-ring-gap)");
    expect(componentSource).toContain("var(--halo-motion-running)");
  });

  it("小屏仍保留圈号与 provisional 标记可见", () => {
    const compactMediaRule = componentSource.match(
      /@media \(max-width: 520px\) \{([\s\S]*?)\n\}/,
    )?.[1];

    expect(compactMediaRule).toContain(".slot-index");
    expect(compactMediaRule).toContain(".confidence-marker");
    expect(compactMediaRule).not.toMatch(
      /(?:display:\s*none|visibility:\s*hidden|opacity:\s*0)/,
    );
  });
});
