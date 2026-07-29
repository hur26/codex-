# Codex Halo USB 设备基础实施计划

> **实施要求：** 全程使用 TDD。先写失败测试并确认 RED，再写最小实现并确认 GREEN。每个任务在本任务测试与相关回归通过后单独提交。

**目标：** 在没有实体硬件的情况下，完成 USB CDC v0.1 协议、Rust 设备管理器、协议级模拟设备、Windows 串口入口、桌面设备状态，以及可为 Waveshare ESP32-S3-Touch-AMOLED-1.43 编译的固件骨架。

**架构：** 现有 `HaloEngine` 继续作为任务绑定和呈现状态的唯一权威源。新增 Rust `device` 层，把脱敏的 `HaloSnapshot` 投影为有限的设备消息；协议级模拟器与真实 CDC 串口实现相同的 `DeviceTransport`。ESP32-S3 只保存高层呈现状态、播放本地动画和上报旋钮事件，不接收任务原始身份或内容。

**技术栈：** Rust stable、Tauri 2、`serialport` 4.9、Vue 3、TypeScript、Vitest、PlatformIO、Arduino framework、C++17、Unity。

**设计依据：** `docs/plans/2026-07-29-usb-device-foundation-design.md`

---

## 全局协议常量

实现期间不得临时改变以下 v0.1 常量；如确需改变，必须先修改协议文档和两端黄金向量。

```text
Magic                  0x43 0x48 ("CH")
Protocol major         0x01
Header bytes           8
CRC bytes              2
Maximum payload        512
Integer byte order     little-endian
CRC                    CRC-16/CCITT-FALSE, polynomial 0x1021, init 0xFFFF
CRC coverage           从 protocolMajor 到 payload 末尾，不含 magic 和 CRC
Retry limit            2
ACK timeout            250 ms
Heartbeat interval     1000 ms
Device watchdog        3000 ms
Maximum label          32 UTF-8 bytes；v0.1 默认发送空标签
```

消息类型：

```text
0x01 HELLO
0x02 CAPABILITIES
0x10 FULL_SNAPSHOT
0x11 RING_UPDATE
0x12 DISPLAY_MODE
0x13 BRIGHTNESS
0x20 HEARTBEAT
0x70 ACK
0x71 NACK
0x80 KNOB_EVENT
0x81 DIAGNOSTICS
```

## Task 0：记录基线并确认工具边界

**文件：**

- 不修改产品文件

**Step 1：执行现有回归**

```powershell
Set-Location D:\Project\codex-halo
python -m unittest discover -s tests -p 'test_*.py'
npm test --prefix apps/desktop
npm run typecheck --prefix apps/desktop
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

预期：全部通过。记录当时的 Python、Vitest 和 Rust 测试数量，后续只能增加或有明确原因调整。

**Step 2：确认 PlatformIO 状态**

```powershell
python -m platformio --version
```

如果模块不存在，在进入固件任务前执行：

```powershell
python -m pip install --user platformio
python -m platformio --version
```

预期：能输出 PlatformIO Core 版本。不在本任务创建固件工程。

## Task 1：冻结协议规范、黄金向量和 Rust 帧编解码器

**文件：**

- Create: `docs/protocol/codex-halo-usb-v0.1.md`
- Create: `docs/protocol/golden-vectors.tsv`
- Create: `apps/desktop/src-tauri/src/device/mod.rs`
- Create: `apps/desktop/src-tauri/src/device/protocol.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Step 1：先写 Rust 失败测试**

在 `protocol.rs` 的测试模块中先加入：

```rust
#[test]
fn encodes_the_hello_golden_vector() {
    let frame = Frame::new(MessageType::Hello, 1, vec![0]);
    assert_eq!(
        hex(&encode(&frame)),
        "4348010101000100006e91"
    );
}

#[test]
fn decodes_fragmented_frames_and_resynchronizes_after_noise() {
    let valid = decode_hex("43480120020000006cae");
    let mut decoder = Decoder::default();
    assert!(decoder.push(&[0xff, 0x00, valid[0]]).is_empty());
    let frames = decoder.push(&valid[1..]);
    assert_eq!(
        frames,
        vec![Ok(Frame::new(MessageType::Heartbeat, 2, vec![]))]
    );
}

#[test]
fn rejects_crc_errors_and_oversized_lengths_without_allocating_the_claimed_size() {
    let mut corrupt = decode_hex("4348010101000100006e91");
    *corrupt.last_mut().unwrap() ^= 0xff;
    let mut decoder = Decoder::default();
    assert_eq!(decoder.push(&corrupt), vec![Err(ProtocolError::CrcMismatch)]);

    let oversized = [0x43, 0x48, 0x01, 0x01, 0x01, 0x00, 0x01, 0x02];
    assert_eq!(
        decoder.push(&oversized),
        vec![Err(ProtocolError::PayloadTooLarge { actual: 513 })]
    );
}
```

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::protocol -- --nocapture
```

预期：因 `device::protocol`、`Frame`、`Decoder` 等尚不存在而编译失败。

**Step 3：写最小编解码器**

必须提供以下稳定接口：

```rust
pub const MAGIC: [u8; 2] = *b"CH";
pub const PROTOCOL_MAJOR: u8 = 1;
pub const MAX_PAYLOAD: usize = 512;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageType {
    Hello = 0x01,
    Capabilities = 0x02,
    FullSnapshot = 0x10,
    RingUpdate = 0x11,
    DisplayMode = 0x12,
    Brightness = 0x13,
    Heartbeat = 0x20,
    Ack = 0x70,
    Nack = 0x71,
    KnobEvent = 0x80,
    Diagnostics = 0x81,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub message_type: MessageType,
    pub sequence: u16,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: MessageType, sequence: u16, payload: Vec<u8>) -> Self;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    UnsupportedVersion { actual: u8 },
    UnknownMessageType { actual: u8 },
    PayloadTooLarge { actual: usize },
    CrcMismatch,
}

