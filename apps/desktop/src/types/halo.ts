export type TaskKey = string;

export type TaskStatus =
  | "running"
  | "waiting"
  | "roundCompleted"
  | "failed"
  | "queued"
  | "idle"
  | "unknown";

export type SignalSource = "hook" | "simulator";
export type Confidence = "observed" | "provisional" | "simulated";
export type SignalKind =
  | "userPromptSubmit"
  | "preToolUse"
  | "postToolUse"
  | "permissionRequest"
  | "stop"
  | "failed";
export type BindingMode = "auto" | "manual";
export type DeviceMode = "virtual";
export type Direction = "clockwise" | "counterClockwise";
export type DisplayMode = "ambient" | "overview" | "detail";

export interface EffectProfile {
  brightness: number;
  speedPercent: number;
  direction: Direction;
  tailPercent: number;
}

export interface TaskRecord {
  taskKey: TaskKey;
  status: TaskStatus;
  source: SignalSource;
  confidence: Confidence;
  lastActiveAtMs: number;
}

export interface RingSlot {
  index: number;
  taskKey: TaskKey | null;
  status: TaskStatus;
  source: SignalSource | null;
  confidence: Confidence | null;
  bindingMode: BindingMode;
  locked: boolean;
  effect: EffectProfile;
}

export interface HaloSnapshot {
  revision: number;
  deviceMode: DeviceMode;
  globalBrightness: number;
  slots: RingSlot[];
  tasks: TaskRecord[];
  queue: TaskRecord[];
}

export interface SimulateSignalInput {
  taskKey: TaskKey;
  signalKind: SignalKind;
  receivedAtMs: number;
}

export interface ManualBindInput {
  taskKey: TaskKey;
  slot: number;
  lock: boolean;
}

export type TaskDragOrigin =
  | { kind: "queue" }
  | { kind: "slot"; slot: number };

export type ActiveDrag =
  | { kind: "task"; taskKey: TaskKey; origin: TaskDragOrigin }
  | { kind: "slot"; slot: number; taskKey: TaskKey };

export interface UpdateEffectInput extends EffectProfile {
  slot: number;
}

export type AdapterState = "online" | "degraded" | "offline";
export type AdapterMode = "hook" | "demo";

export interface AdapterStatus {
  state: AdapterState;
  mode: AdapterMode;
  message: string | null;
  acceptedEvents: number;
  ignoredEvents: number;
  rejectedEvents: number;
}
