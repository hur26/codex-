import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import CrownControl from "./CrownControl.vue";
import componentSource from "./CrownControl.vue?raw";

afterEach(() => {
  vi.useRealTimers();
});

function mountCrown(
  mode: "ambient" | "overview" | "detail" = "ambient",
  selectedSlot: number | null = null,
) {
  return mount(CrownControl, {
    props: {
      mode,
      selectedSlot,
      longPressMs: 500,
    },
  });
}

describe("CrownControl", () => {
  it.each([
    ["ambient", "overview"],
    ["overview", "detail"],
    ["detail", "ambient"],
  ] as const)("短按按清晰循环从 %s 切换到 %s", async (mode, nextMode) => {
    vi.useFakeTimers();
    const wrapper = mountCrown(mode, null);
    const crown = wrapper.get("[data-crown-press]");

    await crown.trigger("pointerdown");
    vi.advanceTimersByTime(200);
    await crown.trigger("pointerup");

    expect(wrapper.emitted("update:mode")).toEqual([[nextMode]]);
  });

  it("detail 没有选中圈时短按仍按契约返回 ambient", async () => {
    vi.useFakeTimers();
    const wrapper = mountCrown("detail", null);
    const crown = wrapper.get("[data-crown-press]");

    await crown.trigger("pointerdown");
    await crown.trigger("pointerup");

    expect(wrapper.emitted("update:mode")).toEqual([["ambient"]]);
  });

  it("左右旋转选择 0..3 并在边界环绕", async () => {
    const fromFirst = mountCrown("overview", 0);
    await fromFirst.get("[data-crown-left]").trigger("click");
    expect(fromFirst.emitted("select")).toEqual([[3]]);

    const fromLast = mountCrown("overview", 3);
    await fromLast.get("[data-crown-right]").trigger("click");
    expect(fromLast.emitted("select")).toEqual([[0]]);

    const fromEmpty = mountCrown("overview", null);
    await fromEmpty.get("[data-crown-right]").trigger("click");
    expect(fromEmpty.emitted("select")).toEqual([[0]]);
  });

  it("长按只触发一次 ambient 且松开不再触发短按", async () => {
    vi.useFakeTimers();
    const wrapper = mountCrown("overview", 2);
    const crown = wrapper.get("[data-crown-press]");

    await crown.trigger("pointerdown");
    vi.advanceTimersByTime(500);
    await crown.trigger("pointerup");

    expect(wrapper.emitted("update:mode")).toEqual([["ambient"]]);
  });

  it("按下与松开时同步暴露表冠按压状态", async () => {
    vi.useFakeTimers();
    const wrapper = mountCrown("ambient", 0);
    const crown = wrapper.get("[data-crown-press]");

    expect(crown.attributes("aria-pressed")).toBe("false");
    await crown.trigger("pointerdown");
    expect(crown.attributes("aria-pressed")).toBe("true");
    await crown.trigger("pointerup");
    expect(crown.attributes("aria-pressed")).toBe("false");
  });

  it("取消或离开表冠时清理长按计时且不触发短按", async () => {
    vi.useFakeTimers();

    for (const cancelEvent of ["pointercancel", "pointerleave"]) {
      const wrapper = mountCrown("overview", 2);
      const crown = wrapper.get("[data-crown-press]");
      await crown.trigger("pointerdown");
      await crown.trigger(cancelEvent);
      vi.advanceTimersByTime(1000);

      expect(wrapper.emitted("update:mode")).toBeUndefined();
      wrapper.unmount();
    }
  });

  it("Enter 键支持短按，长按计时可由 fake timer 稳定控制", async () => {
    vi.useFakeTimers();
    const wrapper = mountCrown("ambient", 0);
    const crown = wrapper.get("[data-crown-press]");

    await crown.trigger("keydown", { key: "Enter" });
    vi.advanceTimersByTime(200);
    await crown.trigger("keyup", { key: "Enter" });

    expect(wrapper.emitted("update:mode")).toEqual([["overview"]]);
  });

  it("固定在设备约四点钟方向并提供可访问名称与触控约束", () => {
    const wrapper = mountCrown();

    expect(wrapper.get("[data-crown-press]").attributes("aria-label")).toContain(
      "表冠",
    );
    expect(wrapper.get("[data-crown-left]").attributes("aria-label")).toContain(
      "上一圈",
    );
    expect(wrapper.get("[data-crown-right]").attributes("aria-label")).toContain(
      "下一圈",
    );
    expect(componentSource).toMatch(
      /\.crown-control\s*\{[\s\S]*position:\s*absolute/,
    );
    expect(componentSource).toMatch(
      /\.crown-control\s*\{[\s\S]*bottom:\s*[^;]+/,
    );
    expect(componentSource).toMatch(
      /\.crown-control\s*\{[\s\S]*(?:right|inset-inline-end):\s*[^;]+/,
    );
    expect(componentSource).toContain("touch-action: none");
  });
});