pub fn encode(frame: &Frame) -> Result<Vec<u8>, ProtocolError>;
pub fn crc16_ccitt_false(bytes: &[u8]) -> u16;

#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<Frame, ProtocolError>>;
}
```

解析器的恢复规则：

1. 丢弃魔数前噪声；
2. 只有收齐 8 字节头部后才读取长度；
3. 长度大于 512 时返回错误并丢弃当前魔数；
4. 未收齐整帧时保留缓冲区；
5. CRC 错误时返回一个错误并继续寻找下一个魔数；
6. 未知类型和主版本不匹配均返回结构化错误；
7. 不依据负载声明预分配超过 `MAX_PAYLOAD` 的内存。

**Step 4：写协议文档和黄金向量**

`golden-vectors.tsv` 使用 UTF-8、LF、制表符分隔：

```text
name	message_type	sequence	payload_hex	frame_hex
hello	01	0001	00	4348010101000100006e91
heartbeat	20	0002		43480120020000006cae
ack_hello	70	0001	01	4348017001000100017381
brightness_80	13	1234	50	4348011334120100502983
```

协议文档必须逐字节说明头部、CRC 覆盖范围、所有消息载荷、状态枚举、上限和版本兼容规则。文档中的示例必须与 TSV 一致。

**Step 5：确认 GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::protocol -- --nocapture
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
```

预期：协议测试通过，Clippy 无警告。

**Step 6：提交**

```powershell
git add docs/protocol apps/desktop/src-tauri/src/device apps/desktop/src-tauri/src/lib.rs
git commit -m "协议：实现 USB v0.1 帧编解码"
```

## Task 2：把中央屏和选中圆环纳入领域权威状态

**文件：**

- Modify: `apps/desktop/src-tauri/src/domain/model.rs`
- Modify: `apps/desktop/src-tauri/src/domain/engine.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`

**Step 1：写失败测试**

在 `engine.rs` 测试模块加入：

```rust
#[test]
fn presentation_intents_cycle_modes_and_wrap_ring_selection() {
    let mut engine = HaloEngine::new(300_000);
    assert_eq!(engine.snapshot().display_mode, DisplayMode::Ambient);
    assert_eq!(engine.snapshot().selected_slot, None);

    engine.apply_presentation_intent(PresentationIntent::Rotate(1));
    assert_eq!(engine.snapshot().selected_slot, Some(0));
    engine.apply_presentation_intent(PresentationIntent::Rotate(-1));
    assert_eq!(engine.snapshot().selected_slot, Some(3));

    engine.apply_presentation_intent(PresentationIntent::ShortPress);
    assert_eq!(engine.snapshot().display_mode, DisplayMode::Overview);
    engine.apply_presentation_intent(PresentationIntent::ShortPress);
    assert_eq!(engine.snapshot().display_mode, DisplayMode::Detail);
    engine.apply_presentation_intent(PresentationIntent::LongPress);
    assert_eq!(engine.snapshot().display_mode, DisplayMode::Ambient);
}

#[test]
fn invalid_selected_slot_is_rejected_without_advancing_revision() {
    let mut engine = HaloEngine::new(300_000);
    let before = engine.snapshot();
    assert_eq!(
        engine.set_presentation(DisplayMode::Detail, Some(4)),
        Err(EngineError::SlotOutOfBounds { slot: 4 })
    );
    assert_eq!(engine.snapshot(), before);
}
```

在 `commands.rs` 增加线边界测试，确认：

```rust
serde_json::json!({"displayMode": "overview", "selectedSlot": 2})
```

能够进入领域层，而未知模式、负数和 `selectedSlot: 4` 返回稳定结构化错误。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml presentation -- --nocapture
```

预期：`DisplayMode`、`PresentationIntent` 和命令不存在。

**Step 3：最小实现**

在领域模型中增加：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayMode {
    Ambient,
    Overview,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationIntent {
    Rotate(i8),
    ShortPress,
    LongPress,
}
```

`HaloSnapshot` 增加：

```rust
pub display_mode: DisplayMode,
pub selected_slot: Option<usize>,
```

`HaloEngine` 增加同名字段，并实现：

```rust
pub fn set_presentation(
    &mut self,
    display_mode: DisplayMode,
    selected_slot: Option<usize>,
) -> Result<(), EngineError>;

pub fn apply_presentation_intent(&mut self, intent: PresentationIntent);
```

约束：

- 旋转从 `None` 开始时，正向选中 0，负向选中 3；
- 之后在 0..3 环绕；
- 短按按 `Ambient → Overview → Detail → Ambient` 循环；
- 长按回到 `Ambient`；
- 只有实际变化才增加 revision；
- reset 恢复 `Ambient + None`。

新增 Tauri 命令：

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetPresentationInput {
    pub display_mode: String,
    pub selected_slot: Option<usize>,
}

