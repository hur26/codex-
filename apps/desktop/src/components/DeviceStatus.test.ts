import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { DeviceStatus } from "../types/halo";
import DeviceStatusView from "./DeviceStatus.vue";

const BASE_STATUS: DeviceStatus = {
  revision: 1,
  state: "virtual",
  transport: "simulator",
  message: null,
  firmwareVersion: "0.1.0",
  retryCount: 0,
};

describe("DeviceStatus", () => {
  it.each([
    ["virtual", "VIRTUAL"],
    ["connecting", "CONNECTING"],
    ["online", "ONLINE"],
    ["incompatible", "INCOMPATIBLE"],
    ["error", "ERROR"],
  ] as const)("renders a recognizable %s label", (state, label) => {
    const wrapper = mount(DeviceStatusView, {
      props: { status: { ...BASE_STATUS, state } },
    });

    expect(wrapper.get("[data-device-status]").text()).toContain(label);
    expect(wrapper.get("[data-device-status]").attributes("data-device-state")).toBe(
      state,
    );
  });

  it("uses an explicit error treatment for incompatible firmware", () => {
    const wrapper = mount(DeviceStatusView, {
      props: { status: { ...BASE_STATUS, state: "incompatible" } },
    });

    expect(wrapper.get("[data-device-status]").classes()).toContain(
      "device-status-incompatible",
    );
  });

  it("does not render task identity or USB serial data", () => {
    const wrapper = mount(DeviceStatusView, {
      props: {
        status: {
          ...BASE_STATUS,
          message: "taskKey serialNumber USB-SECRET-1234",
        },
      },
    });

    expect(wrapper.html()).not.toContain("taskKey");
    expect(wrapper.html()).not.toContain("serialNumber");
    expect(wrapper.html()).not.toContain("USB-SECRET-1234");
  });
});
