import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AdapterStatus,
  BindingMode,
  Confidence,
  EffectProfile,
  HaloSnapshot,
  ManualBindInput,
  RingSlot,
  SignalSource,
  SimulateSignalInput,
  TaskKey,
  TaskRecord,
  TaskStatus,
  UpdateEffectInput,
} from "../types/halo";

export interface HaloBridge {
  getSnapshot(): Promise<HaloSnapshot>;
  subscribeSnapshots(
    listener: (snapshot: HaloSnapshot) => void,
  ): Promise<() => void>;
  getAdapterStatus(): Promise<AdapterStatus>;
  simulateSignal(input: SimulateSignalInput): Promise<HaloSnapshot>;
  manualBind(input: ManualBindInput): Promise<HaloSnapshot>;
  toggleLock(slot: number): Promise<HaloSnapshot>;
  swapSlots(left: number, right: number): Promise<HaloSnapshot>;
  updateEffect(input: UpdateEffectInput): Promise<HaloSnapshot>;
  setGlobalBrightness(value: number): Promise<HaloSnapshot>;
}

class TauriHaloBridge implements HaloBridge {
  getSnapshot(): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("get_snapshot");
  }

  async subscribeSnapshots(
    listener: (snapshot: HaloSnapshot) => void,
  ): Promise<() => void> {
    return listen<HaloSnapshot>("halo://snapshot", (event) => {
      listener(event.payload);
    });
  }

  getAdapterStatus(): Promise<AdapterStatus> {
    return invoke<AdapterStatus>("get_adapter_status");
  }

  simulateSignal(input: SimulateSignalInput): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("simulate_signal", { input });
  }

  manualBind(input: ManualBindInput): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("manual_bind", { input });
  }

  toggleLock(slot: number): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("toggle_lock", { slot });
  }

  swapSlots(left: number, right: number): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("swap_slots", { left, right });
  }

  updateEffect(input: UpdateEffectInput): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("update_effect", { input });
  }

  setGlobalBrightness(value: number): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("set_global_brightness", { value });
  }
}

const DEFAULT_EFFECT: EffectProfile = {
  brightness: 80,
  speedPercent: 100,
  direction: "clockwise",
  tailPercent: 35,
};

const DEMO_ADAPTER_STATUS: AdapterStatus = {
  state: "degraded",
  mode: "demo",
  message: "浏览器演示模式：未连接 Codex Hook",
};

function emptySlot(index: number): RingSlot {
  return {
    index,
    taskKey: null,
    status: "idle",
    source: null,
    confidence: null,
    bindingMode: "auto",
    locked: false,
    effect: { ...DEFAULT_EFFECT },
  };
}

function initialDemoSnapshot(): HaloSnapshot {
  return {
    deviceMode: "virtual",
    globalBrightness: 100,
    slots: Array.from({ length: 4 }, (_, index) => emptySlot(index)),
    tasks: [],
    queue: [],
  };
}

function cloneSnapshot(snapshot: HaloSnapshot): HaloSnapshot {
  return {
    ...snapshot,
    slots: snapshot.slots.map((slot) => ({
      ...slot,
      effect: { ...slot.effect },
    })),
    tasks: snapshot.tasks.map((task) => ({ ...task })),
    queue: snapshot.queue.map((task) => ({ ...task })),
  };
}

function normalizedDemoState(signalKind: SimulateSignalInput["signalKind"]): {
  status: TaskStatus;
  source: SignalSource;
  confidence: Confidence;
} {
  const statusBySignal: Record<
    SimulateSignalInput["signalKind"],
    TaskStatus
  > = {
    userPromptSubmit: "running",
    preToolUse: "running",
    postToolUse: "running",
    permissionRequest: "waiting",
    stop: "roundCompleted",
    failed: "failed",
  };

  return {
    status: statusBySignal[signalKind],
    source: "simulator",
    confidence: "simulated",
  };
}

function assertSlot(snapshot: HaloSnapshot, slot: number): RingSlot {
  const selected = snapshot.slots[slot];
  if (!selected) {
    throw { code: "slotOutOfBounds", slot };
  }
  return selected;
}

function assertTask(snapshot: HaloSnapshot, taskKey: TaskKey): TaskRecord {
  const task = snapshot.tasks.find((candidate) => candidate.taskKey === taskKey);
  if (!task) {
    throw { code: "taskNotFound", taskKey };
  }
  return task;
}

function slotFromTask(
  slot: RingSlot,
  task: TaskRecord,
  bindingMode: BindingMode,
  locked: boolean,
): RingSlot {
  return {
    ...slot,
    taskKey: task.taskKey,
    status: task.status,
    source: task.source,
    confidence: task.confidence,
    bindingMode,
    locked,
  };
}

function clearSlot(slot: RingSlot): RingSlot {
  return {
    ...slot,
    taskKey: null,
    status: "idle",
    source: null,
    confidence: null,
    bindingMode: "auto",
    locked: false,
  };
}

class DemoHaloBridge implements HaloBridge {
  private snapshot = initialDemoSnapshot();
  private readonly listeners = new Set<(snapshot: HaloSnapshot) => void>();

