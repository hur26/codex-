import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { RingSlot, TaskStatus } from "../types/halo";
import CentralDisplay from "./CentralDisplay.vue";
import componentSource from "./CentralDisplay.vue?raw";

function createSlot(index: number, status: TaskStatus = "idle"): RingSlot {
  return {
    index,
    taskKey: status === "idle" ? null : `private-task-fingerprint-${index}`,
    status,
    source: status === "idle" ? null : index === 3 ? "simulator" : "hook",
    confidence:
      status === "idle" ? null : index === 1 ? "provisional" : "observed",
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

const slots = [
  createSlot(0, "running"),
  createSlot(1, "waiting"),
  createSlot(2, "roundCompleted"),
  createSlot(3, "failed"),
];

describe("CentralDisplay", () => {
  it.each(["ambient", "overview", "detail"] as const)(
    "支持 %s 显示模式",
    (mode) => {
      const wrapper = mount(CentralDisplay, {
        props: { mode, slots, selectedSlot: 1 },
      });

      expect(wrapper.get("[data-central-display]").attributes("data-mode")).toBe(
        mode,
      );
    },
  );

  it("ambient 显示原创 Halo 节点图形且不嵌入品牌图片", () => {
    const wrapper = mount(CentralDisplay, {
      props: { mode: "ambient", slots, selectedSlot: null },
    });

    expect(wrapper.find("[data-halo-glyph]").exists()).toBe(true);
    expect(wrapper.findAll("[data-halo-node]")).toHaveLength(4);
    expect(wrapper.find("img").exists()).toBe(false);
    expect(componentSource).not.toMatch(/<image\b/i);
  });

  it("overview 永远按顺序显示四圈匿名名与状态且不泄露 taskKey", () => {
    const wrapper = mount(CentralDisplay, {
      props: { mode: "overview", slots: slots.slice(0, 2), selectedSlot: null },
    });

    const rows = wrapper.findAll("[data-overview-slot]");
    expect(rows).toHaveLength(4);
    expect(rows.map((row) => row.attributes("data-overview-slot"))).toEqual([
      "0",
      "1",
      "2",
      "3",
    ]);
    expect(wrapper.text()).toContain("HALO 01");
    expect(wrapper.text()).toContain("执行中");
    expect(wrapper.text()).toContain("等待确认");
    expect(wrapper.text()).toContain("空闲");
    expect(wrapper.text()).not.toContain("private-task-fingerprint");
  });

  it("detail 显示选中圈号、来源、可信度和本轮状态", () => {
    const wrapper = mount(CentralDisplay, {
      props: { mode: "detail", slots, selectedSlot: 1 },
    });

    const detail = wrapper.get("[data-detail-slot]");
    expect(detail.attributes("data-detail-slot")).toBe("1");
    expect(detail.text()).toContain("第 2 圈");
    expect(detail.text()).toContain("Hook");
    expect(detail.text()).toContain("候选信号");
    expect(detail.text()).toContain("等待确认");
    expect(detail.text()).not.toContain("private-task-fingerprint");
  });

  it("detail 无选中圈时保持模式并呈现明确空状态", () => {
    const wrapper = mount(CentralDisplay, {
      props: { mode: "detail", slots, selectedSlot: null },
    });

    expect(wrapper.get("[data-central-display]").attributes("data-mode")).toBe(
      "detail",
    );
    expect(wrapper.text()).toContain("未选择圆环");
  });

  it("使用 OLED 工业仪器 token 并照顾 reduced motion", () => {
    expect(componentSource).toContain("var(--halo-canvas)");
    expect(componentSource).toContain("var(--halo-focus)");
    expect(componentSource).toContain("var(--halo-running)");
    expect(componentSource).toContain(
      "@media (prefers-reduced-motion: reduce)",
    );
  });
});
