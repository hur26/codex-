# Codex Halo 桌面控制中心

Codex Halo 是 Windows 优先、跨平台架构的四环虚拟设备控制中心。当前
`0.1.0` 是开发包：它读取经过脱敏的本地 Codex Hook 生命周期事件，在四个
同心圆环中显示任务状态，并提供手动绑定、锁定、交换和灯效编辑。当前版本不
包含 USB、固件或实体灯控制。

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

没有探针事件时，应用仍以 `VIRTUAL DEVICE` 启动，不会尝试连接 USB。浏览器
中的 `npm run dev` 使用确定性的演示 bridge；`npm run tauri dev` 使用本地
Tauri bridge。

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