#[tauri::command]
pub fn set_presentation(...) -> Result<HaloSnapshot, CommandError>;
```

**Step 4：确认 GREEN 和领域回归**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
```

**Step 5：提交**

```powershell
git add apps/desktop/src-tauri/src/domain apps/desktop/src-tauri/src/commands.rs
git commit -m "领域：统一屏幕模式与圆环选择状态"
```

## Task 3：实现脱敏设备快照投影和消息载荷

**文件：**

- Create: `apps/desktop/src-tauri/src/device/presentation.rs`
- Modify: `apps/desktop/src-tauri/src/device/mod.rs`

**Step 1：写失败测试**

```rust
#[test]
fn projection_contains_four_ring_states_but_no_task_identity() {
    let snapshot = populated_halo_snapshot();
    let projected = DeviceSnapshot::from_halo(&snapshot);
    let payload = projected.encode_payload().unwrap();

    assert_eq!(projected.rings.len(), 4);
    assert_eq!(projected.revision, snapshot.revision);
    assert_eq!(projected.display_mode, DeviceDisplayMode::Detail);
    assert!(!String::from_utf8_lossy(&payload).contains("0123456789abcdef"));
    assert!(projected.rings.iter().all(|ring| ring.label.is_empty()));
}

#[test]
fn delta_sends_only_changed_ring_or_global_fields() {
    let before = DeviceSnapshot::from_halo(&base_snapshot());
    let mut changed = base_snapshot();
    changed.slots[2].status = TaskStatus::Waiting;
    let after = DeviceSnapshot::from_halo(&changed);

    assert_eq!(
        after.diff(&before),
        vec![DeviceUpdate::Ring(after.rings[2].clone())]
    );
}
```

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::presentation -- --nocapture
```

**Step 3：实现固定设备模型**

接口必须为：

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub revision: u64,
    pub global_brightness: u8,
    pub display_mode: DeviceDisplayMode,
    pub selected_ring: Option<u8>,
    pub rings: [DeviceRing; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRing {
    pub index: u8,
    pub status: DeviceTaskStatus,
    pub brightness: u8,
    pub speed_percent: u16,
    pub direction: DeviceDirection,
    pub tail_percent: u8,
    pub label: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceUpdate {
    Ring(DeviceRing),
    Display { mode: DeviceDisplayMode, selected_ring: Option<u8> },
    Brightness(u8),
}
```

载荷布局必须与协议文档一致：

```text
FULL_SNAPSHOT:
revision:u64, globalBrightness:u8, displayMode:u8, selectedRing:u8,
ringCount:u8, ring[ringCount]

RING:
index:u8, status:u8, brightness:u8, speedPercent:u16,
direction:u8, tailPercent:u8, labelLength:u8, label:bytes
```

状态枚举固定为：

```text
running=1, waiting=2, roundCompleted=3, failed=4,
queued=5, idle=6, unknown=7
```

标签先保留协议字段但写空值，禁止从 `TaskKey` 生成或截取标签。

**Step 4：确认 GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::presentation -- --nocapture
```

**Step 5：提交**

```powershell
git add apps/desktop/src-tauri/src/device
git commit -m "设备：实现脱敏快照与增量投影"
```

## Task 4：定义传输接口并实现协议级模拟设备

**文件：**

- Create: `apps/desktop/src-tauri/src/device/transport.rs`
- Create: `apps/desktop/src-tauri/src/device/simulator.rs`
- Modify: `apps/desktop/src-tauri/src/device/mod.rs`

**Step 1：写失败测试**

```rust
#[test]
fn simulator_handshakes_acks_state_and_records_the_snapshot() {
    let mut simulator = SimulatedTransport::default();
    simulator.connect(&Endpoint::virtual_device()).unwrap();

    simulator.write(&encode(&Frame::new(MessageType::Hello, 1, vec![0])).unwrap()).unwrap();
    let frames = decode_all(&simulator.read().unwrap());
    assert_eq!(frames[0].message_type, MessageType::Capabilities);

    let snapshot = fixture_device_snapshot();
    simulator.write(
        &encode(&Frame::new(MessageType::FullSnapshot, 2, snapshot.encode_payload().unwrap())).unwrap()
    ).unwrap();
    let frames = decode_all(&simulator.read().unwrap());
    assert_eq!(frames[0], Frame::new(MessageType::Ack, 2, vec![MessageType::FullSnapshot as u8]));
    assert_eq!(simulator.applied_snapshot(), Some(&snapshot));
}

#[test]
fn simulator_can_inject_timeout_crc_nack_disconnect_and_knob_events() {
    let mut simulator = SimulatedTransport::default();
    simulator.script(Fault::TimeoutOnce);
    simulator.script(Fault::NackOnce(NackReason::Busy));
    simulator.script(Fault::CorruptCrcOnce);
    simulator.inject_knob(KnobEvent::Rotate(-1));
    simulator.disconnect().unwrap();

    assert_eq!(simulator.pending_fault_count(), 3);
    assert!(!simulator.is_connected());
}
```

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::simulator -- --nocapture
```

**Step 3：实现接口**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub id: String,
    pub label: String,
}