  async getSnapshot(): Promise<HaloSnapshot> {
    return cloneSnapshot(this.snapshot);
  }

  async subscribeSnapshots(
    listener: (snapshot: HaloSnapshot) => void,
  ): Promise<() => void> {
    this.listeners.add(listener);
    let active = true;

    return () => {
      if (active) {
        active = false;
        this.listeners.delete(listener);
      }
    };
  }

  async getAdapterStatus(): Promise<AdapterStatus> {
    return { ...DEMO_ADAPTER_STATUS };
  }

  async simulateSignal(input: SimulateSignalInput): Promise<HaloSnapshot> {
    const normalized = normalizedDemoState(input.signalKind);
    const task: TaskRecord = {
      taskKey: input.taskKey,
      ...normalized,
      lastActiveAtMs: input.receivedAtMs,
    };
    const existingTaskIndex = this.snapshot.tasks.findIndex(
      (candidate) => candidate.taskKey === input.taskKey,
    );

    if (existingTaskIndex >= 0) {
      this.snapshot.tasks[existingTaskIndex] = task;
    } else {
      this.snapshot.tasks.push(task);
    }

    const assignedSlot = this.snapshot.slots.find(
      (slot) => slot.taskKey === input.taskKey,
    );
    if (assignedSlot) {
      Object.assign(
        assignedSlot,
        slotFromTask(
          assignedSlot,
          task,
          assignedSlot.bindingMode,
          assignedSlot.locked,
        ),
      );
    } else {
      const empty = this.snapshot.slots.find((slot) => slot.taskKey === null);
      if (empty) {
        Object.assign(empty, slotFromTask(empty, task, "auto", false));
        this.snapshot.queue = this.snapshot.queue.filter(
          (queued) => queued.taskKey !== task.taskKey,
        );
      } else {
        this.snapshot.queue = [
          task,
          ...this.snapshot.queue.filter(
            (queued) => queued.taskKey !== task.taskKey,
          ),
        ];
      }
    }

    return this.publish();
  }

  async manualBind(input: ManualBindInput): Promise<HaloSnapshot> {
    const target = assertSlot(this.snapshot, input.slot);
    const task = assertTask(this.snapshot, input.taskKey);
    const current = this.snapshot.slots.find(
      (slot) => slot.taskKey === task.taskKey,
    );
    const displacedTaskKey =
      target.taskKey === task.taskKey ? null : target.taskKey;

    if (current && current.index !== target.index) {
      Object.assign(current, clearSlot(current));
    }
    Object.assign(target, slotFromTask(target, task, "manual", input.lock));
    this.snapshot.queue = this.snapshot.queue.filter(
      (queued) => queued.taskKey !== task.taskKey,
    );

    if (displacedTaskKey) {
      const displaced = assertTask(this.snapshot, displacedTaskKey);
      this.snapshot.queue = [
        displaced,
        ...this.snapshot.queue.filter(
          (queued) => queued.taskKey !== displacedTaskKey,
        ),
      ];
    }

    return this.publish();
  }

  async toggleLock(slot: number): Promise<HaloSnapshot> {
    const target = assertSlot(this.snapshot, slot);
    if (target.taskKey === null) {
      throw { code: "emptySlot", slot };
    }
    target.locked = !target.locked;
    return this.publish();
  }

  async swapSlots(left: number, right: number): Promise<HaloSnapshot> {
    const leftSlot = assertSlot(this.snapshot, left);
    const rightSlot = assertSlot(this.snapshot, right);
    const leftEffect = { ...leftSlot.effect };
    const rightEffect = { ...rightSlot.effect };

    this.snapshot.slots[left] = {
      ...rightSlot,
      index: left,
      effect: leftEffect,
    };
    this.snapshot.slots[right] = {
      ...leftSlot,
      index: right,
      effect: rightEffect,
    };
    return this.publish();
  }

  async updateEffect(input: UpdateEffectInput): Promise<HaloSnapshot> {
    const target = assertSlot(this.snapshot, input.slot);
    target.effect = {
      brightness: input.brightness,
      speedPercent: input.speedPercent,
      direction: input.direction,
      tailPercent: input.tailPercent,
    };
    return this.publish();
  }

  async setGlobalBrightness(value: number): Promise<HaloSnapshot> {
    this.snapshot.globalBrightness = value;
    return this.publish();
  }

  private publish(): HaloSnapshot {
    const next = cloneSnapshot(this.snapshot);
    for (const listener of this.listeners) {
      listener(cloneSnapshot(next));
    }
    return next;
  }
}

type TauriGlobal = typeof globalThis & {
  __TAURI__?: unknown;
  __TAURI_INTERNALS__?: unknown;
};

export function isTauriRuntime(): boolean {
  const runtime = globalThis as TauriGlobal;
  return (
    typeof runtime.__TAURI_INTERNALS__ !== "undefined" ||
    typeof runtime.__TAURI__ !== "undefined"
  );
}

export function createDemoHaloBridge(): HaloBridge {
  return new DemoHaloBridge();
}

export function createHaloBridge(): HaloBridge {
  return isTauriRuntime() ? new TauriHaloBridge() : createDemoHaloBridge();
}

export const haloBridge = createHaloBridge();
