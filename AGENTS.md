# Codex Halo Development Handoff

## Project Background

- Codex Halo is developing a desktop-to-hardware status device. The current phase is the USB device foundation and hardware pre-development work.
- The active implementation plan is `docs/plans/2026-07-29-usb-device-foundation.md`.
- The approved design is `docs/plans/2026-07-29-usb-device-foundation-design.md`.
- The USB v0.1 protocol specification is `docs/protocol/codex-halo-usb-v0.1.md`.
- Shared cross-language valid-frame vectors are in `docs/protocol/golden-vectors.tsv`.
- Shared cross-language decoder error streams are in `docs/protocol/decoder-stream-vectors.tsv`.
- The verification report is `docs/research/2026-07-29-usb-device-foundation-verification.md` and must be finalized in Task 12.

## Mandatory Workflow

- Follow the `superpowers` workflow strictly: plan-driven implementation, TDD RED/GREEN evidence, independent specification review, independent code-quality review, verification, then commit each task.
- Do not advance past a task with a known specification, quality, test, build, privacy, or hardware-safety gap.
- Debug failures systematically: establish the root cause and a reproducible observation before changing code or environment state.
- Keep changes scoped to the active task. Do not implement a later task early to make an earlier gate pass.
- After each completed task or material handoff point, update the `Current Progress` section in this file before committing.

## Development Storage Policy

- Put project development dependencies, SDKs, toolchains, package caches, and downloaded archives under `D:\DevTools`.
- Do not place project-specific downloads or large temporary development files on C: when a D: location can be configured.
- PlatformIO core directory: `D:\DevTools\PlatformIO`.
- PlatformIO toolchain archives: `D:\DevTools\PlatformIO\archives\toolchains`.
- PlatformIO framework archives: `D:\DevTools\PlatformIO\archives\frameworks`.
- Development temporary directory: `D:\DevTools\Temp`.
- Python package cache: `D:\DevTools\PipCache` (`PIP_CACHE_DIR`).
- uv package cache: `D:\DevTools\UvCache` (`UV_CACHE_DIR`).
- For PlatformIO commands, set `PLATFORMIO_CORE_DIR=D:\DevTools\PlatformIO`, `TEMP=D:\DevTools\Temp`, `TMP=D:\DevTools\Temp`, and `PYTHONUTF8=1`.
- The user-level `PLATFORMIO_CORE_DIR` is set to `D:\DevTools\PlatformIO`, but commands should still set it explicitly for reproducible handoffs.
- Ask before using an unavoidable C: location for any new development dependency or download.

Windows is the only shipping target. macOS is a development-only environment: use it to write code and run gates, but never add macOS adaptation to the product, and treat Windows as the authority for final gates and for the released firmware artifact. On macOS there is a single volume, so the C:/D: split does not apply and toolchains use the default user-level locations:

- rustup / cargo: `~/.rustup` and `~/.cargo`.
- PlatformIO core directory: `~/.platformio` (`PLATFORMIO_CORE_DIR`).
- Cargo build output stays in `apps/desktop/src-tauri/target`.

Check free space before installing: the full set (rustup, a Tauri 2 `target` directory, `node_modules`, and PlatformIO with the Espressif32 platform and Arduino-ESP32 framework) needs roughly 7-9 GB. The `native` PlatformIO environment alone needs only about 200 MB because it reuses the system clang and no Xtensa toolchain.

## PlatformIO Environment

- PlatformIO Core: 6.1.19, invoked with `python -m platformio`.
- Espressif32 platform: 7.0.1.
- Installed package roots are under `D:\DevTools\PlatformIO\packages`.
- Verified Xtensa ESP32-S3 archive SHA-256: `9000be38d44bf79c39b93a2aeb99b42e956c593ccbc02fe31cb9c71ae1bbcb22`.
- Verified RISC-V archive SHA-256: `b08f568e8fe5069dd521b87da21b8e56117e5c2c3b492f73a51966a46d3379a4`.
- Arduino-ESP32 2.0.17 archive expected size: `254658377`; expected SHA-256: `1f8658d4b18a8001ce782142ad08164af2991d70b83a147c3437a6ee30a9b225`.

Use this PowerShell setup before firmware commands:

```powershell
$env:PLATFORMIO_CORE_DIR = 'D:\DevTools\PlatformIO'
$env:TEMP = 'D:\DevTools\Temp'
$env:TMP = 'D:\DevTools\Temp'
$env:PYTHONUTF8 = '1'
$env:PIP_CACHE_DIR = 'D:\DevTools\PipCache'
$env:UV_CACHE_DIR = 'D:\DevTools\UvCache'
$env:PATH = 'D:\DevTools\CLion 2026.1\bin\mingw\bin;' + $env:PATH
```

## Current Progress

