# Codex Halo USB 设备基础无硬件验证记录

**执行日期：** 2026-08-01；2026-08-13 在合并 diagnostics 审查提交并完成修复后全量复验

**初始验证基线：** `1b897ac`（Task 11）

**本轮远端基线：** `a98cfad`（合并另一开发环境已推送的四个审查提交后）

**验证平台：** Windows x64；本轮没有连接或使用实体 Halo 硬件。

## 结论

USB 设备基础计划的无硬件门禁全部通过。桌面端 Python、Vue/TypeScript、Rust，
以及固件原生测试和 Waveshare ESP32-S3 编译均成功；协议级模拟设备覆盖了握手、
脱敏快照、增量更新、旋钮事件、两次重试、断线重连、CRC 故障恢复和版本不兼容
停写。生产 worker 迭代还验证了 CRC 故障后继续同步。Rust 与 C++ 测试读取同一
份 5 条有效帧黄金 TSV 和同一份 3 条异常流 TSV；有效帧包含严格 7 字节诊断，
异常流锁定了 CRC 失败后的
逐字节重同步，以及严格 v0.1 固定载荷长度的提前拒绝。有限的双向 `DIAGNOSTICS`
路径也已实现并通过回归：桌面端和固件共享严格 7 字节载荷语义，设备端诊断使用与
旋钮事件共享的单调序号，协议错误报告被限流并在背压时保留，桌面端只显示固定、
去值、脱敏的诊断文案。隐私扫描没有发现任务身份或 USB 序列号进入设备载荷、
设备诊断或固件状态。

这些结果只证明协议、状态机、模拟传输、UI 合约和目标固件能够在无硬件环境中
通过自动验证。它们不代表真实 USB CDC、实体 AMOLED、实体灯环、实体旋钮或
真实供电已经验证。

## 工具链

| 工具 | 本轮实际版本 |
|---|---|
| Python | 3.11.15 |
| Node.js | 25.8.2 |
| npm | 11.11.1 |
| rustc | 1.97.1 (`x86_64-pc-windows-msvc`) |
| cargo | 1.97.1 |
| PlatformIO Core | 6.1.19 |
| PlatformIO Espressif32 | 7.0.1 |
| Arduino-ESP32 | 2.0.17（PlatformIO 包版本 `3.20017.241212+sha.dcc1105b`） |

固件命令均显式设置 `PLATFORMIO_CORE_DIR=D:\DevTools\PlatformIO`、
`TEMP/TMP=D:\DevTools\Temp`、`PIP_CACHE_DIR=D:\DevTools\PipCache`、
`UV_CACHE_DIR=D:\DevTools\UvCache`、`PYTHONUTF8=1`，并把
`D:\DevTools\CLion 2026.1\bin\mingw\bin` 放到本轮 `PATH` 前部。

## 全量门禁

以下命令都从仓库根目录执行，固件命令按表中注明的子目录执行。

| # | 命令 | 退出码 | 结果 |
|---:|---|---:|---|
| 1 | `python -m unittest discover -s tests -p 'test_*.py'` | 0 | 64 项运行：63 通过、1 跳过；耗时 16.642 s |
| 2 | `npm test --prefix apps/desktop` | 0 | 12 个测试文件、184 项测试全部通过 |
| 3 | `npm run typecheck --prefix apps/desktop` | 0 | `vue-tsc --noEmit` 通过 |
| 4 | `npm run build --prefix apps/desktop` | 0 | 类型检查和 Vite production build 通过，转换 42 个模块 |
| 5 | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | 0 | Rust 142 项全部通过；main 和 doc tests 各 0 项 |
| 6 | `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings` | 0 | 通过 |
| 7 | `python -m platformio --version` | 0 | `PlatformIO Core, version 6.1.19` |
| 8 | `python -m platformio test -e native` | 0 | 3 个测试组、52/52 通过：HAL 6、协议 19、状态 27 |
| 9 | `python -m platformio run -e waveshare_amoled_143` | 0 | Espressif32 7.0.1，目标构建成功；RAM 19,764/327,680 字节，Flash 281,445/6,553,600 字节 |