pub trait DeviceTransport: Send {
    fn kind(&self) -> TransportKind;
    fn discover(&mut self) -> Result<Vec<Endpoint>, TransportError>;
    fn connect(&mut self, endpoint: &Endpoint) -> Result<(), TransportError>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError>;
    fn read(&mut self) -> Result<Vec<u8>, TransportError>;
    fn disconnect(&mut self) -> Result<(), TransportError>;
    fn is_connected(&self) -> bool;
}
```

`SimulatedTransport` 在收到字节后必须经过真实 `Decoder`，再生成真实编码响应，不能绕过协议直接修改测试状态。故障注入按 FIFO、一次性消费。

能力载荷固定为：

```text
protocolMinor:u8
firmwareMajor:u8
firmwareMinor:u8
firmwarePatch:u8
ringCount:u8
featureFlags:u16
maxPayload:u16
```

v0.1 模拟器返回 `0, 0, 1, 0, 4, 0x0003, 512`；feature bit 0 表示 AMOLED，bit 1 表示旋钮。

**Step 4：确认 GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::simulator -- --nocapture
```

**Step 5：提交**

```powershell
git add apps/desktop/src-tauri/src/device
git commit -m "设备：加入协议级确定性模拟器"
```

## Task 5：实现设备连接、确认、重试和重连状态机

**文件：**

- Create: `apps/desktop/src-tauri/src/device/manager.rs`
- Modify: `apps/desktop/src-tauri/src/device/mod.rs`

**Step 1：写失败测试**

使用显式 `now_ms`，禁止测试休眠：

```rust
#[test]
fn manager_handshakes_then_sends_an_authoritative_full_snapshot() {
    let transport = SimulatedTransport::default();
    let mut manager = DeviceManager::new(transport);

    manager.step(0, &fixture_halo_snapshot());
    manager.step(1, &fixture_halo_snapshot());

    assert_eq!(manager.status().state, DeviceConnectionState::Virtual);
    assert_eq!(
        manager.transport().applied_snapshot(),
        Some(&DeviceSnapshot::from_halo(&fixture_halo_snapshot()))
    );
}

#[test]
fn manager_retries_twice_then_reconnects_with_a_full_snapshot() {
    let mut transport = SimulatedTransport::default();
    transport.script(Fault::TimeoutOnce);
    transport.script(Fault::TimeoutOnce);
    transport.script(Fault::TimeoutOnce);
    let mut manager = DeviceManager::new(transport);

    for now in [0, 250, 500, 750, 751] {
        manager.step(now, &fixture_halo_snapshot());
    }

    assert_eq!(manager.metrics().retry_count, 2);
    assert!(manager.metrics().reconnect_count >= 1);
    assert!(manager.transport().full_snapshot_count() >= 1);
}

#[test]
fn incompatible_major_never_receives_state_writes() {
    let mut transport = SimulatedTransport::default();
    transport.set_protocol_major(2);
    let mut manager = DeviceManager::new(transport);
    manager.step(0, &fixture_halo_snapshot());

    assert_eq!(manager.status().state, DeviceConnectionState::Incompatible);
    assert_eq!(manager.transport().state_write_count(), 0);
}

#[test]
fn old_knob_sequences_are_ignored_and_new_events_become_intents() {
    let mut manager = online_manager();
    manager.transport_mut().inject_knob_with_sequence(9, KnobEvent::ShortPress);
    manager.transport_mut().inject_knob_with_sequence(9, KnobEvent::Rotate(1));
    manager.transport_mut().inject_knob_with_sequence(10, KnobEvent::Rotate(-1));

    assert_eq!(
        manager.step(100, &fixture_halo_snapshot()).intents,
        vec![PresentationIntent::ShortPress, PresentationIntent::Rotate(-1)]
    );
}
```

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::manager -- --nocapture
```

**Step 3：实现纯同步状态机**

必须公开以下脱敏状态：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceConnectionState {
    Virtual,
    Connecting,
    Online,
    Incompatible,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub revision: u64,
    pub state: DeviceConnectionState,
    pub transport: TransportKind,
    pub message: Option<String>,
    pub firmware_version: Option<String>,
    pub retry_count: u32,
}

pub struct StepResult {
    pub status_changed: bool,
    pub intents: Vec<PresentationIntent>,
}

impl<T: DeviceTransport> DeviceManager<T> {
    pub fn step(&mut self, now_ms: u64, snapshot: &HaloSnapshot) -> StepResult;
}
```

状态机要求：

- 连接后依次 `HELLO → CAPABILITIES → FULL_SNAPSHOT → ONLINE/VIRTUAL`；
- 模拟传输的稳定态显示 `VIRTUAL`，真实串口显示 `ONLINE`；
- 状态写入等待同序号 ACK；
- 250 ms 超时最多重试 2 次；
- 第三次失败断开并重新发现；
- 每 1000 ms 发送一次心跳；
- 快照 revision 未变化时不重复写；
- revision 变化时只发送 `diff`；
- 每次新连接无条件发送完整快照；
- 主版本不兼容时进入 `INCOMPATIBLE` 且不发送状态；
- 诊断文本不得包含任务 key、帧完整载荷或串口序列号。

**Step 4：确认 GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::manager -- --nocapture
```

**Step 5：提交**

```powershell
git add apps/desktop/src-tauri/src/device
git commit -m "设备：实现握手重试与重连状态机"
```

## Task 6：实现 Windows 优先的真实 CDC 串口传输

**文件：**

- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Create: `apps/desktop/src-tauri/src/device/serial.rs`
- Modify: `apps/desktop/src-tauri/src/device/mod.rs`

**Step 1：写纯函数失败测试**

```rust
#[test]
fn discovery_prioritizes_espressif_usb_without_hard_coding_a_com_number() {
    let ports = vec![
        usb_port("COM9", 0x1234, 0x5678, Some("Other")),
        usb_port("COM12", 0x303a, 0x1001, Some("USB JTAG/serial debug unit")),
    ];

    let candidates = select_candidates(ports, None);
    assert_eq!(candidates.iter().map(|p| p.port_name.as_str()).collect::<Vec<_>>(), vec!["COM12"]);
}

