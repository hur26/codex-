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

- Branch: `main`, synchronized with `origin/main` at `419f740` (`设备：贯通有限双向诊断路径`). Nothing is unpushed.
- Latest completed task: Task 12 (`验证：记录 USB 设备基础无硬件门禁`), commit `3c6ad7e`, plus worker CRC evidence fix `e33bc4a`, documentation correction `4902003`, protocol-consistency remediation `be93025`, UI diagnostic-safety correction `be86136`, and the finite bidirectional diagnostics implementation `419f740`.
- The finite bidirectional diagnostics implementation and its documentation are committed and pushed. Its first independent specification review found an incompatible-state gate issue; the focused TDD fix subsequently passed independent specification re-review and code-quality review.
- A **second, independent review round** of the same diagnostics work ran on the macOS working copy and is recorded in `docs/research/2026-08-12-diagnostics-independent-review.md`. The Windows environment did not have this round when it committed and pushed `419f740`, so every finding below is still open at `origin/main`. Do not treat `419f740` as closing them.
- Both re-reviews returned **conditional pass**: no correctness defect, no privacy leak, and no violation of the frozen byte layout or v0.1 constants. Four specification findings and six code-quality findings were raised.
- Specification findings S1 and S2 are fixed in the working tree copy of `docs/protocol/codex-halo-usb-v0.1.md` but are **not yet committed**; `origin/main` still carries the pre-fix text. Section 2 now states the normative bidirectional diagnostics rules (silent consume, no ACK, no watchdog refresh, firmware NACKs a malformed diagnostic while the desktop answers with code `0003`, `rejectedMessageType = 81` is legal traffic, sender-side rate limiting), section 3.6 now states that codes are sender-relative and fixes `severity`/`value` per code and direction, and section 4.12 now states that a length, severity, or code mismatch is a format error.
- Specification findings S3 and S4 remain open. S3: `DIAGNOSTICS` still has no shared cross-language golden vector, which the plan's definition of done requires; a candidate row `diagnostics_crc_error 81 0005 02020007000000 4348018105000700020200070000009046` is proposed in the review document and must not be written into the TSV until both the Rust and PlatformIO native suites actually consume it. S4: `DeviceManager` latches the last device diagnostic into `status.message` until reconnect, so a transient device CRC error stays on the UI indefinitely; either document the latch as intended v0.1 behavior or add a TDD-covered clearing rule.
- Code-quality findings Q1-Q6 remain open and were deliberately not edited, because they need Rust and PlatformIO gates that the review environment did not have. Q1: `main.cpp:43 requiresResponse` duplicates the diagnostic-validity rule that `HaloState.cpp:245` also applies. Q2: `manager.rs:507 report_protocol_diagnostic` duplicates the encode/write/reconnect path of `manager.rs:427 begin_request`. Q3: the firmware heartbeat branch writes `type` and `sequence` on a response whose `shouldSend` stays false. Q4: `HaloState.cpp:386` and `:391` index the four diagnostic slots with `code - 1` without a static bound. Q5: `KnobInput` does not document that implementations must latch events until polled, which `main.cpp:122` relies on. Q6: the Rust `TryFrom` impls for `DiagnosticSeverity` and `DiagnosticCode` use `()` as the error type.
- The macOS working copy at `/Users/mac/Desktop/codex-halo` originally had no `.git`; on 2026-08-13 it was re-attached to `https://github.com/hur26/codex-.git` and fast-forwarded to `419f740`. It is configured with `core.autocrlf=input` (the repository stores LF; the Windows checkout produced CRLF) and `core.fileMode=false` (the transfer archive set every file to 0777); without both settings roughly 3000 lines of spurious diff appear. It still has no Rust toolchain, no PlatformIO, and no installed `node_modules`, so only the Python regression can be rerun there: `python3 -m unittest discover -s tests -p 'test_*.py'` gives 64 tests OK, with no skip, because the Windows-only symlink skip does not apply on macOS. Every other gate below reflects the Windows run and was not re-executed on macOS.
- Tasks 1-12 are implemented, tested, and committed. Overall review and its focused quality review found three protocol consistency gaps; all have TDD fixes and passed independent specification and code-quality re-review.
- The approved v0.1 finite bidirectional `DIAGNOSTICS` path is implemented. Rust and C++ share a strict 7-byte typed payload; valid diagnostics are silent and do not create ACK loops; invalid desktop diagnostics are safely NACKed; desktop protocol-error reports are limited to one per manager step and use the existing reconnect path on write failure.
- Firmware keeps four fixed coalescing diagnostic slots rather than an unbounded queue. CRC and malformed counts saturate, watchdog and local-limit reports track edges/latest values, TX backpressure retains pending diagnostics, and diagnostics share the device event sequence with knob events only after successful enqueue.
- Device diagnostic values never reach the UI. Rust maps the four device codes to fixed value-free messages, and Vue's allowlist displays only those sanitized messages while continuing to hide arbitrary text.
- Current desktop gates pass: Python 64 tests (63 passed, 1 Windows symlink skip), Vitest 184/184, TypeScript typecheck, Vite production build, Rust 142/142, Clippy with `-D warnings`, and rustfmt check.
- Current firmware gates pass under `D:\DevTools\PlatformIO`: PlatformIO Core 6.1.19, Espressif32 7.0.1, native firmware tests 47/47, and `waveshare_amoled_143` target build SUCCESS. The incompatible-state gate remediation was followed by focused state 22/22, full native, target, Rust, Vitest, Clippy, and production frontend build reruns.
- The ten required simulated-device scenarios pass: VIRTUAL, handshake, four-ring sanitized snapshot, single-ring delta, short press, rotation wrap, exactly two retries, reconnect full snapshot, CRC recovery through the production worker iteration boundary, and INCOMPATIBLE without state writes.
- Rust and C++ both consume the same four valid rows in `docs/protocol/golden-vectors.tsv` and the same three decoder-error streams in `docs/protocol/decoder-stream-vectors.tsv`; all shared-vector tests pass.
- CRC errors now discard only the current candidate magic before searching later bytes. Both decoders recover multiple consecutive valid frames nested in a failed candidate, normalize the bad outer tail to empty or a legal half magic, and accept the next pushed frame without a false protocol error. Rust retains `Generic` 512-byte frame semantics while production manager and simulator receive paths use `StrictV01` fixed-length rejection, matching firmware.
- Device UI copy is transport-neutral. Non-empty device status messages are visible and accessible only when they exactly match the 20 fixed, sanitized Rust production messages; every unknown message falls back to a constant hidden-diagnostic label, and blank messages add no visual or accessibility noise. The original UI correction and the four new diagnostic messages passed independent specification and code-quality review.
- The privacy scan has seven allowed test-only/negative-assertion matching lines and no task identity or USB serial number in device payloads, device diagnostics, or firmware state.
- Firmware artifact: `firmware/halo-esp32s3/.pio/build/waveshare_amoled_143/firmware.bin`, 281648 bytes, SHA-256 `0C4592FD24A3AB143EB33764BB7170A34F9791F048570221C13B0B2CE32AC209`.
- No physical USB, AMOLED, LED ring, knob, power rail, signal level, thermal behavior, enclosure, or four-ring assembly has been verified. The next hardware step is only the minimum one-ring prototype kit from the BOM.
- Next planned work: commit the S1/S2 protocol corrections and this review record, then close S3, S4, and Q1-Q6 with focused TDD, verify that the S1/S2 protocol-document corrections still match both implementations, rerun the overall final review and the full desktop and firmware gates, push `main`, send the user a concise Chinese development summary, and then prepare the minimum one-ring purchase/assembly session. Development may proceed on either environment, but any task touching Rust or firmware requires the corresponding toolchain to be installed first; see the storage policy note below for macOS.

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