第 7～9 项在 `firmware/halo-esp32s3` 下执行。Python 唯一跳过项是 Windows
会话无法创建符号链接时主动跳过的 Hook 安装测试；其余安装器和路径安全测试
均实际运行。Rust 测试日志中的 MSVC linker stdout 仅说明正在创建 import
library；随后 142 项测试通过，且独立的 Clippy `-D warnings` 和
`cargo fmt -- --check` 门禁均为 0。

## 十个模拟设备场景

全量 Rust 门禁已经运行以下测试。本节另外执行了聚焦回归：worker CRC 1/1、
manager 25/25、simulator 16/16、表冠模式/环绕 1/1、四圈脱敏投影 1/1，均为
退出码 0。

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml app_state::tests::device_worker_iteration_survives_crc_error_and_continues_syncing -- --exact --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::manager::tests -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::simulator::tests -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain::engine::tests::presentation_intents_cycle_modes_and_wrap_ring_selection -- --exact --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml device::presentation::tests::projection_contains_four_ring_states_but_no_task_identity -- --exact --nocapture
```

| # | 场景 | 自动化证据 | 结果 |
|---:|---|---|---|
| 1 | 启动为 `VIRTUAL` | `manager_handshakes_then_sends_an_authoritative_full_snapshot` 断言 manager 为 `Virtual`；`App.test.ts` 和 `DeviceStatus.test.ts` 验证界面文案 `VIRTUAL` | 通过 |
| 2 | HELLO/CAPABILITIES 握手 | `manager_handshakes_then_sends_an_authoritative_full_snapshot`；`simulator_handshakes_acks_state_and_records_the_snapshot` | 通过 |
| 3 | 完整快照包含四圈且脱敏 | `projection_contains_four_ring_states_but_no_task_identity` 断言 4 圈、空 label，编码载荷不含测试 TaskKey | 通过 |
| 4 | 单圈变化只发送单圈增量 | `delta_sends_only_changed_ring_or_global_fields` 只产生 R03（索引 2）更新；`simulator_applies_incremental_state_writes_and_acks_each_sequence` 验证应用与 ACK | 通过 |
| 5 | 旋钮短按改变显示模式 | `old_knob_sequences_are_ignored_and_new_events_become_intents` 把新短按转换为 intent；`presentation_intents_cycle_modes_and_wrap_ring_selection` 验证 `Ambient → Overview → Detail → Ambient` | 通过 |
| 6 | 旋转选择在 0..3 环绕 | `presentation_intents_cycle_modes_and_wrap_ring_selection` 验证初始正转到 0、反转到 3，并验证反向首次选择 3 | 通过 |
| 7 | 超时最多重试两次 | `manager_retries_twice_then_reconnects_with_a_full_snapshot` 明确断言 `retry_count == 2` 后重连 | 通过 |
| 8 | 断线重连发送完整快照 | `every_new_connection_sends_a_full_snapshot` 断线前后完整快照计数从 1 增到 2，状态恢复 `Virtual` | 通过 |
| 9 | CRC 错误不使 worker 崩溃 | `device_worker_iteration_survives_crc_error_and_continues_syncing` 通过生产 `run_device_manager` 实际调用的单次迭代路径，在显式时点注入坏 CRC；5 次迭代均返回，锁外发布保持有效，最终状态为 `Virtual` 并同步亮度 50。manager 的 `corrupt_crc_once_retries_after_timeout_and_applies_the_final_state` 同时提供协议重试细节 | 通过 |
| 10 | 主版本不兼容进入 `INCOMPATIBLE` 并停写 | `incompatible_major_never_receives_state_writes` 断言状态为 `Incompatible` 且 `state_write_count == 0`；能力不足路径也有同样停写断言 | 通过 |

## 故障注入

模拟传输的故障队列按 FIFO、一次性消费，相关 simulator 聚焦测试 16/16 和
manager 聚焦测试 25/25 全部通过：

- `TimeoutOnce`：连续 3 次超时只形成 2 次重试，随后重连并恢复完整快照；
- `NackOnce(Busy)`：单次 NACK 不应用被拒绝的写入；连续 NACK 同样只重试
  2 次，重连后以完整快照恢复最终状态；
- `CorruptCrcOnce`：坏 CRC 不确认当前写入，ACK 超时后重试，最终状态成功应用；
- 显式断线：下一连接重新发送权威完整快照，而不是从旧增量继续；
- 畸形载荷：返回 NACK 且不改变已应用状态；decoder 错误后，同一批次后续有效帧
  仍可恢复处理；
- 不可能的固定 v0.1 载荷长度：生产 manager 和模拟设备均在收齐 8 字节头部后
  立即拒绝，不等待声明的 512 字节；模拟设备按固件语义返回 malformed-payload
  NACK，并继续处理同一批次内嵌的后续有效帧；
- 过期旋钮序号被忽略，序号环绕后的新事件被接受，零增量旋转被拒绝。

固件侧相应故障边界也通过原生测试：CRC 后续帧恢复、超长拒绝、未知版本/类型
结构化错误、严格 v0.1 固定载荷长度恢复、ACK/NACK 队列背压，以及版本错误进入
安全状态。

## 有限双向诊断路径

协议中的 `DIAGNOSTICS` 现已从声明但未贯通的消息类型变成有限、可测试的双向路径：

- Rust 与 C++ 均按 `severity:u8 + code:u16 LE + value:u32 LE` 编解码严格 7 字节载荷，
  未知严重级别、未知代码和错误长度都会被拒绝；
- 桌面端在 CRC 或其他可报告协议错误后，每次 manager step 最多发送一条诊断，
  不等待 ACK、不占用正常重试状态；发送失败复用现有安全重连路径；
- 固件只保留按 4 个固定代码索引的有界待发送槽位。CRC/无效载荷按饱和计数合并，
  看门狗断开和本地亮度限制按状态边沿或最新值合并，不分配无界队列；
- 固件诊断与旋钮事件共享设备事件序号，只有整帧成功进入发送队列后才递增；
  ACK/NACK 优先于诊断，诊断优先于旋钮，发送队列满时诊断仍保留在控制器中；
- 双方收到有效诊断都静默消费，不回复 ACK，也不会刷新固件看门狗或改变权威显示状态；
  无效桌面诊断由固件安全 NACK 并合并为无效载荷诊断，因此不会形成回复循环；
- 设备上报的数值不会进入 UI。Rust 只映射为 4 条固定、去值、脱敏消息，Vue
  allowlist 允许这些固定消息并继续隐藏任何未知非空文本。
- 设备诊断文案采用事件驱动生命周期：下一次成功状态 ACK 或握手完成后清除；
  纯心跳空闲期没有计时过期，因此最后一条诊断可以保留到下一次上述成功事件。

TDD RED 分别复现了 Rust 缺少诊断类型、模拟器无法注入真实诊断、manager 忽略
设备诊断、UI 隐藏预期安全消息、诊断写失败未覆盖，以及固件缺少类型、生产者、
有界队列和共享事件序号。GREEN 后相关用例包含在 Rust 142/142、Vitest 184/184
和固件 native 52/52 中；最后的背压复查还补充验证了无效诊断会先预留 NACK
容量，避免发送队列满时重复处理和重复计数。

独立规格审查随后发现，不兼容状态的前置门禁会在解析前把合法诊断错误拒绝为
`InvalidState`。新增聚焦 RED 在 `UnsupportedVersion` 进入安全状态后复现了该
NACK；最小修复只允许 `Hello` 与 `Diagnostics` 进入各自分支。GREEN 证明合法
诊断仍静默且不恢复兼容状态、不刷新通信、不改变安全状态、不清除待发送诊断；
非法诊断仍结构化拒绝为 `MalformedPayload`，合法与非法诊断之后的心跳均继续被
`InvalidState` 阻止，直接证明兼容状态没有被恢复。随后响应预测复查又加入非空
心跳以及不可响应 CRC/版本错误用例。最终固件状态 27/27、全量 52/52 和目标构建
均通过。

## 共享黄金向量与异常流

唯一来源是：
`D:\Project\codex-halo\docs\protocol\golden-vectors.tsv`，包含 `hello`、
`heartbeat`、`ack_hello`、`brightness_80`、`diagnostics_crc_error` 共 5 条向量。

- Rust `device::protocol::tests::encodes_and_decodes_all_published_golden_vectors`
  通过 `include_str!("../../../../../docs/protocol/golden-vectors.tsv")` 读取该文件，
  在全量 142 项中通过；
- C++ `test_all_shared_golden_vectors_encode_and_decode` 通过
  `std::ifstream("../../docs/protocol/golden-vectors.tsv")` 读取同一文件，在 native
  52 项中通过。

异常流的唯一来源是
`D:\Project\codex-halo\docs\protocol\decoder-stream-vectors.tsv`，包含 3 条：

- `crc_nested_valid`：坏 CRC 候选帧的声明区间内含一帧有效心跳；两端先报告
  CRC 错误，再从坏候选魔数之后恢复该心跳；
- `strict_invalid_length_nested`：`CAPABILITIES` 声明通用上限 512 字节但不符合
  v0.1 固定 9 字节布局；两端严格模式在 8 字节头部处拒绝，并恢复后续心跳。
- `crc_two_nested_valid`：坏 CRC 外层内含连续心跳和亮度两帧，尾部故意留下
  `A5 43`。两端恢复两帧后把尾部归一为合法半魔数 `43`，紧接下一次 `push`
  的正常 HELLO 只产生一帧成功结果，不产生伪版本错误。

Rust 测试通过 `include_str!`、C++ 测试通过 `std::ifstream` 读取同一异常流文件。
因此两端不是各自复制测试常量，而是对两份共享 TSV 执行有效帧编码/解码与异常流
重同步验证。Rust 默认 `Generic` 模式仍能保留并完成合法的 512 字节帧；生产
manager 和模拟设备接收端显式使用 `StrictV01`。

## 隐私扫描

执行：

```powershell
rg -n "taskKey|task_key|serial_number|serialNumber" `
  docs/protocol `
  apps/desktop/src-tauri/src/device `
  firmware/halo-esp32s3
```

