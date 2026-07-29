# Codex Halo 桌面虚拟设备 MVP 验证记录

**执行日期：** 2026-07-29

**验证内容：** 构建产物基于工作树干净的 `1f16425`，包含锁定圈位保护修复，
并使用该提交内的 `tauri.conf.json`。本文后续的报告更新不冒充构建输入。

**平台：** Windows x64，Rust target `x86_64-pc-windows-msvc`

## 结论

桌面虚拟设备 MVP 的自动化测试、类型检查、前端生产构建和 Rust 静态检查均
通过。Windows release 主程序与 NSIS 安装包已实际生成；MSI 未生成，原因是
Tauri 下载 WiX 工具时发生 `timeout: global`，不是应用编译失败，也没有为了
打包启用 VBSCRIPT 或其他系统可选功能。

本轮没有实体硬件。2026-07-29 已在 Windows 实机对锁定修复后的刷新主程序
完成锁定保护、显式解锁、键盘绑定、正常动画与 reduced-motion 人工验证。
Windows UI 自动化发起的两次拖拽没有触发 drop，但该结果受工具指针操作影响，
既不判定产品拖拽失败，也不计为人工通过。

本轮仍没有执行真实双任务到桌面圆环的计时实验。因此，“真实两个 Codex 任务
在 1 秒内更新正确圆环”和所有实体设备行为仍明确标记为未验证，不计入通过项。
当前真实 Hook 门禁还受到桌面进程未执行 Hook 的明确阻断，因此 Task 13 不是
全部门禁通过。

## 工具链

| 工具 | 实际版本 |
|---|---|
| Python | 3.14.0 |
| Node.js | 25.8.2 |
| npm | 11.11.1 |
| rustc | 1.97.1 (`x86_64-pc-windows-msvc`，LLVM 22.1.6) |
| cargo | 1.97.1 |
| Tauri CLI | 2.11.4 |
| 应用版本 | 0.1.0 |

`package.json` 声明的 Node.js 最低版本为 `20.19.0`；上表只表示本次验证实际
使用的版本，不把 Node 25 误写为最低要求。

## 自动验证结果

| 验证命令 | 实际结果 |
|---|---|
| `python -m unittest discover -s tests -p 'test_*.py'` | 共 64 项：63 通过，1 跳过 |
| `npm test` | 10 个测试文件、126 项测试全部通过 |
| `npm run typecheck` | 通过 |
| `npm run build` | 通过；Vite 转换 40 个模块 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 55 项测试全部通过；main 与 doc tests 为 0 项 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` | 通过 |
| `npm audit --audit-level=high` | 0 个已知漏洞 |

Python 唯一跳过项是符号链接拒绝测试。当前 Windows 会话创建测试符号链接时
返回 `WinError 1314`（缺少相应权限）；它不是产品测试失败，其他安装器事务与
安全测试均运行通过。

自动化覆盖的关键行为包括：

- 四槽状态机、第五任务队列、锁定槽保护、交换与自动回填；
- running、provisional waiting、roundCompleted 和 simulated failure 的区分；
- `Stop` 仅显示“本轮完成”，不宣称任务永久完成；
- 灯效参数范围、方向、亮度与实际变化才推进 revision；
- Hook 探针输入上限、路径安全、脱敏、离线恢复和匿名任务隔离；
- Vue 四环、任务轨、键盘/拖拽绑定、灯效编辑、reduced-motion；
- snapshot 与 adapter-status 的先订阅后读取、实时更新、版本裁决和卸载清理。

## Windows 打包结果

`tauri.conf.json` 的 Windows 开发包目标明确为 `nsis` 与 `msi`，默认窗口为
1440 × 900，最小窗口为 1080 × 680。

早期第一次执行 `npm run tauri build` 时，release 主程序成功，随后下载 NSIS
工具发生 Tauri 全局超时；第二次执行复用了缓存，NSIS 安装包成功生成，随后
下载 WiX 工具时发生相同网络超时。

锁定圈位修复提交 `1f16425` 后再次从干净工作树执行同一命令。前端生产构建与
Rust release 编译成功，缓存中的 NSIS 工具成功生成刷新安装包；随后下载 WiX
3.14.1 工具时再次发生 `timeout: global`，所以整条 Tauri 命令最终以失败退出，
但失败发生在已完成主程序与 NSIS 之后。停止继续重试，未修改系统功能。

| 产物 | 大小（字节） | SHA-256 |
|---|---:|---|
| `D:\Project\codex-halo\apps\desktop\src-tauri\target\release\codex-halo-desktop.exe` | 9,054,208 | `418BEED6FB3889CCD67F8C7323400A6E161CA1F84CEB810EC5F844EF44643D84` |
| `D:\Project\codex-halo\apps\desktop\src-tauri\target\release\bundle\nsis\Codex Halo_0.1.0_x64-setup.exe` | 2,178,076 | `340C85EAB6D94C23A83CB89B467B474BB2621F15C7ED084A2FA9063F90CDB3E2` |

两项刷新产物的 Windows `ProductName` 与 `FileDescription` 均为
`Codex Halo`，`FileVersion` 与 `ProductVersion` 均为 `0.1.0`，
Authenticode 状态均为 `NotSigned`。刷新主程序进行了一次非交互进程烟雾
测试：启动 3 秒后仍存活，随后仅终止该次测试创建的进程。该检查直接运行主
程序，不是安装验收；NSIS 仅完成生成、元数据和校验和核对，未执行安装、
卸载或升级。

此前的 release 基线主程序通过 Windows 实机可视化操作完成以下检查：

- 窗口标题为 `Codex Halo`，界面明确显示 `VIRTUAL DEVICE`，未出现 USB 错误；
- Adapter 显示 `ONLINE / HOOK`，四个圆环完整呈现；
- R01 显示 Hook/已观测的正在执行状态，其余圆环为空闲；
- 活动总线可见匿名 R01 正在执行与匿名 U02“本轮完成”；
- 点击表冠后中央屏从 `AMBIENT` 切换到 `OVERVIEW`，可见四圈总览；
- 点击任务轨 R01 后，匿名任务进入选中态，R01 灯效编辑控件解锁；
- 全局亮度从 100 调整为 99 后成功同步，再恢复为 100，验证了写入链路。

这些观察不包含任何任务指纹或内容，也不扩展为未实际执行的交互结论。

### 刷新包 Windows 人工验收

基于 `1f16425` 构建的刷新主程序还完成了以下 Windows 人工验证：

- 锁定 R01 后，选择一个等待队列任务并尝试绑定到 R01，操作被拒绝；R01 继续
  显示 `LOCKED`，原绑定、等待队列和当前选中任务均保持不变，界面显示
  “manualBind 操作失败”；
- 明确解锁 R01 后，将同一个等待任务绑定到 R01 成功，原占用任务回到队列；
- 使用实际键盘聚焦队列任务后，通过 `Tab` 导航至 R02 绑定按钮，按
  `Return` 成功完成交换/绑定并选中 R02；
- 正常模式的连续截图显示黄色光弧沿圆环追逐旋转；
- 以进程级环境变量
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--force-prefers-reduced-motion`
  启动刷新主程序后，间隔 900 ms 的两张画面中光弧位置一致，减少动画通过。

