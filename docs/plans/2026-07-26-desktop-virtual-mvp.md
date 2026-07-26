# Codex Halo 桌面虚拟设备 MVP 实施计划

> **实施要求：** 全程 TDD。官方脚手架和配置文件除外，任何产品行为都必须先写失败测试并确认 RED，再做最小实现并确认 GREEN。

**目标：** 在没有实体硬件的情况下，交付一个 Windows 优先、跨平台架构的 Tauri 2 桌面控制中心；它能读取脱敏 Codex Hook 事件，把最多四个任务自动或手动绑定到同心圆环，并实时模拟灯效、中央屏幕和表冠交互。

**架构：** `apps/desktop` 使用 Vue 3、TypeScript、Vite 和 Tauri 2。纯 Rust `domain` 层负责状态标准化、来源/可信度、任务状态机、灯效配置和四环绑定；Tauri 层负责命令、脱敏探针目录和事件广播；Vue 只渲染快照并发送用户意图。真实 Hook 和内置模拟器共用同一领域 API。

**技术栈：** Tauri 2、Rust stable MSVC、Vue 3、TypeScript、Vite、Vitest、Vue Test Utils、Serde、CSS/SVG 动画。

**视觉方向：** 精密工业仪器。烟黑玻璃、金属刻度、暖黄色运行光、低饱和青蓝连接提示；中心虚拟设备是主视觉。字体本地打包 IBM Plex Sans Variable 与 JetBrains Mono Variable，不依赖网络字体。

**MVP 明确不做：** USB、固件、SQLite、系统托盘、开机启动、实体设备检测、失败状态的真实 Hook 推断。上述能力等虚拟设备和实时状态链路稳定后再实施。

**隐私边界：**

- 只读取现有探针产生的脱敏 JSON。
- 任务主键使用 16 位 `session_id` 指纹，不保存原始 ID。
- 不读取或保存提示词、回复、代码、工具参数、命令、环境变量或认证信息。
- 没有安全标题来源时只显示 `Codex · XXXX` 匿名短名。
- 持久化与诊断不得包含具体任务指纹；本 MVP 不做配置持久化。
- 硬件采购继续冻结。

---

## Task 0：安装工具链并生成官方脚手架

**文件：**

- Create: `apps/desktop/**`

**Step 1：检查先决条件**

```powershell
node --version
npm --version
winget --version
```

使用 `vswhere` 验证 `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`，并检查
`%USERPROFILE%\.cargo\bin\rustup.exe`。当前已知 Node/npm 和 C++ Build Tools
存在，Rust 缺失。

**Step 2：安装 Rust**

```powershell
winget install --id Rustlang.Rustup -e `
  --accept-package-agreements --accept-source-agreements
```

刷新当前终端用户环境，然后：

```powershell
rustup default stable-msvc
rustc --version
cargo --version
```

预期：host 为本机对应的 `*-pc-windows-msvc`。

**Step 3：生成 Tauri 2 + Vue TypeScript**

```powershell
New-Item -ItemType Directory -Path apps -Force
Push-Location apps
npm create tauri-app@latest desktop -- `
  --template vue-ts --manager npm --yes `
  --tauri-version 2 --identifier com.hur26.codexhalo
Pop-Location
Set-Location apps/desktop
npm install
npm install -D vitest @vue/test-utils jsdom
npm install @fontsource-variable/ibm-plex-sans `
  @fontsource-variable/jetbrains-mono
```

在 `package.json` 增加 `test`、`test:watch`、`typecheck` scripts。

**Step 4：验证脚手架**

```powershell
npm run build
npm run typecheck
cargo check --manifest-path src-tauri/Cargo.toml
```

脚手架属于生成代码例外，不为它伪造产品行为测试。

**Step 5：提交**

```powershell
git add apps/desktop
git commit -m "构建：初始化 Tauri 桌面控制中心"
```

---

## Task 1：定义状态、来源与可信度模型

**文件：**

