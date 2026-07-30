import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AdapterStatus,
  BindingMode,
  Confidence,
  DeviceStatus,
  DisplayMode,
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
  subscribeAdapterStatus(
    listener: (status: AdapterStatus) => void,
  ): Promise<() => void>;
  getAdapterStatus(): Promise<AdapterStatus>;
  getDeviceStatus(): Promise<DeviceStatus>;
  subscribeDeviceStatus(
    listener: (status: DeviceStatus) => void,
  ): Promise<() => void>;
  simulateSignal(input: SimulateSignalInput): Promise<HaloSnapshot>;
  manualBind(input: ManualBindInput): Promise<HaloSnapshot>;
  toggleLock(slot: number): Promise<HaloSnapshot>;
  swapSlots(left: number, right: number): Promise<HaloSnapshot>;
  updateEffect(input: UpdateEffectInput): Promise<HaloSnapshot>;
  setGlobalBrightness(value: number): Promise<HaloSnapshot>;
  setPresentation(input: {
    displayMode: DisplayMode;
    selectedSlot: number | null;
  }): Promise<HaloSnapshot>;
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

  async subscribeAdapterStatus(
    listener: (status: AdapterStatus) => void,
  ): Promise<() => void> {
    return listen<AdapterStatus>("halo://adapter-status", (event) => {
      listener(event.payload);
    });
  }

  getAdapterStatus(): Promise<AdapterStatus> {
    return invoke<AdapterStatus>("get_adapter_status");
  }

  getDeviceStatus(): Promise<DeviceStatus> {
    return invoke<DeviceStatus>("get_device_status");
  }

  async subscribeDeviceStatus(
    listener: (status: DeviceStatus) => void,
  ): Promise<() => void> {
    return listen<DeviceStatus>("halo://device-status", (event) => {
      listener(event.payload);
    });
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

  setPresentation(input: {
    displayMode: DisplayMode;
    selectedSlot: number | null;
  }): Promise<HaloSnapshot> {
    return invoke<HaloSnapshot>("set_presentation", { input });
  }
}

const DEFAULT_EFFECT: EffectProfile = {
  brightness: 80,
  speedPercent: 100,
  direction: "clockwise",
  tailPercent: 35,
};

const DEMO_ADAPTER_STATUS: AdapterStatus = {
  revision: 1,
  state: "degraded",
  mode: "demo",
  message: "浏览器演示模式：未连接 Codex Hook",
  acceptedEvents: 0,
  ignoredEvents: 0,
  rejectedEvents: 0,
};