Windows UI 自动化还尝试了两次鼠标拖拽，但都没有触发 drop。现有证据无法区分
产品拖拽问题与自动化工具的指针/拖放限制，因此不据此宣称产品失败，也不宣称
人工通过；组件自动测试中的拖拽覆盖仍然通过，人工拖拽状态记为“工具操作
不确定/未确认”。

### 合成探针桌面验收

2026-07-29 还通过本地脱敏 probe 目录执行了合成事件桌面验收：

- 合成 `PreToolUse` 事件使四个圆环均被任务占用，并出现额外队列项；
- 合成 `PermissionRequest` 在桌面 UI 中显示候选等待信号；
- 合成 `Stop` 显示绿色“本轮完成”，没有宣称任务永久完成；
- 较早一轮产生的 8 个合成事件文件已清理；
- 刷新包验收新增 5 个脱敏 `PreToolUse` 合成文件，验证后逐个删除，并确认
  probe 目录剩余 0 个合成事件文件。

这些结果验证的是合成探针文件经生产 adapter 到桌面 UI 的链路，不是真实
Codex 桌面进程执行 Hook 的证据，也没有测量小于 1 秒的端到端延迟。验收中还
发现手动绑定可覆盖锁定圈的缺陷；该缺陷随后在 `1f16425` 通过领域层不变量
和自动回归测试修复，本节记录的刷新主程序与 NSIS 已包含该提交；刷新主程序
中的锁定拒绝与显式解锁路径也已按上一节完成人工验证。

**未生成：**
`D:\Project\codex-halo\apps\desktop\src-tauri\target\release\bundle\msi\`。
本轮 MSI 阻塞点是 WiX 下载网络超时，未观察到 VBSCRIPT 错误。

## Hook 与延迟证据

已有脱敏证据
[`2026-07-26-codex-status-evidence.md`](./2026-07-26-codex-status-evidence.md)
和匿名聚合报告
[`2026-07-26-two-task-analysis.json`](./2026-07-26-two-task-analysis.json)
证明：一次真实采集窗口内两个匿名任务组的生命周期事件发生重叠，两个任务
身份互不相同，且各自的工具活动和 `Stop` 可以独立归属。该证据验证的是
Hook 任务身份隔离，不是本次打包程序的 UI 路由或显示延迟。

当前实现的探针 worker 轮询间隔为 250 ms；前端集成测试证明 Tauri
`halo://snapshot` 事件到达 Store 后，可在一轮 Vue 微任务内更新对应圆环。
这两项分别是实现时钟和自动测试边界，不能相加冒充真实端到端延迟。

