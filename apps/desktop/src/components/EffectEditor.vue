<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import type {
  Direction,
  EffectProfile,
  RingSlot,
  UpdateEffectInput,
} from "../types/halo";

type NumericField = "global" | "brightness" | "speed" | "tail";

const props = withDefaults(
  defineProps<{
    globalBrightness: number;
    selectedSlot: RingSlot | null;
    pending?: boolean;
  }>(),
  {
    pending: false,
  },
);

const emit = defineEmits<{
  "set-global-brightness": [value: number];
  "update-effect": [input: UpdateEffectInput];
}>();

const draft = reactive({
  global: "",
  brightness: "",
  speed: "",
  tail: "",
  direction: "clockwise" as Direction,
});
const activeField = ref<NumericField | null>(null);
const validationError = ref<string | null>(null);

const RULES: Record<
  NumericField,
  { min: number; max: number; message: string }
> = {
  global: {
    min: 0,
    max: 100,
    message: "全局亮度必须是 0–100 的整数",
  },
  brightness: {
    min: 0,
    max: 100,
    message: "单圈亮度必须是 0–100 的整数",
  },
  speed: {
    min: 25,
    max: 300,
    message: "追逐速度必须是 25–300 的整数",
  },
  tail: {
    min: 1,
    max: 100,
    message: "光尾长度必须是 1–100 的整数",
  },
};

function syncDraft() {
  draft.global = String(props.globalBrightness);
  const effect = props.selectedSlot?.effect;
  if (effect) {
    draft.brightness = String(effect.brightness);
    draft.speed = String(effect.speedPercent);
    draft.tail = String(effect.tailPercent);
    draft.direction = effect.direction;
  }
  validationError.value = null;
}

function parseField(field: NumericField): number | null {
  const value = Number(draft[field]);
  const rule = RULES[field];
  if (
    draft[field].trim() === "" ||
    !Number.isFinite(value) ||
    !Number.isInteger(value) ||
    value < rule.min ||
    value > rule.max
  ) {
    validationError.value = rule.message;
    return null;
  }
  return value;
}

function currentProfile(): EffectProfile | null {
  const brightness = parseField("brightness");
  if (brightness === null) return null;
  const speedPercent = parseField("speed");
  if (speedPercent === null) return null;
  const tailPercent = parseField("tail");
  if (tailPercent === null) return null;

  return {
    brightness,
    speedPercent,
    direction: draft.direction,
    tailPercent,
  };
}

function updateDraft(field: NumericField, event: Event) {
  draft[field] = (event.target as HTMLInputElement).value;
  validationError.value = null;
}

function commit(field: NumericField) {
  if (field === "global") {
    const value = parseField(field);
    if (value !== null) {
      validationError.value = null;
      emit("set-global-brightness", value);
    }
    return;
  }
  commitProfile();
}

function commitProfile() {
  if (!props.selectedSlot) {
    return;
  }
  const profile = currentProfile();
  if (!profile) {
    return;
  }
  validationError.value = null;
  emit("update-effect", {
    slot: props.selectedSlot.index,
    ...profile,
  });
}

function setDirection(direction: Direction) {
  if (!props.selectedSlot || draft.direction === direction) {
    return;
  }
  draft.direction = direction;
  commitProfile();
}

function beginEditing(field: NumericField) {
  activeField.value = field;
}

function finishEditing(field: NumericField) {
  if (activeField.value === field) {
    activeField.value = null;
  }
  if (!props.pending) {
    syncDraft();
  }
}

watch(
  () => [
    props.globalBrightness,
    props.selectedSlot?.index,
    props.selectedSlot?.effect.brightness,
    props.selectedSlot?.effect.speedPercent,
    props.selectedSlot?.effect.direction,
    props.selectedSlot?.effect.tailPercent,
    props.pending,
  ],
  () => {
    if (!props.pending && activeField.value === null) {
      syncDraft();
    }
  },
  { immediate: true, flush: "sync" },
);
</script>

