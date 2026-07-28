// @vitest-environment jsdom

import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App.vue";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({
      deviceMode: "virtual",
      globalBrightness: 100,
      slots: Array.from({ length: 4 }, (_, index) => ({
        index,
        taskKey: null,
      })),
      tasks: [],
      queue: [],
    });
  });

  it("loads and presents the virtual snapshot instead of calling the removed greet command", async () => {
    const wrapper = mount(App);
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("get_snapshot");
    expect(wrapper.text()).toContain("VIRTUAL DEVICE");
    expect(wrapper.text()).toContain("4 个光环已就绪");
  });
});
