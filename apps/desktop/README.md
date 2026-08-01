# Codex Halo 桌面控制中心

Codex Halo 是 Windows 优先、跨平台架构的四环虚拟设备控制中心。当前
`0.1.0` 是开发包：它读取经过脱敏的本地 Codex Hook 生命周期事件，在四个
同心圆环中显示任务状态，并提供手动绑定、锁定、交换、灯效编辑和 USB CDC
Halo 设备开发路径。真实实体灯环、屏幕、编码器和供电兼容性仍需后续硬件
bring-up 验证。

## Windows 开发环境

- Windows 10/11 x64，Microsoft Edge WebView2 Runtime。
- Node.js `>=20.19.0` 与 npm。
- Rust stable MSVC 工具链；本机需具备 Visual Studio C++ Build Tools。
- Python 3 用于运行仓库根目录的 Hook 探针测试，桌面应用本身不内嵌 Python。

安装依赖并启动：

```powershell
Set-Location apps/desktop
npm install
npm run tauri dev
```

## 硬件原型提示

- 当前桌面包可以驱动虚拟设备或 USB CDC Halo 设备。Task 12 只完成无硬件门禁；真实硬件兼容性必须等实体 bring-up 后确认。
- 第一轮硬件验证只允许接一圈 20 LED 的 5 V 灯环，并使用带限流保护的 5 V 电源。
- 固件功耗限制保持 `maxMilliAmps=500`，亮度硬上限保持 30%，直到台架实测电流和温升通过。
- 独立供电时必须遵守完整清单中的上电/下电顺序；灯环未供电时禁止驱动 DATA。
- 不要把任务身份、提示词、回复、源码、路径、凭据或 USB 序列号发送给固件。
- 采购与接线资料见[最小采购 BOM](../../docs/hardware/2026-07-29-prototype-bom-v0.1.md)和[接线与供电检查清单](../../docs/hardware/2026-07-29-wiring-and-power-checklist.md)。

默认设备传输仍是模拟器 / `VIRTUAL DEVICE`。Tauri 开发环境需要显式设置
`CODEX_HALO_DEVICE_TRANSPORT=serial` 才会启用 USB CDC Halo 设备路径；Hook
探针事件只影响本地事件输入，不决定是否尝试连接 USB。浏览器中的
`npm run dev` 使用确定性的演示 bridge；`npm run tauri dev` 使用本地 Tauri
bridge。

## Hook 事件目录

桌面适配器按以下顺序寻找脱敏事件目录：

1. 测试或宿主传入的显式路径；
2. 环境变量 `CODEX_HALO_PROBE_DIR`；
3. 用户主目录下的 `.codex-halo/probe`。

探针只允许写入白名单生命周期元数据和不可逆任务指纹。不要把原始 Hook JSON、
提示词、回复、代码、认证信息或其他敏感内容放进该目录。

## 验证

```powershell
# 仓库根目录
python -m unittest discover -s tests -p 'test_*.py'

Set-Location apps/desktop
npm test
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

## Windows 打包

```powershell
Set-Location apps/desktop
npm run tauri build
```

配置会尝试生成 NSIS `setup.exe` 和 WiX `msi`。产物位于
`src-tauri/target/release/bundle/`；未安装的独立开发可执行文件位于
`src-tauri/target/release/codex-halo-desktop.exe`。部分 Windows 环境禁用了
VBSCRIPT，可能导致 WiX MSI 阶段失败；这种情况下使用已成功生成的 NSIS
安装包或开发可执行文件，不需要为了本开发包启用系统可选功能。

本开发包未签名，Windows 可能显示发布者未知。它不是实体硬件固件，也不代表
真实设备兼容性已经验证。