- Create: `apps/desktop/src-tauri/src/domain/mod.rs`
- Create: `apps/desktop/src-tauri/src/domain/model.rs`
- Create: `apps/desktop/src-tauri/src/domain/normalize.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Step 1：写失败测试**

分别覆盖：

- `UserPromptSubmit`、`PreToolUse`、`PostToolUse` → `Running`。
- `PermissionRequest` → `Waiting`，但 `confidence=Provisional`。
- `Stop` → `RoundCompleted`，不能叫任务永久完成。
- `Failed` 只允许 `source=Simulator`，真实 Hook 不推断失败。
- 非 16 位小写十六进制任务键被拒绝。

精确模型：

```rust
pub enum TaskStatus {
    Running,
    Waiting,
    RoundCompleted,
    Failed,
    Queued,
    Idle,
    Unknown,
}

pub enum SignalSource { Hook, Simulator }
pub enum Confidence { Observed, Provisional, Simulated }

pub struct NormalizedState {
    pub status: TaskStatus,
    pub source: SignalSource,
    pub confidence: Confidence,
}
```

`TaskRecord` 和 `RingSlot` 必须保留 `source` 与 `confidence`。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain::normalize
```

**Step 3：最小实现并确认 GREEN**

所有前端结构派生 Serde、`Clone`、`Debug`、`PartialEq`；枚举序列化为
`camelCase`。

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain::normalize
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
```

**Step 4：提交**

```powershell
git add apps/desktop/src-tauri/src
git commit -m "功能：建立可信任务状态模型"
```

---

## Task 2：实现四环混合绑定引擎

**文件：**

- Create: `apps/desktop/src-tauri/src/domain/engine.rs`
- Modify: `apps/desktop/src-tauri/src/domain/mod.rs`
- Modify: `apps/desktop/src-tauri/src/domain/model.rs`

**Step 1：逐项 RED**

每个行为一个测试：

- 四个最近活动任务自动进入四个空槽。
- 第五个任务进入队列，不覆盖锁定槽。
- 手动绑定不会让同一任务重复占槽。
- 锁定槽不被自动分配覆盖。
- 交换槽位保留任务、绑定模式和锁定状态。
- `RoundCompleted` 自动槽保持 300000 ms 后释放。
- `RoundCompleted` 锁定槽不自动释放。
- 后续活动按相同任务键刷新，不创建新任务。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain::engine
```

**Step 3：最小 API**

```rust
impl HaloEngine {
    pub fn new(round_complete_hold_ms: u64) -> Self;
    pub fn apply_signal(&mut self, signal: TaskSignal);
    pub fn manual_bind(&mut self, task: &TaskKey, slot: usize, lock: bool)
        -> Result<(), EngineError>;
    pub fn toggle_lock(&mut self, slot: usize) -> Result<(), EngineError>;
    pub fn swap_slots(&mut self, left: usize, right: usize)
        -> Result<(), EngineError>;
    pub fn tick(&mut self, now_ms: u64);
    pub fn snapshot(&self) -> HaloSnapshot;
}
```

所有时间由调用方传入，测试不得真实 sleep。

**Step 4：确认 GREEN 与提交**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
git add apps/desktop/src-tauri/src/domain
git commit -m "功能：实现四环混合绑定引擎"
```

---

## Task 3：实现可工作的灯效配置模型

**文件：**

- Create: `apps/desktop/src-tauri/src/domain/effects.rs`
- Modify: `apps/desktop/src-tauri/src/domain/model.rs`
- Modify: `apps/desktop/src-tauri/src/domain/engine.rs`

**Step 1：逐项 RED**

- 全局亮度只接受 0～100。
- 速度只接受 25～300，表示 0.25×～3.00×。
- 光尾长度只接受 1～100。
- 方向只接受 clockwise/counterClockwise。
- 更新某圈灯效后快照返回新值，其他圈不变。
- 状态默认颜色由固定安全预设提供，不接受任意未验证字符串。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain::effects
```

**Step 3：最小模型**

```rust
pub struct EffectProfile {
    pub brightness: u8,
    pub speed_percent: u16,
    pub direction: Direction,
    pub tail_percent: u8,
}
```

引擎增加 `update_effect(slot, EffectProfile)` 和
`set_global_brightness(value)`。

