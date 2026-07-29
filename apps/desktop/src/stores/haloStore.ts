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
  let startPromise: Promise<void> | null = null;

  function recordError(operation: HaloStoreOperation, error: unknown): void {
    state.error = stableError(operation, error);
  }

  async function load(): Promise<void> {
    state.loading = true;
    state.error = null;
    try {
      state.snapshot = await bridge.getSnapshot();
    } catch (error: unknown) {
      recordError("load", error);
    } finally {
      state.loading = false;
    }
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

  async function start(): Promise<void> {
    if (unlisten) {
      return;
    }
    if (startPromise) {
      return startPromise;
    }

    startPromise = bridge
      .subscribeSnapshots((snapshot) => {
        state.snapshot = snapshot;
      })
      .then((cleanup) => {
        unlisten = cleanup;
      })
      .catch((error: unknown) => {
        recordError("subscribe", error);
      })
      .finally(() => {
        startPromise = null;
      });
    return startPromise;
  }

  async function stop(): Promise<void> {
    if (startPromise) {
      await startPromise;
    }
    const cleanup = unlisten;
    unlisten = null;
    cleanup?.();
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
      state.snapshot = snapshot;
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
