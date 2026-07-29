import { computed, reactive } from "vue";
import { haloBridge, type HaloBridge } from "../services/haloBridge";
import type {
  AdapterStatus,
  HaloSnapshot,
  ManualBindInput,
  SimulateSignalInput,
  UpdateEffectInput,
} from "../types/halo";

export type HaloStoreOperation =
  | "load"
  | "subscribe"
  | "adapterStatus"
  | "simulateSignal"
  | "manualBind"
  | "toggleLock"
  | "swapSlots"
  | "updateEffect"
  | "setGlobalBrightness";

export interface HaloStoreError {
  operation: HaloStoreOperation;
  code: string;
  message: string;
}

export interface HaloStoreState {
  snapshot: HaloSnapshot | null;
  adapterStatus: AdapterStatus;
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

export function createHaloStore(bridge: HaloBridge = haloBridge) {
  const state = reactive<HaloStoreState>({
    snapshot: null,
    adapterStatus: { ...INITIAL_ADAPTER_STATUS },
    loading: false,
    error: null,
  });
  const occupiedSlotCount = computed(
    () =>
      state.snapshot?.slots.filter((slot) => slot.taskKey !== null).length ?? 0,
  );

  let snapshotUnlisten: (() => void) | null = null;
  let adapterStatusUnlisten: (() => void) | null = null;
  let desiredRunning = false;
  let lifecycleGeneration = 0;
  let lifecyclePromise: Promise<boolean> = Promise.resolve(false);
  let acceptedSnapshotCount = 0;
  let acceptedAdapterStatusCount = 0;
  let loadPromise: Promise<void> | null = null;

  function recordError(operation: HaloStoreOperation, error: unknown): void {
    state.error = stableError(operation, error);
  }

  function applySnapshot(snapshot: HaloSnapshot): boolean {
    if (
      state.snapshot !== null &&
      snapshot.revision < state.snapshot.revision
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

  function scheduleSubscription(
    targetRunning: boolean,
    generation: number,
  ): Promise<boolean> {
    const transition = async () => {
      if (!targetRunning) {
        const cleanupSnapshot = snapshotUnlisten;
        const cleanupAdapterStatus = adapterStatusUnlisten;
        snapshotUnlisten = null;
        adapterStatusUnlisten = null;
        cleanupSnapshot?.();
        cleanupAdapterStatus?.();
        return false;
      }

      if (snapshotUnlisten && adapterStatusUnlisten) {
        return true;
      }

      const [snapshotResult, adapterStatusResult] = await Promise.allSettled([
        bridge.subscribeSnapshots((snapshot) => {
          if (desiredRunning && lifecycleGeneration === generation) {
            applySnapshot(snapshot);
          }
        }),
        bridge.subscribeAdapterStatus((status) => {
          if (desiredRunning && lifecycleGeneration === generation) {
            applyAdapterStatus(status);
          }
        }),
      ]);
      const cleanupSnapshot =
        snapshotResult.status === "fulfilled" ? snapshotResult.value : null;
      const cleanupAdapterStatus =
        adapterStatusResult.status === "fulfilled"
          ? adapterStatusResult.value
          : null;
      const subscriptionFailed =
        snapshotResult.status === "rejected" ||
        adapterStatusResult.status === "rejected";

      if (
        subscriptionFailed ||
        !desiredRunning ||
        lifecycleGeneration !== generation
      ) {
        cleanupSnapshot?.();
        cleanupAdapterStatus?.();
        if (
          subscriptionFailed &&
          desiredRunning &&
          lifecycleGeneration === generation
        ) {
          desiredRunning = false;
          recordError(
            "subscribe",
            snapshotResult.status === "rejected"
              ? snapshotResult.reason
              : adapterStatusResult.status === "rejected"
                ? adapterStatusResult.reason
                : undefined,
          );
        }
        return false;
      }

      snapshotUnlisten = cleanupSnapshot;
      adapterStatusUnlisten = cleanupAdapterStatus;
      return true;
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
    if (!desiredRunning && !snapshotUnlisten && !adapterStatusUnlisten) {
      return lifecyclePromise;
    }
    desiredRunning = false;
    lifecycleGeneration += 1;
    return scheduleSubscription(false, lifecycleGeneration);
  }

  async function replaceFromCommand(
    operation: Exclude<
      HaloStoreOperation,
      "load" | "subscribe" | "adapterStatus"
    >,
    command: () => Promise<HaloSnapshot>,
  ): Promise<HaloSnapshot | null> {
    state.error = null;
    try {
      const snapshot = await command();
      applySnapshot(snapshot);
      return snapshot;
    } catch (error: unknown) {
      recordError(operation, error);
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

  return {
    state,
    occupiedSlotCount,
    load,
    refreshAdapterStatus,
    start,
    stop,
    simulateSignal,
    manualBind,
    toggleLock,
    swapSlots,
    updateEffect,
    setGlobalBrightness,
  };
}