#[test]
fn explicit_override_is_exact_but_still_requires_protocol_handshake() {
    let candidates = select_candidates(vec![usb_port("COM12", 0x303a, 1, None)], Some("COM77"));
    assert_eq!(candidates[0].port_name, "COM77");
    assert!(!candidates[0].protocol_verified);
}

#[test]
fn diagnostics_never_expose_usb_serial_numbers() {
    let port = usb_port_with_serial("COM12", 0x303a, 1, "sensitive-device-serial");
    assert!(!CandidatePort::from(port).diagnostic_label().contains("sensitive-device-serial"));
}
```

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::serial -- --nocapture
```

**Step 3：加入依赖与实现**

`Cargo.toml`：

```toml
serialport = "4.9.0"
```

`SerialTransport` 使用 `serialport::available_ports()` 和阻塞 `SerialPort`：

- 115200 baud、8-N-1、无流控；
- 读超时不大于 20 ms；
- 自动候选只接受 USB VID `0x303A`，或 manufacturer/product 含 `Espressif`、`ESP32`、`Waveshare`；
- 环境变量 `CODEX_HALO_SERIAL_PORT` 可给出精确端口名用于开发；
- 候选筛选只用于减少误开端口，最终身份仍由 `HELLO/CAPABILITIES` 决定；
- 不把 USB serial number 放入状态或日志；
- 连接失败返回结构化 `TransportError`，不 panic。

不要在前端或领域层引用 `serialport` 类型。

**Step 4：确认 GREEN 与跨平台编译边界**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::serial -- --nocapture
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
```

**Step 5：提交**

```powershell
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/src/device
git commit -m "设备：接入安全的 CDC 串口传输"
```

## Task 7：把设备工作线程接入 Tauri 生命周期

**文件：**

- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Step 1：写失败测试**

```rust
#[test]
fn default_device_status_is_virtual_and_serializable_without_identity() {
    let state = AppState::default();
    let status = get_device_status_inner(&state).unwrap();
    assert_eq!(status.state, DeviceConnectionState::Virtual);
    let json = serde_json::to_string(&status).unwrap();
    assert!(!json.contains("taskKey"));
    assert!(!json.contains("serialNumber"));
}

#[test]
fn a_knob_event_mutates_the_engine_then_the_next_step_echoes_the_new_snapshot() {
    let engine = Mutex::new(HaloEngine::new(300_000));
    apply_device_intents(&engine, vec![PresentationIntent::ShortPress]).unwrap();
    assert_eq!(engine.lock().unwrap().snapshot().display_mode, DisplayMode::Overview);
}
```

另加源码契约测试或可注入 worker 测试，确认退出同时停止 probe worker 和 device worker。

**Step 2：确认 RED**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml app_state::tests commands::tests -- --nocapture
```

**Step 3：实现生命周期**

`AppState` 新增：

```rust
pub(crate) device_status: Arc<Mutex<DeviceStatus>>,
device_worker_stop: Arc<AtomicBool>,
device_worker_handle: Mutex<Option<JoinHandle<()>>>,
```

新增：

```rust
pub fn start_device_worker<R: Runtime>(&self, app_handle: AppHandle<R>, mode: DeviceTransportMode);
pub fn stop_device_worker(&self);
```

运行规则：

- 默认 `DeviceTransportMode::Simulator`；
- `CODEX_HALO_DEVICE_TRANSPORT=serial` 时使用真实串口；
- worker 每 50 ms 执行一次 `manager.step`；
- 只在设备状态语义变化时发送 `halo://device-status`；
- 旋钮 intent 在持有 engine 锁时应用，释放锁后发送 `halo://snapshot`；
- 不同时持有 `engine` 和 `device_status` 两把锁；
- `RunEvent::Exit` 停止两个 worker；
- `Drop` 路径可重复调用且不会双重 join。

新增命令并注册：

```rust
#[tauri::command]
pub fn get_device_status(
    state: tauri::State<'_, AppState>,
) -> Result<DeviceStatus, CommandError>;
```

同时注册 Task 2 的 `set_presentation`。

**Step 4：确认 GREEN**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
```

**Step 5：提交**

```powershell
git add apps/desktop/src-tauri/src
git commit -m "桌面：接入设备工作线程与状态事件"
```

## Task 8：在可视化驱动中显示设备状态并同步屏幕选择

**文件：**

- Modify: `apps/desktop/src/types/halo.ts`
- Modify: `apps/desktop/src/services/haloBridge.ts`
- Modify: `apps/desktop/src/stores/haloStore.ts`
- Modify: `apps/desktop/src/stores/haloStore.test.ts`
- Create: `apps/desktop/src/components/DeviceStatus.vue`
- Create: `apps/desktop/src/components/DeviceStatus.test.ts`
- Modify: `apps/desktop/src/App.vue`
- Modify: `apps/desktop/src/App.test.ts`
- Modify: `apps/desktop/src/styles/base.css`

**Step 1：先写类型与组件失败测试**

新增前端类型：

```ts
export type DeviceConnectionState =
  | "virtual"
  | "connecting"
  | "online"
  | "incompatible"
  | "error";

