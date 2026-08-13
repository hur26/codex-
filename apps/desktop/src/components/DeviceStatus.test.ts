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

const SAFE_DIAGNOSTICS = [
  "Device endpoint was not found",
  "Device discovery failed",
  "Device connection failed",
  "Protocol major is incompatible",
  "Device read failed",
  "Device capabilities are incompatible",
  "Device rejected state update",
  "Device snapshot was invalid",
  "Device update was invalid",
  "Device frame could not be encoded",
  "Device write failed",
  "Device response timed out",
  "Device retry failed",
  "Device heartbeat failed",
  "Device worker could not start",
  "Virtual device state is unavailable",
  "Device watchdog entered disconnected state",
  "Device reported a CRC error",
  "Device reported an invalid payload",
  "Device local power or brightness limit is active",
] as const;

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

  it("shows a known safe diagnostic message visibly and accessibly", () => {
    const wrapper = mount(DeviceStatusView, {
      props: {
        status: {
          ...BASE_STATUS,
          state: "incompatible",
          transport: "serial",
          message: "Protocol major is incompatible",
        },
      },
    });

    expect(wrapper.get("[data-device-message]").text()).toBe(
      "Protocol major is incompatible",
    );
    expect(wrapper.get("[data-device-status]").attributes("aria-label")).toContain(
      "Protocol major is incompatible",
    );
    expect(wrapper.get("[data-device-status]").attributes("title")).toBe(
      "Protocol major is incompatible",
    );
  });

  it.each(SAFE_DIAGNOSTICS)(
    "allows the fixed Rust diagnostic: %s",
    (message) => {
      const wrapper = mount(DeviceStatusView, {
        props: {
          status: { ...BASE_STATUS, state: "error", message },
        },
      });

      expect(wrapper.get("[data-device-message]").text()).toBe(message);
    },
  );

  it.each([
    ["bare task key", "0123456789abcdef"],
    ["prompt body", "请重写用户的支付提示词正文"],
    ["Windows port", "COM77"],
    ["Unix port", "/dev/ttyUSB0"],
    ["arbitrary USB serial", "SN9X7K2Q4M8P"],
    ["raw frame hex", "4348010b0700a1b2c3d4e5f6c7d8"],
  ])("hides unknown %s diagnostics", (_kind, message) => {
    const wrapper = mount(DeviceStatusView, {
      props: {
        status: {
          ...BASE_STATUS,
          state: "error",
          transport: "serial",
          message,
        },
      },
    });

    expect(wrapper.html()).not.toContain(message);
    expect(wrapper.get("[data-device-message]").text()).toBe(
      "设备诊断信息已隐藏",
    );
    expect(wrapper.get("[data-device-status]").attributes("title")).toBe(
      "设备诊断信息已隐藏",
    );
    expect(wrapper.get("[data-device-status]").attributes("aria-label")).not.toContain(
      message,
    );
  });

  it("does not render message chrome or accessible noise for an empty message", () => {
    const wrapper = mount(DeviceStatusView, {
      props: { status: { ...BASE_STATUS, message: "   " } },
    });

    expect(wrapper.find("[data-device-message]").exists()).toBe(false);
    expect(wrapper.get("[data-device-status]").attributes("title")).toBeUndefined();
    expect(wrapper.get("[data-device-status]").attributes("aria-label")).toBe(
      "Device VIRTUAL, simulator",
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