退出码为 0，共 7 行匹配；逐行解释如下：

| 命中 | 解释 |
|---|---|
| `manager.rs:671 task_key: None` | manager 测试夹具明确构造无任务身份的槽位，不进入设备载荷 |
| `manager.rs:1267 taskKey` | 负向断言：序列化设备状态不得包含 `taskKey` |
| `manager.rs:1268 serialNumber` | 负向断言：序列化设备状态不得包含 `serialNumber` |
| `presentation.rs:348 task_key: Some(...)` | 脱敏投影测试故意在领域夹具放入 TaskKey，随后断言设备载荷不含该值且所有 label 为空 |
| `serial.rs:252 serial_number` | 测试帮助函数的可选参数，只用于构造含虚构 USB 序列号的第三方 `UsbPortInfo` 测试值 |
| `serial.rs:259 serial_number` | 同一测试帮助函数把虚构值复制进第三方测试结构，随后转换为生产候选项以验证序列号被丢弃；不是设备载荷或生产状态字段 |
| `serial.rs:371 diagnostics_never_expose_usb_serial_numbers` | 测试名称，断言敏感序列号不出现在诊断标签或 Debug 输出 |

`docs/protocol` 和 `firmware/halo-esp32s3` 均无匹配。7 行允许命中全部位于测试夹具、
测试名称或负向断言；设备协议载荷、设备状态、诊断输出和固件状态没有携带任务
身份或 USB 序列号。协议同时固定空 label，未承载提示词、回复或代码内容。

