# Codex Halo 桌面虚拟设备 MVP 验证记录

**执行日期：** 2026-07-29

**验证内容：** 基于 `21565de` 加本次提交中的源码与 Tauri 配置；构建产物使用
本次提交内的 `tauri.conf.json`。本文不使用尚未产生的提交哈希作自引用基线。

**平台：** Windows x64，Rust target `x86_64-pc-windows-msvc`

## 结论

桌面虚拟设备 MVP 的自动化测试、类型检查、前端生产构建和 Rust 静态检查均
通过。Windows release 主程序与 NSIS 安装包已实际生成；MSI 未生成，原因是
Tauri 下载 WiX 工具时发生 `timeout: global`，不是应用编译失败，也没有为了
打包启用 VBSCRIPT 或其他系统可选功能。

本轮没有实体硬件。2026-07-29 已在 Windows 实机启动 release 主程序并完成
一组可视化操作烟雾测试，但没有执行真实双任务到桌面圆环的计时实验。因此，
“真实两个 Codex 任务在 1 秒内更新正确圆环”和所有实体设备行为仍明确标记为
未验证，不计入通过项。当前真实 Hook 门禁还受到桌面进程未执行 Hook 的明确
阻断，因此 Task 13 不是全部门禁通过。

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

第一次执行 `npm run tauri build` 时，release 主程序成功，随后下载 NSIS
工具发生 Tauri 全局超时。第二次执行复用了缓存，NSIS 安装包成功生成，随后
下载 WiX 工具时再次发生相同网络超时。停止继续重试，未修改系统功能。

| 产物 | 大小（字节） | SHA-256 |
|---|---:|---|
| `D:\Project\codex-halo\apps\desktop\src-tauri\target\release\codex-halo-desktop.exe` | 9,053,184 | `4BCFDFC1DF5DC79125E7F688D3927FED95DFD03F9E251D04BF17F9231126D173` |
| `D:\Project\codex-halo\apps\desktop\src-tauri\target\release\bundle\nsis\Codex Halo_0.1.0_x64-setup.exe` | 2,181,752 | `E6D499DF8597FB1103D44B8577551C3323A21E2C66B642C9ABFA06E0CCAF3B1E` |

两项产物的 Windows 元数据均为 `Codex Halo 0.1.0`，Authenticode 状态均为
`NotSigned`。主程序进行了一次非交互进程烟雾测试：启动 3 秒后仍存活，随后
仅终止该次测试创建的进程。

同一 release 主程序还通过 Windows 实机可视化操作完成以下检查：

- 窗口标题为 `Codex Halo`，界面明确显示 `VIRTUAL DEVICE`，未出现 USB 错误；
- Adapter 显示 `ONLINE / HOOK`，四个圆环完整呈现；
- R01 显示 Hook/已观测的正在执行状态，其余圆环为空闲；
- 活动总线可见匿名 R01 正在执行与匿名 U02“本轮完成”；
- 点击表冠后中央屏从 `AMBIENT` 切换到 `OVERVIEW`，可见四圈总览；
- 点击任务轨 R01 后，匿名任务进入选中态，R01 灯效编辑控件解锁；
- 全局亮度从 100 调整为 99 后成功同步，再恢复为 100，验证了写入链路。

这些观察不包含任何任务指纹或内容，也不扩展为未实际执行的交互结论。

### 合成探针桌面验收

2026-07-29 还通过本地脱敏 probe 目录执行了合成事件桌面验收：

- 合成 `PreToolUse` 事件使四个圆环均被任务占用，并出现额外队列项；
- 合成 `PermissionRequest` 在桌面 UI 中显示候选等待信号；
- 合成 `Stop` 显示绿色“本轮完成”，没有宣称任务永久完成；
- 本轮产生的 8 个合成事件文件在验收结束后已清理。

这些结果验证的是合成探针文件经生产 adapter 到桌面 UI 的链路，不是真实
Codex 桌面进程执行 Hook 的证据，也没有测量小于 1 秒的端到端延迟。验收中还
发现手动绑定可覆盖锁定圈的缺陷；该缺陷随后通过领域层不变量和自动回归测试
修复，但尚未在打包后的窗口中重新执行同一人工步骤。

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
| 四圈动画与 reduced-motion | 组件、样式测试 | 四圈静态呈现已观察；动画与 reduced-motion 未操作 | 部分人工验证 |
| 五任务、拖拽、键盘绑定、锁定、交换、灯效编辑 | Store/组件测试 | 合成事件已验证四圈和额外队列；R01 选择、编辑解锁和亮度写回已验证；其余未逐项复验 | 部分人工验证 |
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
时间戳的 Windows 桌面会话中完成真实双任务小于 1 秒的端到端验证，并逐项执行
可视化交互验收，再决定是否进入硬件选型或签名发布。

## 隐私检查

本记录只包含测试计数、工具版本、匿名聚合结论、构建路径和产物摘要。
未从 Hook/探针事件读取或复制代码内容，也未复制或提交任务指纹值、提示词、
回复、原始 Hook JSON、环境变量值或认证信息。SHA-256 仅用于校验本轮构建产物。
