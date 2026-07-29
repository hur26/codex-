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
  state: "offline",
  mode: "hook",
  message: "适配器状态尚未读取",
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

  let unlisten: (() => void) | null = null;
  let desiredRunning = false;
  let lifecycleGeneration = 0;
  let lifecyclePromise = Promise.resolve();
  let snapshotRevision = 0;
  let loadPromise: Promise<void> | null = null;

  function recordError(operation: HaloStoreOperation, error: unknown): void {
    state.error = stableError(operation, error);
  }

  function applySnapshot(snapshot: HaloSnapshot): void {
    state.snapshot = snapshot;
    snapshotRevision += 1;
  }

  function load(): Promise<void> {
    if (loadPromise) {
      return loadPromise;
    }

    state.loading = true;
    state.error = null;
    const revisionAtStart = snapshotRevision;
    const request = (async () => {
      try {
        const snapshot = await bridge.getSnapshot();
        if (snapshotRevision === revisionAtStart) {
          applySnapshot(snapshot);
        }
      } catch (error: unknown) {
        if (snapshotRevision === revisionAtStart) {
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
    try {
      state.adapterStatus = await bridge.getAdapterStatus();
    } catch (error: unknown) {
      state.adapterStatus = {
        state: "offline",
        mode: "hook",
        message: "无法读取适配器状态",
      };
      recordError("adapterStatus", error);
    }
  }

  function scheduleSubscription(
    targetRunning: boolean,
    generation: number,
  ): Promise<void> {
    const transition = async () => {
      if (!targetRunning) {
        const cleanup = unlisten;
        unlisten = null;
        cleanup?.();
        return;
      }

      if (unlisten) {
        return;
      }

      try {
        const cleanup = await bridge.subscribeSnapshots((snapshot) => {
          if (desiredRunning && lifecycleGeneration === generation) {
            applySnapshot(snapshot);
          }
        });
        if (!desiredRunning || lifecycleGeneration !== generation) {
          cleanup();
          return;
        }
        unlisten = cleanup;
      } catch (error: unknown) {
        if (desiredRunning && lifecycleGeneration === generation) {
          desiredRunning = false;
          recordError("subscribe", error);
        }
      }
    };

    lifecyclePromise = lifecyclePromise.then(transition, transition);
    return lifecyclePromise;
  }

  function start(): Promise<void> {
    if (desiredRunning) {
      return lifecyclePromise;
    }
    desiredRunning = true;
    lifecycleGeneration += 1;
    return scheduleSubscription(true, lifecycleGeneration);
  }

  function stop(): Promise<void> {
    if (!desiredRunning && !unlisten) {
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