- Branch: `main`. The remote review baseline is `origin/main` at `a98cfad`; this handoff change closes the findings discovered after that push.
- Tasks 1-12 remain complete. The merged remote work consists of `7bc4b1a` (diagnostics protocol/review record), `7fd0164` (Windows delivery target), `2fb61b4` (diagnostic-slot bounds), and `a98cfad` (response prediction aligned with handling).
- The second diagnostics review raised S1-S4 and Q1-Q6. S1, Q3, Q4, and Q5 were closed by the merged commits. The Windows remediation closes S2, S3, S4, Q1, Q2, and Q6: receiver validation is explicitly structural and direction-tolerant; the fifth shared golden vector is a 7-byte diagnostic; transient device diagnostics clear on the next successful state ACK or handshake; response prediction and handling share `diagnosticNeedsResponse`; frame writing shares one encode/write/reconnect helper; and Rust diagnostic conversions use typed errors.
- Q4 intentionally has no conventional runtime RED/GREEN claim. The v0.1 public paths cannot construct an out-of-range slot value; the fix is compile-time boundary hardening using an enum sentinel, static assertions, explicit mapping, and refusal branches. The Windows native and target builds compile and exercise the resulting boundary, but the private refusal branches remain unreachable through the public v0.1 API.
- S4's lifecycle is deliberately finite but event-driven: the message clears after the next successful state ACK or handshake. It can remain visible during a period containing only idle heartbeats; do not describe it as time-based expiry.
- Current desktop gates pass on Windows: Python 64 tests (63 passed, 1 Windows symlink skip), Vitest 184/184, TypeScript typecheck, Vite production build, Rust 142/142, Clippy with `-D warnings`, and rustfmt check.
- Current firmware gates pass under `D:\DevTools\PlatformIO`: PlatformIO Core 6.1.19, Espressif32 7.0.1, native tests 52/52 (HAL 6, protocol 19, state 27), and the `waveshare_amoled_143` target build succeeds with RAM 19,764/327,680 bytes and Flash 281,445/6,553,600 bytes.
- Rust and C++ consume the same five valid rows in `docs/protocol/golden-vectors.tsv`, including `diagnostics_crc_error`, and the same three decoder-error streams in `docs/protocol/decoder-stream-vectors.tsv`.
- The ten required simulated-device scenarios still pass: VIRTUAL, handshake, four-ring sanitized snapshot, single-ring delta, short press, rotation wrap, exactly two retries, reconnect full snapshot, CRC recovery through the production worker iteration boundary, and INCOMPATIBLE without state writes.
- The privacy scan has seven allowed test-only or negative-assertion matches. No task identity or USB serial number enters device payloads, diagnostics, firmware state, or user-facing diagnostic text.
- Firmware artifact: `firmware/halo-esp32s3/.pio/build/waveshare_amoled_143/firmware.bin`, 281808 bytes, SHA-256 `DD6C7AF93910EAC7AD2E759B0BA60E9643B31A8DF15CD5FFCA9ABCC360230F96`.
- No physical USB, AMOLED, LED ring, knob, power rail, signal level, thermal behavior, enclosure, or four-ring assembly has been verified. The next development boundary is the minimum one-ring purchase and assembly session from the BOM; no four-ring expansion is authorized by the present evidence.

## Previous Progress Snapshot

- Branch: `main`.
- Latest completed task: Task 9 (`固件：建立 ESP32-S3 协议测试骨架`). Use `git log -1` for its commit hash.
- After the Task 9 commit, the branch is four commits ahead of `origin/main`; do not push until Task 12 and all final gates pass.
- Tasks 1-9 are implemented, independently reviewed, tested, and committed.
- Task 9 native protocol tests pass 15/15. They cover all 11 message types, four shared TSV vectors, CRC, 10/522-byte boundaries, fragmentation, consecutive frames, noise, unknown version/type, oversize handling, CRC recovery, and false `CH + 512` recovery.
- Task 9 independent specification review: PASS.
- Task 9 independent code-quality review: PASS.
- Task 9 dependencies are installed under `D:\DevTools\PlatformIO`; the Arduino-ESP32 2.0.17 archive in `archives\frameworks` matches the official size and SHA-256.
- Task 9 target build exposed and resolved a plan inconsistency: Task 9 requires `platformio run` to pass but originally had no `src` file. A minimal empty Arduino `src/main.cpp` compile probe was added without implementing Task 10 behavior. Task 10 must modify this file rather than create it.
- Task 9 final gates: native protocol tests 15/15 PASS; `waveshare_amoled_143` build SUCCESS; focused specification re-review PASS; focused quality re-review PASS.
- Next planned work: Task 10 firmware state machine and null HAL, Task 11 BOM/wiring/power documentation, Task 12 full no-hardware end-to-end gates and verification report.
- After Task 12 passes, push `main` and send the user a concise Chinese development summary.

## Verification Commands

From `firmware/halo-esp32s3` after applying the environment above:

```powershell
python -m platformio test -e native
python -m platformio run -e waveshare_amoled_143
```

Task 12 full gates are specified in `docs/plans/2026-07-29-usb-device-foundation.md`; use that document as the source of truth rather than copying an outdated command list here.
