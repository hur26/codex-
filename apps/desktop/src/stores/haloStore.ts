import { computed, reactive } from "vue";
import { haloBridge, type HaloBridge } from "../services/haloBridge";
import type {
  AdapterStatus,
  DeviceStatus,
  DisplayMode,
  HaloSnapshot,
  ManualBindInput,
  SimulateSignalInput,
  UpdateEffectInput,
} from "../types/halo";

export type HaloStoreOperation =
  | "load"
  | "subscribe"
  | "adapterStatus"
  | "deviceStatus"
  | "simulateSignal"
  | "manualBind"
  | "toggleLock"
  | "swapSlots"
  | "updateEffect"
  | "setGlobalBrightness"
  | "setPresentation";

export interface HaloStoreError {
  operation: HaloStoreOperation;
  code: string;
  message: string;
}

export interface HaloStoreState {
  snapshot: HaloSnapshot | null;
  adapterStatus: AdapterStatus;
  deviceStatus: DeviceStatus;
  loading: boolean;
  error: HaloStoreError | null;
}

const INITIAL_ADAPTER_STATUS: AdapterStatus = {
  revision: 0,
  state: "offline",
  mode: "hook",
  message: "适配器状态尚未读取",
  acceptedEvents: 0,
  ignoredEvents: 0,
  rejectedEvents: 0,
};

const INITIAL_DEVICE_STATUS: DeviceStatus = {
  revision: 0,
  state: "connecting",
  transport: "simulator",
  message: null,
  firmwareVersion: null,
  retryCount: 0,
};

function errorCode(error: unknown): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
  ) {
    return error.code;
  }
  return "bridgeFailure";
}

function stableError(
  operation: HaloStoreOperation,
  error: unknown,
): HaloStoreError {
  return {
    operation,
    code: errorCode(error),
    message: `${operation} 操作失败`,
  };
}

function beginSubscription(
  subscribe: () => Promise<() => void>,
): Promise<() => void> {
  return Promise.resolve().then(subscribe);
}

function cleanupSubscriptions(
  ...unlistens: Array<(() => void) | null>
): void {
  for (const unlisten of unlistens) {
    try {
      unlisten?.();
    } catch {
      // Continue so one faulty listener cannot leak the remaining subscriptions.
    }
  }
}