**Step 4：确认 GREEN 与提交**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
git add apps/desktop/src-tauri/src/domain
git commit -m "功能：实现可编辑灯效参数"
```

---

## Task 4：提供 Tauri 虚拟设备命令

**文件：**

- Create: `apps/desktop/src-tauri/src/app_state.rs`
- Create: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Step 1：逐项 RED**

- 初始快照有四个空槽，设备模式为 `virtual`。
- 注入模拟信号后返回相同任务的新状态。
- 模拟失败状态带 `source=Simulator/confidence=Simulated`。
- 绑定、锁定、交换、灯效更新均返回原子快照。
- 越界槽位和无效参数返回结构化错误，不 panic。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands
```

**Step 3：命令契约**

```text
get_snapshot
simulate_signal
manual_bind
toggle_lock
swap_slots
update_effect
set_global_brightness
reset_virtual_device
```

`AppState` 使用 `Mutex<HaloEngine>`；锁中不做文件 I/O 或异步等待。

**Step 4：确认 GREEN 与提交**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
git add apps/desktop/src-tauri/src
git commit -m "功能：加入虚拟设备命令桥接"
```

---

## Task 5：建立 Vue Bridge、实时订阅与 Store

**文件：**

- Create: `apps/desktop/src/types/halo.ts`
- Create: `apps/desktop/src/services/haloBridge.ts`
- Create: `apps/desktop/src/stores/haloStore.ts`
- Create: `apps/desktop/src/stores/haloStore.test.ts`
- Modify: `apps/desktop/vite.config.ts`

**Step 1：逐项 RED**

- `load()` 获取快照并结束 loading。
- 命令成功时原子替换快照，失败时保留旧快照并记录错误。
- `start()` 订阅 `halo://snapshot`，收到事件后更新快照。
- 重复 `start()` 不创建重复监听器，`stop()` 调用 unlisten。
- `getAdapterStatus()` 显示 online/degraded/offline。
- 非 Tauri 浏览器环境使用明确的演示 bridge。

接口必须包含：

```ts
export interface HaloBridge {
  getSnapshot(): Promise<HaloSnapshot>
  subscribeSnapshots(listener: (snapshot: HaloSnapshot) => void):
    Promise<() => void>
  getAdapterStatus(): Promise<AdapterStatus>
  simulateSignal(input: SimulateSignalInput): Promise<HaloSnapshot>
  manualBind(input: ManualBindInput): Promise<HaloSnapshot>
  toggleLock(slot: number): Promise<HaloSnapshot>
  swapSlots(left: number, right: number): Promise<HaloSnapshot>
  updateEffect(input: UpdateEffectInput): Promise<HaloSnapshot>
  setGlobalBrightness(value: number): Promise<HaloSnapshot>
}
```

**Step 2：RED/GREEN**

```powershell
Set-Location apps/desktop
npm test -- haloStore.test.ts
npm run typecheck
```

实现使用 Tauri `invoke` 与 `listen`；Store 使用 Vue 原生
`reactive/computed`，不增加状态库。

**Step 3：提交**

```powershell
git add apps/desktop/src apps/desktop/vite.config.ts
git commit -m "功能：建立实时状态 Bridge 与 Store"
```

---

## Task 6：实现四环虚拟设备主视觉

**文件：**

- Create: `apps/desktop/src/components/HaloPreview.vue`
- Create: `apps/desktop/src/components/HaloPreview.test.ts`
- Create: `apps/desktop/src/styles/tokens.css`
- Create: `apps/desktop/src/styles/base.css`
- Modify: `apps/desktop/src/main.ts`

**Step 1：逐项 RED**

- 永远渲染四个由内到外编号的圆环。
- 每圈暴露状态、来源、可信度和选中态。
- running 黄追逐、waiting 橙呼吸、roundCompleted 绿提示、
  simulated failed 红脉冲、queued 紫慢转、unknown 蓝慢闪。
- provisional 状态有可见但克制的候选标记。
- 点击圆环触发 `select(slot)`。
- `prefers-reduced-motion` 下动画停止但颜色保留。

**Step 2：RED/GREEN**

```powershell
npm test -- HaloPreview.test.ts
npm run typecheck
npm run build
```

使用 CSS mask/conic-gradient 或 SVG，不使用位图灯环；颜色、尺寸和动画都来自
CSS variables。

**Step 3：提交**

```powershell
git add apps/desktop/src
git commit -m "界面：实现四环虚拟设备预览"
```

