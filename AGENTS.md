# Codex Halo Development Handoff

## Project Background

- Codex Halo is developing a desktop-to-hardware status device. The current phase is the USB device foundation and hardware pre-development work.
- The active implementation plan is `docs/plans/2026-07-29-usb-device-foundation.md`.
- The approved design is `docs/plans/2026-07-29-usb-device-foundation-design.md`.
- The USB v0.1 protocol specification is `docs/protocol/codex-halo-usb-v0.1.md`.
- Shared cross-language protocol vectors are in `docs/protocol/golden-vectors.tsv`.
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

- Branch: `main`.
- Latest completed task: Task 10 (`固件：实现设备状态机与安全适配层`). Use `git log -1` for its commit hash.
- After the Task 10 commit, the branch is five commits ahead of `origin/main`; do not push until Task 12 and all final gates pass.
- Tasks 1-10 are implemented, independently reviewed, tested, and committed.
- Task 10 implements the firmware device controller, fixed v0.1 state payload validation, null HAL interfaces, non-blocking USB CDC main loop, 3 s watchdog, default `maxMilliAmps=500`, and `brightnessCeiling=30`.
- Task 10 decoder behavior is split between generic protocol decoding, which preserves valid half frames up to 512-byte payloads, and firmware `StrictV01` mode, which rejects impossible fixed v0.1 payload lengths before they can stall recovery.
- Task 10 TX uses a fixed `TxPump` with 8 frame slots of 19 bytes each, `Serial.setTxTimeoutMs(0)`, full-frame backpressure, short-write retry, and pending decoded input so state writes are not applied before their ACK/NACK can be queued.
- Task 10 native firmware tests pass 38/38 across protocol, state, and HAL queues. They cover low-revision full snapshot overwrite, strict length recovery, generic max-frame retention with nested frame bytes, CRC resynchronization, unsupported version safety, and TX queue backpressure.
- Task 10 target build `waveshare_amoled_143` passes with PlatformIO under `D:\DevTools\PlatformIO`.
- Task 10 independent specification review: PASS.
- Task 10 independent code-quality review: PASS.
- Next planned work: Task 11 BOM/wiring/power documentation, then Task 12 full no-hardware end-to-end gates and verification report.
- After Task 12 passes, push `main` and send the user a concise Chinese development summary.

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