export type DeviceTransportKind = "simulator" | "serial";

export interface DeviceStatus {
  revision: number;
  state: DeviceConnectionState;
  transport: DeviceTransportKind;
  message: string | null;
  firmwareVersion: string | null;
  retryCount: number;
}
```

`DeviceStatus.test.ts`：

```ts
it.each([
  ["virtual", "VIRTUAL"],
  ["connecting", "CONNECTING"],
  ["online", "ONLINE"],
  ["incompatible", "INCOMPATIBLE"],
  ["error", "ERROR"],
] as const)("显示可辨认的 %s 状态", (state, label) => {
  const wrapper = mount(DeviceStatusView, {
    props: { status: { ...BASE_STATUS, state } },
  });
  expect(wrapper.get("[data-device-status]").text()).toContain(label);
});

it("不渲染任务身份或 USB 序列号", () => {
  const wrapper = mount(DeviceStatusView, {
    props: { status: BASE_STATUS },
  });
  expect(wrapper.html()).not.toContain("taskKey");
  expect(wrapper.html()).not.toContain("serialNumber");
});
```

Store 测试先要求：

- `getDeviceStatus()` 与 `subscribeDeviceStatus()`；
- revision 旧值被忽略；
- `stop()` 清理三类订阅；
- 部分订阅失败时清理已经成功的订阅；
- `setPresentation()` 返回的新快照进入 store。

**Step 2：确认 RED**

```powershell
npm test --prefix apps/desktop -- DeviceStatus haloStore App
```

预期：缺少组件、类型和 bridge 方法。

**Step 3：实现 bridge 和 store**

`HaloBridge` 增加：

```ts
getDeviceStatus(): Promise<DeviceStatus>;
subscribeDeviceStatus(listener: (status: DeviceStatus) => void): Promise<() => void>;
setPresentation(input: {
  displayMode: DisplayMode;
  selectedSlot: number | null;
}): Promise<HaloSnapshot>;
```

Tauri 事件名固定为 `halo://device-status`。浏览器 demo bridge 返回 `VIRTUAL / simulator / firmwareVersion 0.1.0`。

Store 新增 `deviceStatus` 和 `refreshDeviceStatus()`，生命周期与现有 snapshot、adapter status 一起启动和停止。

**Step 4：让 UI 使用领域快照**

`HaloSnapshot` 增加：

```ts
displayMode: DisplayMode;
selectedSlot: number | null;
```

`App.vue` 不再把 `displayMode` 和 `selectedSlot` 作为只存在于组件内的权威状态。所有选择和模式变化通过 `store.setPresentation()`，渲染值来自最新快照。

注意：

- 拖拽中的临时视觉状态继续保留在前端；
- 点击任务、点击圆环和表冠都调用同一 presentation 命令；
- 命令失败时保持上一个快照，不做乐观覆盖；
- 顶部同时显示 `DEVICE` 和现有 `ADAPTER`，两者不能混为一个状态；
- `INCOMPATIBLE` 使用明确错误色，但不隐藏虚拟圆环或 Hook 状态。

**Step 5：确认 GREEN 与回归**

```powershell
npm test --prefix apps/desktop
npm run typecheck --prefix apps/desktop
npm run build --prefix apps/desktop
```

**Step 6：提交**

```powershell
git add apps/desktop/src
git commit -m "界面：显示设备状态并同步表冠交互"
```

## Task 9：创建 PlatformIO 固件工程和 C++ 协议黄金测试

**文件：**

- Create: `firmware/halo-esp32s3/platformio.ini`
- Create: `firmware/halo-esp32s3/lib/HaloProtocol/HaloProtocol.hpp`
- Create: `firmware/halo-esp32s3/lib/HaloProtocol/HaloProtocol.cpp`
- Create: `firmware/halo-esp32s3/test/test_protocol/test_main.cpp`
- Create: `firmware/halo-esp32s3/README.md`

**Step 1：创建最小配置**

`platformio.ini`：

```ini
[platformio]
default_envs = waveshare_amoled_143

[env]
test_framework = unity
build_unflags = -std=gnu++11
build_flags = -std=gnu++17

[env:native]
platform = native

[env:waveshare_amoled_143]
platform = espressif32
board = esp32-s3-devkitc-1
framework = arduino
board_upload.flash_size = 16MB
board_build.flash_size = 16MB
board_build.partitions = default_16MB.csv
board_build.arduino.memory_type = qio_opi
build_flags =
    ${env.build_flags}
    -DARDUINO_USB_MODE=1
    -DARDUINO_USB_CDC_ON_BOOT=1
    -DBOARD_HAS_PSRAM
```

首轮不引入屏幕或 LED 第三方库。

**Step 2：先写 C++ 失败测试**

测试从仓库唯一来源 `docs/protocol/golden-vectors.tsv` 读取十六进制向量：

