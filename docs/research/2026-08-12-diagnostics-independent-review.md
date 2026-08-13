# 有限双向诊断路径：独立规格审查与独立代码质量审查

**审查对象：** 工作区中尚未提交的有限双向 `DIAGNOSTICS` 实现及其文档。

**审查范围：**

- `apps/desktop/src-tauri/src/device/protocol.rs`
- `apps/desktop/src-tauri/src/device/manager.rs`
- `firmware/halo-esp32s3/lib/HaloCore/HaloState.hpp` / `HaloState.cpp`
- `firmware/halo-esp32s3/lib/HaloHal/HaloHal.hpp`
- `firmware/halo-esp32s3/src/main.cpp`
- `apps/desktop/src/components/DeviceStatus.vue`
- `docs/protocol/codex-halo-usb-v0.1.md`、`docs/protocol/golden-vectors.tsv`
- `docs/research/2026-07-29-usb-device-foundation-verification.md`

**审查环境：** macOS 工作副本（`/Users/mac/Desktop/codex-halo`）。该副本没有 `.git`，也没有 Rust、PlatformIO 工具链，因此本轮只能做静态审查，不能重跑 Rust、Vitest、Clippy、前端构建或固件门禁。可运行且已运行的门禁只有 Python 回归。

## 已执行的门禁

| 命令 | 结果 |
|---|---|
| `python3 -m unittest discover -s tests -p 'test_*.py'` | 64 tests, OK（macOS 上无 Windows 符号链接跳过） |

其余门禁未执行，状态仍以 Windows 环境的上一轮记录为准。

## 独立规格审查

结论：**有条件通过。** 实现行为与批准设计一致，未发现实现违反协议文档中已冻结的字节布局或常量。发现 4 项规格文档缺口，其中 S1、S2 已在本轮直接补齐协议文档；S3、S4 需要在有门禁的环境中处理。

### S1 双向诊断的行为契约不在冻结的协议文档中（已修复）

`docs/protocol/codex-halo-usb-v0.1.md` 只定义了 `DIAGNOSTICS` 的 7 字节布局和"双向、不要求 ACK"，没有定义现在两端实际遵守的规范行为：合法诊断必须静默消费且不刷新看门狗、不改变权威状态；非法诊断由固件复用 `sequence` 回 `NACK`（`rejectedMessageType = 81`），而桌面端不回 `NACK` 而是发送代码 `0003` 的诊断；发送方必须限流。

这些规则此前只写在验证报告里。验证报告是过程记录，不是跨语言线路契约：仅按协议文档实现的第三方固件可以合法地对诊断回 `ACK` 或用诊断刷新看门狗，从而破坏互操作。

`§2` 已补入上述规则，`§4.12` 已补入"长度、级别、代码任一不匹配即格式错误"。

### S2 代码表方向语义不明（已修复）

`§3.6` 的代码表按设备视角措辞（"看门狗进入断连状态"、"CRC 错误"）。路径变为双向后，桌面端也会发送 `0002` 与 `0003`，用来描述桌面端自己的解码失败。文档没有说明代码是相对发送方还是相对接收方，接收方无法判断对端的 `0002` 指的是"你出错了"还是"我出错了"。`value` 的取值也只写了"与代码相关的非敏感数值"，两端实际取值不同。

`§3.6` 已补入方向无关（相对发送方）的说明，以及按代码和方向固定的 `severity`/`value` 表。

### S3 `DIAGNOSTICS` 没有跨语言黄金向量（待处理）

`golden-vectors.tsv` 有 `hello`、`heartbeat`、`ack_hello`、`brightness_80` 四行，没有诊断行。`decoder-stream-vectors.tsv` 里的 `0x81` 只作为损坏外层容器出现，不校验诊断负载语义。

计划的完成定义要求"Rust 与 C++ 读取同一份黄金向量并通过"。目前诊断编码只由两端各自的单元测试覆盖，而这两份测试来自同一次实现，无法排除共同误读。

建议新增一行（CRC 已用 CRC-16/CCITT-FALSE 本地核算，并用现有 `hello` 行反向验证过实现）：

```text
diagnostics_crc_error	81	0005	02020007000000	4348018105000700020200070000009046
```

必须在能同时运行 Rust 与 PlatformIO native 测试的环境中加入，并让两端的共享向量测试真正消费该行，否则不得写入 TSV。

### S4 诊断消息的生命周期未定义（待处理）