## 固件构建产物

| 产物 | 字节数 | SHA-256 |
|---|---:|---|
| `D:\Project\codex-halo\firmware\halo-esp32s3\.pio\build\waveshare_amoled_143\firmware.bin` | 281,808 | `DD6C7AF93910EAC7AD2E759B0BA60E9643B31A8DF15CD5FFCA9ABCC360230F96` |

产物信息由 PowerShell `Resolve-Path`、`Get-Item` 和
`Get-FileHash -Algorithm SHA256` 从上述 `firmware.bin` 直接读取，命令退出码 0。

该文件只是针对 `esp32-s3-devkitc-1` / Arduino 框架成功编译出的固件二进制，尚未
烧录到 Waveshare 实体板，也未验证其真实外设引脚或电气行为。

## 完成定义与验证边界

无硬件可验证的完成定义已经满足：两端共享有效帧与异常流向量；半帧、噪声、CRC、
超长、固定 v0.1 长度和未知版本行为确定；模拟器覆盖握手、ACK/NACK、旋钮、超时和断线；每次新连接使用
完整快照；CRC 恢复通过生产 worker 迭代边界验证；UI 区分 Hook adapter 与
Halo device 状态；现有拖拽、绑定、灯效和
Hook 回归包含在 Python/Vitest/Rust 全量门禁中；BOM 与接线文档把首轮限制为
一圈并保留安全供电边界。

