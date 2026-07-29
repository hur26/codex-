import { mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import type { RingSlot, TaskRecord, TaskStatus } from "../types/halo";
import TaskRail from "./TaskRail.vue";
import componentSource from "./TaskRail.vue?raw";

const baseStyles = readFileSync(
  resolve(process.cwd(), "src/styles/base.css"),
  "utf8",
);

const PRIVATE_KEYS = {
  running: "b1c9e134b8f245a0",
  waiting: "9a720f116f9b47de",
  completed: "81eaae341dc24f13",
  queued: "fifth-task-secret",
} as const;

function task(
  taskKey: string,
  status: TaskStatus,
  lastActiveAtMs: number,
): TaskRecord {
  return {
    taskKey,
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

function slot(
  index: number,
  record: TaskRecord | null,
  locked = false,
): RingSlot {
  return {
    index,
    taskKey: record?.taskKey ?? null,
    status: record?.status ?? "idle",
    source: record?.source ?? null,
    confidence: record?.confidence ?? null,
    bindingMode: "auto",
    locked,
    effect: {
      brightness: 80,
      speedPercent: 100,
      direction: "clockwise",
      tailPercent: 35,
    },
  };
}

const nowMs = 2_000_000;
const running = task(PRIVATE_KEYS.running, "running", nowMs - 4_000);
const waiting = task(PRIVATE_KEYS.waiting, "waiting", nowMs - 75_000);
const completed = task(
  PRIVATE_KEYS.completed,
  "roundCompleted",
  nowMs - 3_600_000,
);
const queued = task(PRIVATE_KEYS.queued, "queued", nowMs - 2_000);
const slots = [
  slot(0, running),
  slot(1, waiting, true),
  slot(2, completed),
  slot(3, null),
];

describe("TaskRail", () => {
  it("以圈号匿名显示状态、来源、可信度和最近活动，绝不泄露 taskKey", () => {
    const wrapper = mount(TaskRail, {
      props: {
        slots,
        tasks: [running, waiting, completed, queued],
        queue: [queued],
        nowMs,
        selectedSlot: 1,
      },
    });

    const rows = wrapper.findAll("[data-task-slot]");
    expect(rows).toHaveLength(4);
    expect(rows.map((row) => row.attributes("data-task-slot"))).toEqual([
      "0",
      "1",
      "2",
      "3",
    ]);
    expect(rows[0].text()).toContain("RING 01");
    expect(rows[0].text()).toContain("正在执行");
    expect(rows[0].text()).toContain("Hook");
    expect(rows[0].text()).toContain("已观测");
    expect(rows[0].text()).toContain("4 秒前");
    expect(rows[1].text()).toContain("等待确认");
    expect(rows[1].text()).toContain("候选信号");
    expect(rows[1].text()).toContain("1 分钟前");
    expect(rows[1].attributes("aria-current")).toBe("true");
    expect(rows[3].text()).toContain("未绑定");

    const markup = wrapper.html();
    for (const key of Object.values(PRIVATE_KEYS)) {
      expect(markup).not.toContain(key);
    }
    expect(markup).not.toContain("taskKey");
  });

  it("将第五个任务明确显示为等待队列且保持匿名", () => {
    const wrapper = mount(TaskRail, {
      props: {
        slots,
        tasks: [running, waiting, completed, queued],
        queue: [queued],
        nowMs,
        selectedSlot: null,
      },
    });

    const queueRows = wrapper.findAll("[data-queue-task]");
    expect(queueRows).toHaveLength(1);
    expect(queueRows[0].text()).toContain("QUEUE 01");
    expect(queueRows[0].text()).toContain("排队等待");
    expect(queueRows[0].text()).toContain("2 秒前");
    expect(wrapper.text()).toContain("1 WAITING");
    expect(wrapper.html()).not.toContain(PRIVATE_KEYS.queued);
  });

  it("轨道只读但可选择已绑定圈，不暴露 Task 9 的拖拽或绑定控件", async () => {
    const wrapper = mount(TaskRail, {
      props: {
        slots,
        tasks: [running, waiting, completed],
        queue: [],
        nowMs,
        selectedSlot: null,
      },
    });

    await wrapper.get('[data-task-slot="2"]').trigger("click");

    expect(wrapper.emitted("select")).toEqual([[2]]);
    expect(wrapper.find("[draggable=true]").exists()).toBe(false);
    expect(wrapper.find("[data-bind-control]").exists()).toBe(false);
    expect(wrapper.find("[data-lock-control]").exists()).toBe(false);
  });

  it("提供可访问的抽屉开关并让折叠内容持续存在于 DOM", async () => {
    const wrapper = mount(TaskRail, {
      props: {
        slots,
        tasks: [running],
        queue: [queued],
        nowMs,
        selectedSlot: null,
      },
    });
    const toggle = wrapper.get("[data-rail-toggle]");

    expect(toggle.attributes("aria-expanded")).toBe("false");
    expect(wrapper.get("[data-rail-content]").element).toBeTruthy();

    await toggle.trigger("click");

    expect(toggle.attributes("aria-expanded")).toBe("true");
    expect(wrapper.get("[data-task-slot]").element).toBeTruthy();
    expect(wrapper.get("[data-queue-task]").element).toBeTruthy();
  });

  it("以 1180px 断点切换抽屉并锁定页面不裁切的 CSS 契约", () => {
    expect(componentSource).toContain("data-rail-toggle");
    expect(baseStyles).toContain("@media (max-width: 1179px)");
    expect(baseStyles).toMatch(
      /@media \(max-width: 1179px\)[\s\S]*\.task-rail/,
    );
    expect(baseStyles).toMatch(/\.app-shell\s*\{[\s\S]*min-height:\s*0/);
    expect(baseStyles).toMatch(/\.app-main\s*\{[\s\S]*min-height:\s*0/);
    expect(baseStyles).toMatch(/\.device-stage\s*\{[\s\S]*min-height:\s*0/);
  });

  it("仅在紧凑断点隐藏折叠内容，并在展开后恢复键盘与辅助技术可见性", () => {
    const compactMediaRule = baseStyles.match(
      /@media \(max-width: 1179px\) \{([\s\S]*?)(?=\n@media \(max-width: 700px\))/,
    )?.[1];

    expect(compactMediaRule).toMatch(
      /\.task-rail:not\(\.rail-expanded\) \.task-rail-content\s*\{[\s\S]*visibility:\s*hidden/,
    );
    expect(compactMediaRule).toMatch(
      /\.task-rail\.rail-expanded \.task-rail-content\s*\{[\s\S]*visibility:\s*visible/,
    );
    expect(
      baseStyles.slice(0, baseStyles.indexOf("@media (max-width: 1179px)")),
    ).not.toMatch(/\.task-rail-content\s*\{[\s\S]*visibility:\s*hidden/);
  });

  it("任务行与抽屉按钮支持强制色焦点，并在 reduced motion 下关闭过渡", () => {
    expect(componentSource).toContain("@media (forced-colors: active)");
    expect(componentSource).toMatch(
      /forced-colors:[\s\S]*\.task-row[\s\S]*\.rail-toggle[\s\S]*outline:\s*2px solid Highlight/,
    );
    expect(componentSource).toContain(
      "@media (prefers-reduced-motion: reduce)",
    );
    expect(componentSource).toMatch(
      /prefers-reduced-motion:[\s\S]*\.task-row[\s\S]*transition:\s*none/,
    );
    expect(baseStyles).toMatch(
      /prefers-reduced-motion:[\s\S]*\.task-rail[\s\S]*transition:\s*none/,
    );
  });

  it("等待队列使用稳定任务主键作为 VDOM key，但不输出到 HTML", () => {
    expect(componentSource).toContain(':key="queuedTask.taskKey"');
    expect(componentSource).not.toContain(':key="index"');
  });
});