```cpp
void test_hello_matches_shared_golden_vector() {
  const auto vectors = loadGoldenVectors("../../docs/protocol/golden-vectors.tsv");
  const halo::Frame hello{halo::MessageType::Hello, 1, {0}};
  TEST_ASSERT_EQUAL_STRING(
      vectors.at("hello").frameHex.c_str(),
      toHex(halo::encode(hello)).c_str());
}

void test_fragmented_frame_and_crc_error() {
  halo::Decoder decoder;
  const auto bytes = fromHex("4348010101000100006e91");
  TEST_ASSERT_TRUE(decoder.push(bytes.data(), 3).empty());
  const auto decoded = decoder.push(bytes.data() + 3, bytes.size() - 3);
  TEST_ASSERT_EQUAL(1, decoded.size());
  TEST_ASSERT_TRUE(decoded[0].ok());

  auto corrupt = bytes;
  corrupt.back() ^= 0xff;
  TEST_ASSERT_EQUAL(
      halo::ProtocolError::CrcMismatch,
      decoder.push(corrupt.data(), corrupt.size())[0].error);
}
```

**Step 3：确认 RED**

```powershell
Set-Location D:\Project\codex-halo\firmware\halo-esp32s3
python -m platformio test -e native
```

预期：`HaloProtocol` 尚不存在，编译失败。

**Step 4：实现与 Rust 同构的固定内存解析器**

接口：

```cpp
namespace halo {
constexpr std::array<uint8_t, 2> kMagic{0x43, 0x48};
constexpr uint8_t kProtocolMajor = 1;
constexpr size_t kMaxPayload = 512;

enum class MessageType : uint8_t { /* 使用全局表中的固定值 */ };
enum class ProtocolError : uint8_t {
  None,
  UnsupportedVersion,
  UnknownMessageType,
  PayloadTooLarge,
  CrcMismatch,
};

struct Frame {
  MessageType type;
  uint16_t sequence;
  std::vector<uint8_t> payload;
};

uint16_t crc16CcittFalse(const uint8_t* bytes, size_t length);
std::vector<uint8_t> encode(const Frame& frame);

class Decoder {
 public:
  std::vector<DecodeResult> push(const uint8_t* bytes, size_t length);
 private:
  std::array<uint8_t, kMaxPayload + 10> buffer_{};
  size_t used_{0};
};
}
```

ESP32 目标代码不允许依据帧内长度动态扩展超过 522 字节。native 测试辅助函数可以使用 STL。

**Step 5：确认 GREEN 和目标编译**

```powershell
python -m platformio test -e native
python -m platformio run -e waveshare_amoled_143
```

预期：native 测试通过，目标环境编译通过。

**Step 6：提交**

```powershell
Set-Location D:\Project\codex-halo
git add firmware/halo-esp32s3
git commit -m "固件：建立 ESP32-S3 协议测试骨架"
```

## Task 10：实现固件设备状态机和空硬件适配器

**文件：**

- Create: `firmware/halo-esp32s3/lib/HaloCore/HaloState.hpp`
- Create: `firmware/halo-esp32s3/lib/HaloCore/HaloState.cpp`
- Create: `firmware/halo-esp32s3/lib/HaloHal/HaloHal.hpp`
- Create: `firmware/halo-esp32s3/src/main.cpp`
- Create: `firmware/halo-esp32s3/test/test_state/test_main.cpp`

**Step 1：写失败测试**

```cpp
void test_full_snapshot_becomes_authoritative_state() {
  halo::DeviceController controller;
  const auto frame = fixtureFullSnapshotFrame();
  const auto response = controller.handle(frame, 100);

  TEST_ASSERT_EQUAL(halo::MessageType::Ack, response.type);
  TEST_ASSERT_EQUAL_UINT64(42, controller.state().revision);
  TEST_ASSERT_EQUAL(4, controller.state().ringCount);
}

void test_watchdog_enters_low_brightness_disconnected_state() {
  halo::DeviceController controller;
  controller.handle(fixtureFullSnapshotFrame(), 0);
  controller.tick(3001);

  TEST_ASSERT_TRUE(controller.state().disconnected);
  TEST_ASSERT_LESS_OR_EQUAL(16, controller.state().effectiveBrightness);
}

void test_local_current_limit_caps_effective_brightness() {
  halo::PowerPolicy policy{500, 80};
  TEST_ASSERT_LESS_OR_EQUAL(80, policy.limitBrightness(100, 20));
}
```

**Step 2：确认 RED**

```powershell
Set-Location D:\Project\codex-halo\firmware\halo-esp32s3
python -m platformio test -e native
```

**Step 3：实现固件核心与 HAL**

HAL 接口固定为：

```cpp
class RingRenderer {
 public:
  virtual ~RingRenderer() = default;
  virtual void apply(const DeviceState& state) = 0;
  virtual void tick(uint32_t nowMs) = 0;
};

class DisplayRenderer {
 public:
  virtual ~DisplayRenderer() = default;
  virtual void apply(const DeviceState& state) = 0;
};

class KnobInput {
 public:
  virtual ~KnobInput() = default;
  virtual std::optional<KnobEvent> poll(uint32_t nowMs) = 0;
};
```

`main.cpp` 使用 `NullRingRenderer`、`NullDisplayRenderer` 和 `NullKnobInput`，完成：

- `Serial.begin(115200)`；
- 非阻塞读取；
- 协议 decoder；
- `HELLO` 后发送能力；
- 状态写入后 ACK/NACK；
- 每轮调用 renderer `tick`；
- 旋钮事件存在时编码上报；
- 3 秒通信 watchdog；
- 默认 `maxMilliAmps=500`、`brightnessCeiling=30`，直到真实供电验证后再提高。

不得使用 `delay()` 播放动画；主循环中的短暂让步不得超过 1 ms。

**Step 4：确认 GREEN**