export function createHaloStore(bridge: HaloBridge = haloBridge) {
  const state = reactive<HaloStoreState>({
    snapshot: null,
    adapterStatus: { ...INITIAL_ADAPTER_STATUS },
    deviceStatus: { ...INITIAL_DEVICE_STATUS },
    loading: false,
    error: null,
  });
  const occupiedSlotCount = computed(
    () =>
      state.snapshot?.slots.filter((slot) => slot.taskKey !== null).length ?? 0,
  );

  let snapshotUnlisten: (() => void) | null = null;
  let adapterStatusUnlisten: (() => void) | null = null;
  let deviceStatusUnlisten: (() => void) | null = null;
  let pendingSubscriptionAttempt: { cancel: () => void } | null = null;
  let desiredRunning = false;
  let lifecycleGeneration = 0;
  let lifecyclePromise: Promise<boolean> = Promise.resolve(false);
  let acceptedSnapshotCount = 0;
  let acceptedAdapterStatusCount = 0;
  let acceptedDeviceStatusCount = 0;
  let loadPromise: Promise<void> | null = null;
  let commandGeneration = 0;

  function recordError(operation: HaloStoreOperation, error: unknown): void {
    state.error = stableError(operation, error);
  }

  function applySnapshot(snapshot: HaloSnapshot): boolean {
    if (
      state.snapshot !== null &&
      snapshot.revision <= state.snapshot.revision
    ) {
      return false;
    }
    state.snapshot = snapshot;
    acceptedSnapshotCount += 1;
    return true;
  }

  function applyAdapterStatus(status: AdapterStatus): boolean {
    if (
      acceptedAdapterStatusCount > 0 &&
      status.revision <= state.adapterStatus.revision
    ) {
      return false;
    }
    if (status.revision < state.adapterStatus.revision) {
      return false;
    }
    state.adapterStatus = status;
    acceptedAdapterStatusCount += 1;
    return true;
  }

  function applyDeviceStatus(status: DeviceStatus): boolean {
    if (
      acceptedDeviceStatusCount > 0 &&
      status.revision <= state.deviceStatus.revision
    ) {
      return false;
    }
    if (status.revision < state.deviceStatus.revision) {
      return false;
    }
    state.deviceStatus = status;
    acceptedDeviceStatusCount += 1;
    return true;
  }

  function load(): Promise<void> {
    if (loadPromise) {
      return loadPromise;
    }

    state.loading = true;
    state.error = null;
    const acceptedCountAtStart = acceptedSnapshotCount;
    const request = (async () => {
      try {
        const snapshot = await bridge.getSnapshot();
        applySnapshot(snapshot);
      } catch (error: unknown) {
        if (acceptedSnapshotCount === acceptedCountAtStart) {
          recordError("load", error);
        }
      }
    })();
    const tracked = request.finally(() => {
      if (loadPromise === tracked) {
        loadPromise = null;
        state.loading = false;
      }
    });
    loadPromise = tracked;
    return tracked;
  }

  async function refreshAdapterStatus(): Promise<void> {
    const acceptedCountAtStart = acceptedAdapterStatusCount;
    try {
      applyAdapterStatus(await bridge.getAdapterStatus());
    } catch (error: unknown) {
      if (acceptedAdapterStatusCount === acceptedCountAtStart) {
        if (acceptedAdapterStatusCount === 0) {
          state.adapterStatus = {
            revision: state.adapterStatus.revision,
            state: "offline",
            mode: "hook",
            message: "无法读取适配器状态",
            acceptedEvents: 0,
            ignoredEvents: 0,
            rejectedEvents: 0,
          };
        }
        recordError("adapterStatus", error);
      }
    }
  }

  async function refreshDeviceStatus(): Promise<void> {
    const acceptedCountAtStart = acceptedDeviceStatusCount;
    try {
      applyDeviceStatus(await bridge.getDeviceStatus());
    } catch (error: unknown) {
      if (acceptedDeviceStatusCount === acceptedCountAtStart) {
        if (acceptedDeviceStatusCount === 0) {
          state.deviceStatus = {
            ...state.deviceStatus,
            state: "error",
            message: null,
          };
        }
        recordError("deviceStatus", error);
      }
    }
  }

  function scheduleSubscription(
    targetRunning: boolean,
    generation: number,
  ): Promise<boolean> {
    const transition = async () => {
      if (!targetRunning) {
        const cleanupSnapshot = snapshotUnlisten;
        const cleanupAdapterStatus = adapterStatusUnlisten;
        const cleanupDeviceStatus = deviceStatusUnlisten;
        snapshotUnlisten = null;
        adapterStatusUnlisten = null;
        deviceStatusUnlisten = null;
        cleanupSubscriptions(
          cleanupSnapshot,
          cleanupAdapterStatus,
          cleanupDeviceStatus,
        );
        return false;
      }

      if (snapshotUnlisten && adapterStatusUnlisten && deviceStatusUnlisten) {
        return true;
      }

      const subscriptions = [
        beginSubscription(() =>
          bridge.subscribeSnapshots((snapshot) => {
            if (desiredRunning && lifecycleGeneration === generation) {
              applySnapshot(snapshot);
            }
          }),
        ),
        beginSubscription(() =>
          bridge.subscribeAdapterStatus((status) => {
            if (desiredRunning && lifecycleGeneration === generation) {
              applyAdapterStatus(status);
            }
          }),
        ),
        beginSubscription(() =>
          bridge.subscribeDeviceStatus((status) => {
            if (desiredRunning && lifecycleGeneration === generation) {
              applyDeviceStatus(status);
            }
          }),
        ),
      ];

      return new Promise<boolean>((resolve) => {
        const unlistens: Array<(() => void) | null> = [null, null, null];
        let remaining = subscriptions.length;
        let settled = false;

        const finish = (error?: unknown) => {
          if (settled) {
            return;
          }
          settled = true;
          cleanupSubscriptions(...unlistens);
          unlistens.fill(null);
          if (pendingSubscriptionAttempt === attempt) {
            pendingSubscriptionAttempt = null;
          }
          if (
            error !== undefined &&
            desiredRunning &&
            lifecycleGeneration === generation
          ) {
            desiredRunning = false;
            recordError("subscribe", error);
          }
          resolve(false);
        };
        const attempt = {
          cancel: () => finish(),
        };
        pendingSubscriptionAttempt = attempt;

        subscriptions.forEach((subscription, index) => {
          subscription.then(
            (unlisten) => {
              if (settled) {
                cleanupSubscriptions(unlisten);
                return;
              }
              unlistens[index] = unlisten;
              remaining -= 1;
              if (
                !desiredRunning ||
                lifecycleGeneration !== generation
              ) {
                finish();
                return;
              }
              if (remaining === 0) {
                settled = true;
                if (pendingSubscriptionAttempt === attempt) {
                  pendingSubscriptionAttempt = null;
                }
                [
                  snapshotUnlisten,
                  adapterStatusUnlisten,
                  deviceStatusUnlisten,
                ] = unlistens;
                resolve(true);
              }
            },
            (error: unknown) => finish(error),
          );
        });

        if (!desiredRunning || lifecycleGeneration !== generation) {
          finish();
        }
      });
    };

    lifecyclePromise = lifecyclePromise.then(transition, transition);
    return lifecyclePromise;
  }

  function start(): Promise<boolean> {
    if (desiredRunning) {
      return lifecyclePromise;
    }
    desiredRunning = true;
    lifecycleGeneration += 1;
    return scheduleSubscription(true, lifecycleGeneration);
  }

  function stop(): Promise<boolean> {
    if (
      !desiredRunning &&
      !snapshotUnlisten &&
      !adapterStatusUnlisten &&
      !deviceStatusUnlisten &&
      !pendingSubscriptionAttempt
    ) {
      return lifecyclePromise;
    }
    desiredRunning = false;
    lifecycleGeneration += 1;
    pendingSubscriptionAttempt?.cancel();
    return scheduleSubscription(false, lifecycleGeneration);
  }

  async function replaceFromCommand(
    operation: Exclude<
      HaloStoreOperation,
      "load" | "subscribe" | "adapterStatus" | "deviceStatus"
    >,
    command: () => Promise<HaloSnapshot>,
  ): Promise<HaloSnapshot | null> {
    const generation = ++commandGeneration;
    state.error = null;
    try {
      const snapshot = await command();
      const current = state.snapshot;
      if (current !== null && snapshot.revision < current.revision) {
        return null;
      }
      if (applySnapshot(snapshot)) {
        return snapshot;
      }
      return state.snapshot;
    } catch (error: unknown) {
      if (generation === commandGeneration) {
        recordError(operation, error);
      }
      return null;
    }
  }

  function simulateSignal(
    input: SimulateSignalInput,
  ): Promise<HaloSnapshot | null> {
    return replaceFromCommand("simulateSignal", () =>
      bridge.simulateSignal(input),
    );
  }

  function manualBind(input: ManualBindInput): Promise<HaloSnapshot | null> {
    return replaceFromCommand("manualBind", () => bridge.manualBind(input));
  }

  function toggleLock(slot: number): Promise<HaloSnapshot | null> {
    return replaceFromCommand("toggleLock", () => bridge.toggleLock(slot));
  }

  function swapSlots(
    left: number,
    right: number,
  ): Promise<HaloSnapshot | null> {
    return replaceFromCommand("swapSlots", () =>
      bridge.swapSlots(left, right),
    );
  }

  function updateEffect(
    input: UpdateEffectInput,
  ): Promise<HaloSnapshot | null> {
    return replaceFromCommand("updateEffect", () => bridge.updateEffect(input));
  }

  function setGlobalBrightness(
    value: number,
  ): Promise<HaloSnapshot | null> {
    return replaceFromCommand("setGlobalBrightness", () =>
      bridge.setGlobalBrightness(value),
    );
  }

  function setPresentation(input: {
    displayMode: DisplayMode;
    selectedSlot: number | null;
  }): Promise<HaloSnapshot | null> {
    return replaceFromCommand("setPresentation", () =>
      bridge.setPresentation(input),
    );
  }

  return {
    state,
    occupiedSlotCount,
    load,
    refreshAdapterStatus,
    refreshDeviceStatus,
    start,
    stop,
    simulateSignal,
    manualBind,
    toggleLock,
    swapSlots,
    updateEffect,
    setGlobalBrightness,
    setPresentation,
  };
}