---

## Task 7：实现中央屏幕与表冠

**文件：**

- Create: `apps/desktop/src/components/CentralDisplay.vue`
- Create: `apps/desktop/src/components/CentralDisplay.test.ts`
- Create: `apps/desktop/src/components/CrownControl.vue`
- Create: `apps/desktop/src/components/CrownControl.test.ts`

**Step 1：逐项 RED**

- 屏幕支持 ambient/overview/detail。
- overview 显示四圈匿名名与状态。
- detail 显示选中圈、来源、可信度和本轮状态。
- 短按表冠切换模式；左右旋转选择圈位；长按返回 ambient。
- 表冠固定在设备约 4 点钟方向。

**Step 2：RED/GREEN**

```powershell
npm test -- CentralDisplay.test.ts CrownControl.test.ts
npm run typecheck
```

中央图形使用原创 Halo 节点图形，不嵌入品牌图片。

**Step 3：提交**

```powershell
git add apps/desktop/src/components
git commit -m "界面：实现中央屏幕与表冠交互"
```

---

## Task 8：实现控制中心布局和只读任务轨

**文件：**

- Create: `apps/desktop/src/components/TaskRail.vue`
- Create: `apps/desktop/src/components/TaskRail.test.ts`
- Create: `apps/desktop/src/components/ActivityStrip.vue`
- Modify: `apps/desktop/src/App.vue`
- Modify: `apps/desktop/src/styles/base.css`

**Step 1：逐项 RED**

- 顶部明确显示 `VIRTUAL DEVICE` 和适配器状态。
- 左侧显示匿名任务、状态、来源/可信度和最近活动。
- 中间显示虚拟设备；底部显示最小活动轨迹。
- 第五个任务显示在等待队列。
- 1440×900 无裁切；小于 1180 px 时任务轨变为抽屉。

**Step 2：RED/GREEN**

```powershell
npm test -- TaskRail.test.ts
npm run typecheck
npm run build
```

**Step 3：提交**

```powershell
git add apps/desktop/src
git commit -m "界面：建立虚拟设备控制中心布局"
```

---

## Task 9：实现拖拽、键盘绑定、锁定与交换

**文件：**

- Create: `apps/desktop/src/components/BindingControls.vue`
- Create: `apps/desktop/src/components/BindingControls.test.ts`
- Modify: `apps/desktop/src/components/TaskRail.vue`
- Modify: `apps/desktop/src/components/HaloPreview.vue`
- Modify: `apps/desktop/src/App.vue`

**Step 1：逐项 RED**

- 任务拖到圆环调用 `manualBind`。
- 已绑定圆环拖到另一圈调用 `swapSlots`。
- 锁定按钮调用 `toggleLock`。
- 操作失败显示错误并保留旧快照。
- 键盘菜单能完成“绑定到第 N 圈”，不依赖鼠标拖拽。

**Step 2：RED/GREEN**

```powershell
npm test -- BindingControls.test.ts
npm run typecheck
```

**Step 3：提交**

```powershell
git add apps/desktop/src
git commit -m "界面：实现任务拖拽与锁定绑定"
```

---

## Task 10：实现真正可用的灯效编辑器

**文件：**

- Create: `apps/desktop/src/components/EffectEditor.vue`
- Create: `apps/desktop/src/components/EffectEditor.test.ts`
- Modify: `apps/desktop/src/App.vue`
- Modify: `apps/desktop/src/components/HaloPreview.vue`

**Step 1：逐项 RED**

- 全局亮度调用 `setGlobalBrightness`。
- 速度、方向、光尾调用 `updateEffect`。
- 越界输入在前端阻止且显示范围。
- 后端返回快照后，虚拟圆环立即反映速度、方向、亮度和光尾。
- 不提供尚未实现的颜色自由编辑控件。

**Step 2：RED/GREEN**

```powershell
npm test -- EffectEditor.test.ts
npm run typecheck
npm run build
```

**Step 3：提交**

```powershell
git add apps/desktop/src
git commit -m "界面：实现实时灯效参数编辑"
```

---

## Task 11：读取真实脱敏 Hook 目录

**文件：**