<template>
  <aside class="effect-editor" data-effect-editor aria-label="灯效参数编辑器">
    <header class="editor-header">
      <span>
        <small>PHOTOMETRIC CONTROL</small>
        <strong>灯效参数</strong>
      </span>
      <em :class="{ active: pending }" role="status">
        {{ pending ? "SYNC" : "READY" }}
      </em>
    </header>

    <section class="editor-bank global-bank" aria-labelledby="global-heading">
      <div class="bank-heading">
        <span>
          <small>MASTER OUTPUT</small>
          <strong id="global-heading">全局亮度</strong>
        </span>
        <output>{{ draft.global || "--" }}%</output>
      </div>

      <div class="parameter-row">
        <input
          data-global-range
          type="range"
          min="0"
          max="100"
          step="1"
          :value="draft.global"
          aria-label="全局亮度"
          @focus="beginEditing('global')"
          @blur="finishEditing('global')"
          @input="updateDraft('global', $event)"
          @change="commit('global')"
        />
        <label class="number-control">
          <span class="sr-only">全局亮度数值</span>
          <input
            data-global-number
            type="number"
            inputmode="numeric"
            min="0"
            max="100"
            step="1"
            :value="draft.global"
            aria-describedby="global-brightness-unit"
            @focus="beginEditing('global')"
            @blur="finishEditing('global')"
            @input="updateDraft('global', $event)"
            @change="commit('global')"
          />
          <span id="global-brightness-unit">%</span>
        </label>
      </div>
      <div class="scale" aria-hidden="true"><span>0</span><i /><span>100</span></div>
    </section>

    <section
      class="editor-bank channel-bank"
      :class="{ disabled: !selectedSlot }"
      aria-labelledby="channel-heading"
    >
      <div class="bank-heading">
        <span>
          <small>RING CHANNEL</small>
          <strong id="channel-heading">
            {{
              selectedSlot
                ? `物理圈 R${String(selectedSlot.index + 1).padStart(2, "0")}`
                : "选择一个物理圈"
            }}
          </strong>
        </span>
        <output>{{ selectedSlot ? "ACTIVE" : "STANDBY" }}</output>
      </div>

      <fieldset :disabled="!selectedSlot">
        <legend class="sr-only">选中物理圈灯效</legend>

        <label class="parameter-block">
          <span><strong>单圈亮度</strong><small>RING LEVEL / %</small></span>
          <div class="parameter-row">
            <input
              type="range"
              min="0"
              max="100"
              step="1"
              :value="draft.brightness"
              aria-label="单圈亮度"
              @focus="beginEditing('brightness')"
              @blur="finishEditing('brightness')"
              @input="updateDraft('brightness', $event)"
              @change="commit('brightness')"
            />
            <span class="number-control">
              <input
                data-effect-brightness-number
                type="number"
                inputmode="numeric"
                min="0"
                max="100"
                step="1"
                :value="draft.brightness"
                aria-label="单圈亮度数值"
                @focus="beginEditing('brightness')"
                @blur="finishEditing('brightness')"
                @input="updateDraft('brightness', $event)"
                @change="commit('brightness')"
              />
              <span>%</span>
            </span>
          </div>
        </label>

        <label class="parameter-block">
          <span><strong>追逐速度</strong><small>CHASE RATE / %</small></span>
          <div class="parameter-row">
            <input
              type="range"
              min="25"
              max="300"
              step="1"
              :value="draft.speed"
              aria-label="追逐速度"
              @focus="beginEditing('speed')"
              @blur="finishEditing('speed')"
              @input="updateDraft('speed', $event)"
              @change="commit('speed')"
            />
            <span class="number-control">
              <input
                data-effect-speed-number
                type="number"
                inputmode="numeric"
                min="25"
                max="300"
                step="1"
                :value="draft.speed"
                aria-label="追逐速度数值"
                @focus="beginEditing('speed')"
                @blur="finishEditing('speed')"
                @input="updateDraft('speed', $event)"
                @change="commit('speed')"
              />
              <span>%</span>
            </span>
          </div>
        </label>

        <div class="parameter-block">
          <span><strong>追逐方向</strong><small>ROTATION VECTOR</small></span>
          <div class="direction-control" aria-label="追逐方向">
            <button
              type="button"
              data-direction="clockwise"
              :aria-pressed="draft.direction === 'clockwise'"
              @click="setDirection('clockwise')"
            >
              <span aria-hidden="true">↻</span> 顺时针
            </button>
            <button
              type="button"
              data-direction="counterClockwise"
              :aria-pressed="draft.direction === 'counterClockwise'"
              @click="setDirection('counterClockwise')"
            >
              <span aria-hidden="true">↺</span> 逆时针
            </button>
          </div>
        </div>

        <label class="parameter-block">
          <span><strong>光尾长度</strong><small>TRAIL LENGTH / %</small></span>
          <div class="parameter-row">
            <input
              type="range"
              min="1"
              max="100"
              step="1"
              :value="draft.tail"
              aria-label="光尾长度"
              @focus="beginEditing('tail')"
              @blur="finishEditing('tail')"
              @input="updateDraft('tail', $event)"
              @change="commit('tail')"
            />
            <span class="number-control">
              <input
                data-effect-tail-number
                type="number"
                inputmode="numeric"
                min="1"
                max="100"
                step="1"
                :value="draft.tail"
                aria-label="光尾长度数值"
                @focus="beginEditing('tail')"
                @blur="finishEditing('tail')"
                @input="updateDraft('tail', $event)"
                @change="commit('tail')"
              />
              <span>%</span>
            </span>
          </div>
        </label>
      </fieldset>
    </section>

    <p
      v-if="validationError"
      class="validation-error"
      data-validation-error
      role="alert"
    >
      {{ validationError }}
    </p>
    <p v-else class="editor-hint">
      参数在释放滑块或确认数值后写入虚拟设备
    </p>
  </aside>
</template>