```powershell
python -m platformio test -e native
python -m platformio run -e waveshare_amoled_143
```

**Step 5：提交**

```powershell
Set-Location D:\Project\codex-halo
git add firmware/halo-esp32s3
git commit -m "固件：实现设备状态机与安全适配层"
```

## Task 11：编写最小采购 BOM、接线和供电检查

**文件：**

- Create: `docs/hardware/2026-07-29-prototype-bom-v0.1.md`
- Create: `docs/hardware/2026-07-29-wiring-and-power-checklist.md`
- Modify: `apps/desktop/README.md`
- Modify: `firmware/halo-esp32s3/README.md`

**Step 1：写 BOM**

BOM 分成两栏：

1. 现在购买的一圈验证套件；
2. 仅估算、暂不购买的四圈扩展。

第一栏至少包含：

- Waveshare ESP32-S3-Touch-AMOLED-1.43 一块；
- 20 LED 的 WS2813/兼容 5V 灯环一圈；
- 可按压旋转编码器一个；
- 330–470 Ω 数据串联电阻；
- 500–1000 µF、耐压不低于 6.3V 的电源电容；
- 5V、带限流保护的电源；
- USB-C 数据线；
- 杜邦线、接线端子、面包板或免焊转接件；
- 万用表。

不要在尚未确认库存时写死商家价格；记录型号、关键规格和可替代条件。

**Step 2：写接线检查**

必须包含：

- 断电接线；
- 确认 5V 与 3.3V 逻辑边界；
- 共地；
- 先测空载电压；
- 数据电阻靠近首颗 LED；
- 大电容跨接灯环 5V/GND；
- 初次上电把电源限流设低；
- 第一轮只接一圈；
- 不从开发板 5V 引脚给四圈供电；
- 固件亮度上限保持 30%；
- 未确认灯环信号电平容差时预留电平转换器；
- 出现重启、闪烁、线材发热立即断电。

**Step 3：文档校验**

```powershell
rg -n "一圈|四圈|共地|限流|330|500|1000|maxMilliAmps|30%" docs/hardware firmware/halo-esp32s3/README.md
```

预期：所有关键安全项均能找到。

**Step 4：提交**

```powershell
git add docs/hardware apps/desktop/README.md firmware/halo-esp32s3/README.md
git commit -m "硬件：补充最小采购与安全接线清单"
```

## Task 12：执行无硬件端到端门禁

**文件：**

- Modify only if verification exposes a confirmed defect
- Update: `docs/research/2026-07-29-usb-device-foundation-verification.md`

**Step 1：桌面全量门禁**

```powershell
Set-Location D:\Project\codex-halo
python -m unittest discover -s tests -p 'test_*.py'
npm test --prefix apps/desktop
npm run typecheck --prefix apps/desktop
npm run build --prefix apps/desktop
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
```

预期：全部通过。

**Step 2：固件门禁**

```powershell
Set-Location D:\Project\codex-halo\firmware\halo-esp32s3
python -m platformio test -e native
python -m platformio run -e waveshare_amoled_143
```

预期：协议与状态机测试通过，Waveshare 目标编译成功。

**Step 3：模拟设备场景**

用 Rust 集成测试或测试命令逐项记录：

1. 启动为 `VIRTUAL`；
2. HELLO/CAPABILITIES 成功；
3. 完整快照含四圈且不含 task key；
4. 单圈状态变化只产生单圈更新；
5. 旋钮短按改变显示模式；
6. 旋转选择环绕 0..3；
7. 超时只重试两次；
8. 断线重连发送完整快照；
9. CRC 错误不会使 worker 崩溃；
10. 主版本不兼容进入 `INCOMPATIBLE` 且停止状态写入。

**Step 4：隐私扫描**

```powershell
rg -n "taskKey|task_key|serial_number|serialNumber" `
  docs/protocol `
  apps/desktop/src-tauri/src/device `
  firmware/halo-esp32s3
```

预期：只出现禁止项说明、领域输入字段或明确断言；设备载荷、诊断和固件状态中没有任务身份或 USB 序列号。

**Step 5：写验证报告**

报告必须包含：

- 所有命令与结果；
- 测试数量；
- PlatformIO 和目标平台版本；
- 固件产物路径与 SHA-256；
- 模拟器故障注入结果；
- 未验证项：实体屏幕、实体灯环、实体旋钮、真实 USB 握手、真实供电；
- 下一步只购买最小一圈验证套件。

**Step 6：提交**

```powershell
Set-Location D:\Project\codex-halo
git add docs/research/2026-07-29-usb-device-foundation-verification.md
git commit -m "验证：记录 USB 设备基础无硬件门禁"
```

## 完成定义

只有同时满足以下条件，本计划才算完成：

- Rust 与 C++ 读取同一份黄金向量并通过；
- 协议解析对半帧、噪声、CRC、超长负载和未知版本有确定行为；
- 模拟器验证握手、ACK/NACK、旋钮、超时和断线；
- 每次重连都用完整快照恢复；
- UI 同时区分 Codex Hook 适配器状态和 Halo 设备状态；
- 固件 native 测试通过；
- Waveshare ESP32-S3 目标编译通过；
- 现有拖拽、绑定、灯效和 Hook 回归通过；
- 设备通信中不包含任务身份、提示词、代码或 USB 序列号；
- BOM 和接线清单明确限制第一轮只接一圈；
- 验证报告清楚列出尚未经过实体硬件确认的部分。