**真实 Hook 文件产生 → 打包应用接收 → 正确圆环完成渲染的墙钟延迟：未测量。**
因此计划中的“两个真实 Codex 任务在 1 秒内更新正确圆环”仍为未验证。

### 当前真实双任务门禁阻断

2026-07-29 使用现有双任务验收 B 运行 15.373 秒，共产生 468 项执行记录。
检查确认用户目录下的 `.codex/hooks.json` 已配置且与官方 Hook 结构一致，
`config.toml` 也显式设置了 `hooks = true`；但是运行结束后
`.codex-halo/probe` 目录没有新增文件。

这组证据说明当前 Codex 桌面进程没有执行已配置的 Hook。由于探针入口没有
产生事件，本次运行不能验证打包应用的双任务路由，也不能测量小于 1 秒的端到端
延迟。需要重启 Codex 桌面进程并重新检查 Hook 信任/审批状态后，再重复双任务
门禁。这里记录的是当前真实阻断，不把 15.373 秒运行或 468 项记录误报为通过。

## 桌面与硬件验证边界

| 项目 | 自动证据 | 人工/真实证据 | 结论 |
|---|---|---|---|
| 无硬件时显示 `VIRTUAL DEVICE`，无 USB 依赖 | App 组件测试和无 USB 实现 | release 实机显示正常且无 USB 错误 | 通过 |
| 四圈动画与 reduced-motion | 组件、样式测试 | 正常模式连续截图看到黄色光弧追逐；进程级强制 reduced-motion 后，间隔 900 ms 的两张画面光弧位置一致 | 通过 |
| 五任务、锁定、手动绑定与交换 | Store/组件测试 | 合成事件验证四圈和额外队列；刷新包验证锁定拒绝保持原子状态，显式解锁后可换入等待任务并将原任务送回队列 | 通过 |
| 键盘绑定 | 组件测试 | 实际键盘聚焦队列任务，`Tab` 到 R02 绑定按钮，`Return` 完成交换/绑定并选中 R02 | 通过 |
| 鼠标拖拽 | 组件测试通过 | Windows UI 自动化两次未触发 drop，工具操作不确定 | 人工未确认，不判产品失败 |
| 灯效编辑 | 组件测试 | R01 选择、编辑解锁和全局亮度写回已验证 | 部分人工验证 |
| waiting 候选标记、Stop“本轮完成” | 实时集成与组件测试 | 合成 `PermissionRequest` 候选等待与合成 `Stop` 绿色“本轮完成”已观察 | 合成链路通过，真实 Hook 未验证 |
| 表冠和中央屏模式 | 组件测试 | `AMBIENT` → `OVERVIEW` 及四圈总览已观察 | 通过 |
| Adapter 状态与 Hook 来源 | 实时集成测试 | 实机显示 `ONLINE / HOOK`，R01 为 Hook/已观测 | 通过 |
| 两个真实任务不串圈且小于 1 秒 | 上游匿名身份 Gate 通过 | 验收 B 已运行，但当前桌面进程未执行 Hook，probe 无新增文件 | 阻断，未验证 |
| PermissionRequest 稳定代表等待审批 | provisional 映射测试 | 尚无真实 Hook 证据 | 未验证 |
| 失败状态 | simulator 明确标注 | 真实 Hook 失败推断未实现 | 未验证 |
| Codex/驱动重启后的身份连续性 | 无 | 无 | 未验证 |
| NSIS 安装、卸载与升级 | 安装包已生成 | 未执行系统安装 | 未验证 |
| MSI | 无产物 | WiX 下载超时 | 未验证 |
| 实体灯、USB、固件、屏幕和旋钮 | 不在 MVP 实现中 | 尚未购买硬件 | 未验证 |

硬件采购继续冻结。下一阶段应先重启 Codex 并复查 Hook 信任状态，再在可记录
时间戳的 Windows 桌面会话中完成真实双任务小于 1 秒的端到端验证；使用可确认
原生 drop 的输入方式单独复验人工拖拽，并在 WiX 工具可用后补齐 MSI，同时
另行执行 NSIS 安装、卸载与升级验收。完成这些软件门禁后，再决定是否进入
硬件选型或签名发布。

## 隐私检查

本记录只包含测试计数、工具版本、匿名聚合结论、构建路径和产物摘要。
未从 Hook/探针事件读取或复制代码内容，也未复制或提交任务指纹值、提示词、
回复、原始 Hook JSON、环境变量值或认证信息。SHA-256 仅用于校验本轮构建产物。