<style scoped>
.effect-editor {
  position: absolute;
  z-index: 40;
  top: 1rem;
  left: 1rem;
  width: 14.25rem;
  padding: 0.78rem;
  border: 1px solid var(--halo-hairline);
  border-radius: 0.45rem;
  color: var(--halo-text);
  background:
    linear-gradient(145deg, var(--halo-glass-sheen), transparent 32%),
    rgb(10 14 15 / 94%);
  box-shadow: 0 1rem 2rem var(--halo-shadow);
  backdrop-filter: blur(0.8rem);
}

.editor-header,
.bank-heading,
.parameter-block > span,
.parameter-row,
.scale {
  display: flex;
  align-items: center;
}

.editor-header {
  justify-content: space-between;
  padding-bottom: 0.58rem;
  border-bottom: 1px solid var(--halo-hairline);
}

.editor-header > span,
.bank-heading > span {
  display: grid;
  gap: 0.08rem;
}

small,
.editor-hint,
.validation-error {
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.45rem;
  letter-spacing: 0.08em;
}

.editor-header strong,
.bank-heading strong {
  font-size: 0.67rem;
  font-weight: 540;
}

.editor-header em {
  padding: 0.18rem 0.32rem;
  border: 1px solid var(--halo-hairline);
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.43rem;
  font-style: normal;
  letter-spacing: 0.1em;
}

.editor-header em.active {
  border-color: var(--halo-running);
  color: var(--halo-running);
  box-shadow: inset 0 0 0.5rem var(--halo-running-glow);
}

.editor-bank {
  padding: 0.67rem 0 0.58rem;
  border-bottom: 1px solid var(--halo-hairline);
}

.bank-heading {
  justify-content: space-between;
  margin-bottom: 0.55rem;
}

.bank-heading output {
  color: var(--halo-focus);
  font-family: var(--halo-font-mono);
  font-size: 0.56rem;
}

.channel-bank.disabled {
  opacity: 0.48;
}

fieldset {
  display: grid;
  gap: 0.56rem;
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.parameter-block {
  display: grid;
  gap: 0.3rem;
}

.parameter-block > span {
  justify-content: space-between;
}

.parameter-block strong {
  font-size: 0.55rem;
  font-weight: 500;
}

.parameter-row {
  gap: 0.55rem;
}

input[type="range"] {
  width: 100%;
  height: 0.85rem;
  margin: 0;
  accent-color: var(--halo-running);
  cursor: ew-resize;
}

.number-control {
  display: grid;
  width: 4.1rem;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  border: 1px solid var(--halo-hairline);
  border-radius: 0.22rem;
  background: var(--halo-canvas);
}

.number-control input {
  width: 100%;
  min-width: 0;
  padding: 0.27rem 0.15rem 0.27rem 0.34rem;
  border: 0;
  outline: 0;
  color: var(--halo-text);
  background: transparent;
  font-family: var(--halo-font-mono);
  font-size: 0.56rem;
  -moz-appearance: textfield;
}

.number-control input::-webkit-inner-spin-button {
  opacity: 0.28;
}

.number-control > span {
  padding-right: 0.3rem;
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.46rem;
}

.number-control:focus-within {
  border-color: var(--halo-focus);
  box-shadow: 0 0 0 1px var(--halo-focus);
}

.scale {
  gap: 0.25rem;
  margin-top: 0.25rem;
  color: var(--halo-text-muted);
  font-family: var(--halo-font-mono);
  font-size: 0.4rem;
}

.scale i {
  height: 1px;
  flex: 1;
  background: repeating-linear-gradient(
    90deg,
    var(--halo-metal) 0 1px,
    transparent 1px 8px
  );
}

.direction-control {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.28rem;
}

.direction-control button {
  padding: 0.34rem 0.2rem;
  border: 1px solid var(--halo-hairline);
  border-radius: 0.24rem;
  color: var(--halo-text-muted);
  background: var(--halo-glass-sheen);
  font-family: var(--halo-font-mono);
  font-size: 0.46rem;
  cursor: pointer;
}

.direction-control button[aria-pressed="true"] {
  border-color: var(--halo-running);
  color: var(--halo-running);
  background: var(--halo-running-glow);
}

.direction-control button:focus-visible {
  outline: 2px solid var(--halo-focus);
  outline-offset: 1px;
}

.editor-hint,
.validation-error {
  min-height: 1.2rem;
  margin: 0.55rem 0 0;
  line-height: 1.45;
}

.validation-error {
  color: var(--halo-failed);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

fieldset:disabled input,
fieldset:disabled button {
  cursor: not-allowed;
}

@media (max-width: 1380px) {
  .effect-editor {
    top: 0.7rem;
    left: 0.7rem;
  }
}

@media (max-width: 700px) {
  .effect-editor {
    top: 0.5rem;
    left: 0.5rem;
    width: min(13.5rem, calc(100% - 1rem));
    max-height: calc(100% - 1rem);
    overflow: auto;
  }
}

@media (forced-colors: active) {
  .effect-editor,
  .number-control,
  .direction-control button {
    border-color: ButtonText;
    background: Canvas;
  }
}
</style>