`DeviceManager` 把最后一条有效设备诊断存入 `last_diagnostic_message`，并在每次 ACK 后经 `restore_diagnostic_message` 重新写回 `status.message`；只有 `reset_connection_state` 会清除。因此一次瞬时的设备 CRC 错误会让 UI 上的"Device reported a CRC error"一直显示到重连为止，而固件没有"已恢复"诊断。

`manager.rs:537`、`manager.rs:584`。这属于边沿事件被当作持久状态呈现。协议文档和批准设计都没有规定诊断文案的存活时间。需要在 Windows 环境中二选一：把"锁存到重连"写进设计作为 v0.1 的明确行为，或加入清除条件（例如成功完成 N 次状态写入后清空），后者需要 TDD 覆盖。

## 独立代码质量审查

结论：**有条件通过。** 未发现正确性缺陷或隐私泄漏。发现 5 项质量问题，均需在有门禁的环境中修改并重跑测试，本轮不改代码。

### Q1 诊断合法性判定被复制成两份（固件）

`firmware/halo-esp32s3/src/main.cpp:43` 的 `requiresResponse` 重新调用 `Diagnostic::decodePayload`，用来决定是否为 `NACK` 预留发送槽位；`HaloState.cpp:245` 的 `Diagnostics` 分支再判定一次。同一条规则有两份实现，将来任何"什么算合法诊断"的改动只要漏改一处，槽位预留就会和实际是否回 `NACK` 脱节：TX 队列满时会在 `processDecoded` 里提前 `return`，或者反过来白白占用一个槽位。

建议由 `DeviceController` 暴露单一判定（例如 `wouldRespond(const Frame&)`），`main.cpp` 只调用它。

### Q2 诊断发送路径复制了 `begin_request` 的编码与写入逻辑（桌面端）

`manager.rs:507` 的 `report_protocol_diagnostic` 重复了 `manager.rs:427` `begin_request` 的 encode → write → 失败即 `force_reconnect` 结构，连两处错误文案（`"Device frame could not be encoded"`、`"Device write failed"`）都是复制的。差别只是诊断不设置 `pending`。建议抽出 `fn write_frame(&mut self, frame: &Frame) -> bool`，两处共用。

### Q3 心跳分支写入了不会被发送的响应字段（固件）

`HaloState.cpp:238` 构造 `ControllerResponse` 并设置 `type = Heartbeat`、`sequence = frame.sequence`，但 `shouldSend` 保持默认 `false`，这两个字段永远不会上线。读代码时容易误以为固件会回送心跳。建议删除这两个赋值，或补一行说明为什么保留。

### Q4 诊断槽位下标依赖代码值连续（固件）

`HaloState.cpp:386` 与 `:391` 都用 `static_cast<size_t>(code) - 1` 索引 `std::array<std::optional<Diagnostic>, 4> pendingDiagnostics_`。只要将来新增第 5 个代码或代码值不再从 1 连续编号，就会越界写。建议加 `static_assert` 绑定代码数量与数组长度，或改成显式映射函数。

### Q5 旋钮输入的锁存要求没有写进接口（固件）

`main.cpp:122` 在存在待发诊断时完全跳过 `knobInput.poll(nowMs)`，因此 `KnobInput` 实现必须把事件锁存到被轮询为止，否则用户的旋钮操作会在诊断积压期间被静默丢弃。`HaloHal.hpp` 的 `KnobInput` 接口没有写这条要求，而 v0.1 只有 `NullKnobInput`，实测无法暴露该问题。建议在接口注释中写明"实现必须保留事件直到 `poll` 被调用"。

### Q6 Rust `TryFrom` 使用 `()` 作为错误类型（桌面端，次要）

`protocol.rs:53` 与 `:75` 的 `TryFrom` 实现用 `type Error = ()`，所有调用点都写成 `.ok()?`，错误信息被完全丢弃。改成返回 `Option` 的 `from_u8` / `from_u16` 关联函数更直白，也避免和 `MessageType::try_from` 返回结构化 `ProtocolError` 的风格混淆。

## 未验证项

- Rust 142 项测试、Clippy、rustfmt、Vitest 184 项、TypeScript 检查、Vite 生产构建：本环境无 Rust 工具链与 `node_modules`，未执行。
- 固件 native 47 项测试与 `waveshare_amoled_143` 目标构建：本环境无 PlatformIO，未执行。
- 提交与推送：本工作副本没有 `.git`，无法比对未提交改动，也无法提交。
- 实体硬件：屏幕、灯环、旋钮、真实 USB 握手、真实供电仍全部未验证。
