import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { RingSlot, TaskRecord } from "../types/halo";
import ActivityStrip from "./ActivityStrip.vue";

const tasks: TaskRecord[] = [
  {
    taskKey: "hidden-running-key",
    status: "running",
    source: "hook",
    confidence: "observed",
    lastActiveAtMs: 90_000,
  },
  {
    taskKey: "hidden-queued-key",
    status: "queued",
    source: "simulator",
    confidence: "simulated",
    lastActiveAtMs: 80_000,
  },
];

const slots: RingSlot[] = [
  {
    index: 2,
    taskKey: tasks[0].taskKey,
    status: "running",
    source: "hook",
    confidence: "observed",
    bindingMode: "auto",
    locked: false,
    effect: {
      brightness: 80,
      speedPercent: 100,
      direction: "clockwise",
      tailPercent: 35,
    },
  },
];

describe("ActivityStrip", () => {
  it("按最近活动显示最小匿名轨迹并区分圈位与队列", () => {
    const wrapper = mount(ActivityStrip, {
      props: { tasks, slots, queue: [tasks[1]], nowMs: 100_000 },
    });

    const events = wrapper.findAll("[data-activity-event]");
    expect(events).toHaveLength(2);
    expect(events[0].text()).toContain("R03");
    expect(events[0].text()).toContain("正在执行");
    expect(events[0].text()).toContain("10 秒前");
    expect(events[1].text()).toContain("Q01");
    expect(events[1].text()).toContain("排队等待");
    expect(wrapper.html()).not.toContain("hidden-running-key");
    expect(wrapper.html()).not.toContain("hidden-queued-key");
  });

  it("没有活动时仍保留明确的空轨道", () => {
    const wrapper = mount(ActivityStrip, {
      props: { tasks: [], slots: [], queue: [], nowMs: 100_000 },
    });

    expect(wrapper.get("[data-activity-strip]").text()).toContain(
      "等待状态信号",
    );
  });
});