以下项目仍未经过实体验证：

- 真实 Windows USB CDC 枚举、串口打开、HELLO/CAPABILITIES 和重连；
- Waveshare 1.43 英寸 AMOLED 的真实初始化、刷新、颜色和布局；
- 20 LED 灯环的真实电平兼容、方向、动画、亮度和温升；
- 实体旋转编码器的方向、消抖、短按和长按；
- 外部 5 V 限流电源、共地、AHCT125 电平转换、压降、浪涌和断电顺序；
- 四圈扩展、外壳、定制 PCB 和长期稳定性。

下一步不是购买四圈成品，而是只购买 BOM 中的最小一圈验证套件：一块主控圆屏
板、一圈约 20 颗 LED、一个可按压编码器、规定的电平转换/保护元件、限流 5 V
电源、连接材料和万用表。完成单圈真实 USB、显示、灯效、旋钮和供电验证后，
再决定是否扩展到四圈。

## 工作树检查

更新本报告和 `AGENTS.md` 后执行 `git diff --check`，退出码 0；精确暂存两份
文件后执行 `git diff --cached --check`，退出码同样为 0。后续规格审查发现缺少
worker 边界 CRC 证据，因此从生产 `run_device_manager` 提取并复用了单次迭代
函数，新增一个 worker 回归测试；CRC、重试和设备状态机行为本身没有改变。

总体审查随后发现 Rust 与固件对两项接收语义不一致：Rust 会把 CRC 失败候选按
声明帧长整体丢弃，且生产接收端没有固定 v0.1 载荷长度的严格模式。本轮以共享
`decoder-stream-vectors.tsv` 建立两端同源证据，把 Rust CRC 失败路径改为丢弃
当前候选魔数后逐字节重同步，并为 Rust 增加与固件对称的 `Generic` / `StrictV01`
模式。聚焦 RED 分别复现了内嵌帧丢失和 512 字节不可能声明阻塞；修复后 Rust
132/132、固件 native 39/39、Clippy 和 rustfmt 检查均通过。质量复查进一步用
第三条共享流复现了 Rust 丢失第二个内嵌帧、C++ 遗留坏外层 CRC 尾并在下一次
`push` 产生伪版本错误的问题；两端现会逐帧消耗成功结果，并把剩余噪声归一为
空缓冲或单字节半魔数。该修复没有增加任何
实体硬件验证结论。

界面复查还发现串口在线时的部分文案仍写成“虚拟设备”，且设备状态消息原本既不
显示，也没有进入无障碍名称。修复后文案改为 transport-neutral 的“设备”，固定
安全消息可见且可访问；前端只允许 Rust 生产路径当前使用的 16 条固定脱敏文案，
未知非空文本统一显示“设备诊断信息已隐藏”，空白文本不增加视觉或无障碍噪音。
新增测试分别覆盖裸任务指纹、提示词、Windows/Unix 串口、任意 USB 序列号和原始
帧十六进制，前端全量 180/180、类型检查和生产构建均通过。该修复通过独立规格与
代码质量审查。

本轮随后补齐批准设计要求的有限双向诊断路径。诊断实现的 RED/GREEN、背压、
限流、共享序号、静默消费、固定 UI 文案和隐私边界均已通过上述自动化门禁；更新后
全量结果为 Rust 142/142、Vitest 184/184、固件 native 52/52，目标构建成功。
该路径仍只经过模拟传输、原生状态机测试和目标编译，不能视为真实 USB CDC
双向诊断已经在实体板上验证。

2026-08-13 合并远端审查工作后，独立审查发现并关闭 S1-S4、Q1-Q6。剩余修复包括
接收端方向宽容规则、第五条跨语言诊断黄金向量、成功 ACK/握手后的诊断文案清除、
统一固件响应预测 helper、统一 Rust 写帧 helper 和带类型的诊断转换错误。Q4 属于
编译边界硬化，没有传统运行时 RED：公开 v0.1 路径不能构造越界槽位值。完整处置
和残余边界见 `docs/research/2026-08-12-diagnostics-independent-review.md`。