const DEMO_DEVICE_STATUS: DeviceStatus = {
  revision: 1,
  state: "virtual",
  transport: "simulator",
  message: null,
  firmwareVersion: "0.1.0",
  retryCount: 0,
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
    revision: 0,
    deviceMode: "virtual",
    displayMode: "ambient",
    selectedSlot: null,
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
  private readonly queue: TaskKey[] = [];
  private readonly listeners = new Set<(snapshot: HaloSnapshot) => void>();
  private readonly adapterStatusListeners = new Set<
    (status: AdapterStatus) => void
  >();
  private readonly deviceStatusListeners = new Set<
    (status: DeviceStatus) => void
  >();

  async getSnapshot(): Promise<HaloSnapshot> {
    return this.currentSnapshot();
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

  async subscribeAdapterStatus(
    listener: (status: AdapterStatus) => void,
  ): Promise<() => void> {
    this.adapterStatusListeners.add(listener);
    let active = true;

    return () => {
      if (active) {
        active = false;
        this.adapterStatusListeners.delete(listener);
      }
    };
  }

  async getAdapterStatus(): Promise<AdapterStatus> {
    return { ...DEMO_ADAPTER_STATUS };
  }

  async getDeviceStatus(): Promise<DeviceStatus> {
    return { ...DEMO_DEVICE_STATUS };
  }

  async subscribeDeviceStatus(
    listener: (status: DeviceStatus) => void,
  ): Promise<() => void> {
    this.deviceStatusListeners.add(listener);
    let active = true;

    return () => {
      if (active) {
        active = false;
        this.deviceStatusListeners.delete(listener);
      }
    };
  }

  async simulateSignal(input: SimulateSignalInput): Promise<HaloSnapshot> {
    const normalized = normalizedDemoState(input.signalKind);
    const existingTask = this.snapshot.tasks.find(
      (candidate) => candidate.taskKey === input.taskKey,
    );
    if (
      existingTask &&
      (input.receivedAtMs < existingTask.lastActiveAtMs ||
        (input.receivedAtMs === existingTask.lastActiveAtMs &&
          normalized.status === existingTask.status &&
          normalized.source === existingTask.source &&
          normalized.confidence === existingTask.confidence))
    ) {
      return this.currentSnapshot();
    }

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
      this.removeFromQueue(task.taskKey);
    } else {
      const empty = this.snapshot.slots.find((slot) => slot.taskKey === null);
      if (empty) {
        Object.assign(empty, slotFromTask(empty, task, "auto", false));
        this.removeFromQueue(task.taskKey);
      } else {
        this.enqueueRecent(task.taskKey);
      }
    }

    return this.publishChanged();
  }

  async manualBind(input: ManualBindInput): Promise<HaloSnapshot> {
    const target = assertSlot(this.snapshot, input.slot);
    const task = assertTask(this.snapshot, input.taskKey);
    const current = this.snapshot.slots.find(
      (slot) => slot.taskKey === task.taskKey,
    );
    if (target.locked) {
      if (
        current?.index === target.index &&
        target.bindingMode === "manual" &&
        input.lock
      ) {
        return this.currentSnapshot();
      }
      throw { code: "slotLocked", slot: target.index };
    }
    if (current?.locked) {
      throw { code: "slotLocked", slot: current.index };
    }
    if (current?.index === target.index) {
      const changed =
        target.bindingMode !== "manual" || target.locked !== input.lock;
      Object.assign(target, slotFromTask(target, task, "manual", input.lock));
      this.removeFromQueue(task.taskKey);
      return changed ? this.publishChanged() : this.currentSnapshot();
    }

    const displacedTaskKey =
      target.taskKey === task.taskKey ? null : target.taskKey;

    if (current) {
      Object.assign(current, clearSlot(current));
    }
    Object.assign(target, slotFromTask(target, task, "manual", input.lock));
    this.removeFromQueue(task.taskKey);

    if (displacedTaskKey) {
      this.enqueueRecent(displacedTaskKey);
    }
    this.fillEmptySlotsFromQueue();

    return this.publishChanged();
  }

  async toggleLock(slot: number): Promise<HaloSnapshot> {
    const target = assertSlot(this.snapshot, slot);
    if (target.taskKey === null) {
      throw { code: "emptySlot", slot };
    }
    target.locked = !target.locked;
    return this.publishChanged();
  }

  async swapSlots(left: number, right: number): Promise<HaloSnapshot> {
    const leftSlot = assertSlot(this.snapshot, left);
    const rightSlot = assertSlot(this.snapshot, right);
    if (left === right) {
      return this.currentSnapshot();
    }
    if (leftSlot.locked) {
      throw { code: "slotLocked", slot: left };
    }
    if (rightSlot.locked) {
      throw { code: "slotLocked", slot: right };
    }
    if (sameSlotAssignment(leftSlot, rightSlot)) {
      return this.currentSnapshot();
    }
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
    return this.publishChanged();
  }

  async updateEffect(input: UpdateEffectInput): Promise<HaloSnapshot> {
    const target = assertSlot(this.snapshot, input.slot);
    const effect = {
      brightness: input.brightness,
      speedPercent: input.speedPercent,
      direction: input.direction,
      tailPercent: input.tailPercent,
    };
    if (sameEffect(target.effect, effect)) {
      return this.currentSnapshot();
    }
    target.effect = effect;
    return this.publishChanged();
  }

  async setGlobalBrightness(value: number): Promise<HaloSnapshot> {
    if (this.snapshot.globalBrightness === value) {
      return this.currentSnapshot();
    }
    this.snapshot.globalBrightness = value;
    return this.publishChanged();
  }

  async setPresentation(input: {
    displayMode: DisplayMode;
    selectedSlot: number | null;
  }): Promise<HaloSnapshot> {
    if (
      input.selectedSlot !== null &&
      (input.selectedSlot < 0 || input.selectedSlot >= this.snapshot.slots.length)
    ) {
      throw { code: "slotOutOfBounds", slot: input.selectedSlot };
    }
    if (
      this.snapshot.displayMode === input.displayMode &&
      this.snapshot.selectedSlot === input.selectedSlot
    ) {
      return this.currentSnapshot();
    }
    this.snapshot.displayMode = input.displayMode;
    this.snapshot.selectedSlot = input.selectedSlot;
    return this.publishChanged();
  }

  private publishChanged(): HaloSnapshot {
    this.snapshot.revision += 1;
    const next = this.currentSnapshot();
    for (const listener of this.listeners) {
      listener(cloneSnapshot(next));
    }
    return next;
  }

  private currentSnapshot(): HaloSnapshot {
    const tasks = this.snapshot.tasks
      .map((task) => ({ ...task }))
      .sort(compareTasksByActivity);
    const queue = this.queue.flatMap((taskKey) => {
      const task = this.snapshot.tasks.find(
        (candidate) => candidate.taskKey === taskKey,
      );
      return task ? [{ ...task, status: "queued" as const }] : [];
    });
    return cloneSnapshot({
      ...this.snapshot,
      tasks,
      queue,
    });
  }

  private removeFromQueue(taskKey: TaskKey): void {
    const index = this.queue.indexOf(taskKey);
    if (index >= 0) {
      this.queue.splice(index, 1);
    }
  }

  private enqueueRecent(taskKey: TaskKey): void {
    this.removeFromQueue(taskKey);
    this.queue.push(taskKey);
    this.queue.sort((left, right) => {
      const leftTask = assertTask(this.snapshot, left);
      const rightTask = assertTask(this.snapshot, right);
      return (
        rightTask.lastActiveAtMs - leftTask.lastActiveAtMs ||
        compareTaskKeys(left, right)
      );
    });
  }

  private fillEmptySlotsFromQueue(): void {
    let empty = this.snapshot.slots.find((slot) => slot.taskKey === null);
    while (empty && this.queue.length > 0) {
      const taskKey = this.queue.shift();
      if (!taskKey) {
        return;
      }
      const task = assertTask(this.snapshot, taskKey);
      Object.assign(empty, slotFromTask(empty, task, "auto", false));
      empty = this.snapshot.slots.find((slot) => slot.taskKey === null);
    }
  }
}

function sameEffect(left: EffectProfile, right: EffectProfile): boolean {
  return (
    left.brightness === right.brightness &&
    left.speedPercent === right.speedPercent &&
    left.direction === right.direction &&
    left.tailPercent === right.tailPercent
  );
}

function sameSlotAssignment(left: RingSlot, right: RingSlot): boolean {
  return (
    left.taskKey === right.taskKey &&
    left.status === right.status &&
    left.source === right.source &&
    left.confidence === right.confidence &&
    left.bindingMode === right.bindingMode &&
    left.locked === right.locked
  );
}

function compareTaskKeys(left: TaskKey, right: TaskKey): number {
  if (left === right) {
    return 0;
  }
  return left < right ? -1 : 1;
}

function compareTasksByActivity(left: TaskRecord, right: TaskRecord): number {
  return (
    right.lastActiveAtMs - left.lastActiveAtMs ||
    compareTaskKeys(left.taskKey, right.taskKey)
  );
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