- Create: `apps/desktop/src-tauri/src/probe_adapter.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

**Step 1：逐项 RED**

- 路径优先级：测试显式 override → `CODEX_HALO_PROBE_DIR` → 跨平台 home 下
  `.codex-halo/probe`。
- 路径解析函数接收显式 env/home 参数，测试不修改进程全局环境。
- 只接受 schema 1、已知 Hook 和合法 16 位 `$.session_id` 指纹。
- 忽略 JSON 中额外的提示词、工具参数等字段。
- 同一文件只处理一次；每轮最多 128 个；按文件名排序。
- 缺少 session、损坏文件、目录缺失只增加安全诊断计数，不崩溃。
- 两个 session 进入两个任务，不串圈。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml probe_adapter
```

**Step 3：最小实现**

- 每 250 ms 读取新增文件，不修改或删除探针数据。
- 只把 session 指纹、turn 指纹、Hook 类型和接收时间交给引擎。
- 只保存最后处理文件名，不保存事件内容。
- 目录状态通过 `get_adapter_status` 暴露。
- 快照变化时由 Tauri 发 `halo://snapshot`。

**Step 4：GREEN 与提交**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
git add apps/desktop/src-tauri
git commit -m "功能：接入脱敏 Hook 实时事件"
```

---

## Task 12：验证真实 Hook 到 Vue 的实时订阅

**文件：**

- Create: `apps/desktop/src/services/liveIntegration.test.ts`
- Modify: `apps/desktop/src/stores/haloStore.ts`
- Modify: `apps/desktop/src/App.vue`

**Step 1：逐项 RED**

- App mount 调用 Store `load()` 和 `start()`。
- `halo://snapshot` 到达后正确圆环在一轮微任务内更新。
- App unmount 调用 `stop()`。
- adapter degraded/offline 时显示蓝色诊断，不覆盖锁定任务的身份。
- provisional waiting 和 roundCompleted 在 UI 中明确标注候选/本轮完成。

**Step 2：RED/GREEN**

```powershell
npm test -- liveIntegration.test.ts
npm test
npm run typecheck
```

**Step 3：提交**

```powershell
git add apps/desktop/src
git commit -m "功能：打通 Hook 到四环实时订阅"
```

---

## Task 13：端到端验证与 Windows 开发包

**文件：**

- Create: `docs/research/2026-07-26-desktop-mvp-verification.md`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/README.md`

**Step 1：自动验证**

```powershell
& 'C:\Users\BaiYang\AppData\Local\Programs\Python\Python314\python.exe' `
  -m unittest discover -s tests -p 'test_*.py'
Set-Location apps/desktop
npm test
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

**Step 2：桌面验证**

```powershell
npm run tauri dev
```

必须验证：

- 无硬件时完整显示 `VIRTUAL DEVICE`，不报 USB 错误。
- 四圈动画和 reduced-motion 正常。
- 五任务、拖拽、键盘绑定、锁定、交换、灯效编辑正常。
- 两个真实 Codex 任务在 1 秒内更新正确圆环。
- waiting 明确显示 provisional；Stop 明确显示“本轮完成”。

**Step 3：构建**

```powershell
npm run tauri build
```

MSI 若只因 VBSCRIPT 失败，先交付 NSIS 或开发可执行文件，不在 MVP 阶段强制修改
系统可选功能。

**Step 4：记录证据**

记录工具链版本、测试数量、Hook 延迟、未验证边界、产物绝对路径与 SHA-256。

**Step 5：提交**

```powershell
git add apps/desktop docs/research/2026-07-26-desktop-mvp-verification.md
git commit -m "验证：完成桌面虚拟设备 MVP"
```

---

## 完成门禁

- Rust 状态机、绑定和灯效测试全部通过。
- Vue 组件、交互、订阅、类型检查和生产构建通过。
- 两个真实 Codex 任务不串圈；第五任务进入队列。
- 锁定槽不被覆盖；本轮完成不被宣称为任务永久完成。
- provisional/simulated 状态在数据和 UI 中可区分。
- 任何提交文件都不含具体任务指纹、提示词、代码、原始 Hook JSON 或认证信息。
- 无硬件时完整可用，且明确标记为虚拟设备。
- 硬件采购继续冻结，直到桌面 MVP 与实时适配器稳定。
