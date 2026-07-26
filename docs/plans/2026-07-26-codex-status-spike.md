# Codex Status Identity Spike Implementation Plan

> **For implementer:** Use TDD throughout. Write failing test first. Watch it fail. Then implement.

**Goal:** Produce privacy-safe runtime evidence showing whether Codex desktop lifecycle events contain a stable identity that can distinguish at least two concurrent tasks.

**Architecture:** Build a disposable Python 3.11 probe using only the standard library. A hook process reads one JSON event from stdin, removes all values except approved event metadata, fingerprints candidate identity values, and writes one uniquely named JSON file to a local spool directory. A separate analyzer summarizes candidate identity paths without exposing prompts, replies, code, commands, or file contents.

**Tech Stack:** Python 3.11 standard library, `unittest`, Codex lifecycle hooks, JSON/JSONL evidence files.

---

## Safety gate

This spike must finish before scaffolding Tauri, buying the complete hardware set, or changing the global indicator architecture.

The probe must never persist:

- Prompt or reply text.
- Tool arguments.
- Commands.
- Code or file contents.
- Environment variables.
- Authentication data.

The hook installer must preserve existing hooks, default to dry-run, create a timestamped backup before applying, and require a separate `--apply` invocation.

## Task 1: Privacy-safe event summarizer

**Files:**

- Create: `tools/codex_probe/__init__.py`
- Create: `tools/codex_probe/sanitize.py`
- Create: `tests/__init__.py`
- Test: `tests/test_sanitize.py`

**Step 1: Write the failing test**

```python
# tests/test_sanitize.py
import json
import unittest
from datetime import datetime, timezone

from tools.codex_probe.sanitize import summarize_event


class SummarizeEventTests(unittest.TestCase):
    def test_redacts_values_and_fingerprints_candidate_ids(self):
        payload = {
            "hook_type": "PreToolUse",
            "tool_name": "exec_command",
            "thread_id": "thread-secret-123",
            "cwd": r"D:\Private\Client",
            "arguments": {
                "cmd": "print the production token",
                "content": "SECRET-CODE-CONTENT",
            },
            "env": {"SECRET_KEY_NAME": "SECRET_VALUE"},
            "message": "private prompt",
        }

        summary = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 0, tzinfo=timezone.utc),
        )
        serialized = json.dumps(summary, ensure_ascii=False)

        self.assertEqual(summary["hook_type"], "PreToolUse")
        self.assertEqual(summary["tool_name"], "exec_command")
        self.assertEqual(summary["top_level_keys"], sorted(payload))
        self.assertEqual(summary["received_at"], "2026-07-26T08:00:00+00:00")
        self.assertNotIn("thread-secret-123", serialized)
        self.assertNotIn(r"D:\Private\Client", serialized)
        self.assertNotIn("SECRET-CODE-CONTENT", serialized)
        self.assertNotIn("private prompt", serialized)
        self.assertNotIn("production token", serialized)
        self.assertNotIn("SECRET_KEY_NAME", serialized)
        self.assertNotIn("SECRET_VALUE", serialized)

        identities = {item["path"]: item for item in summary["identity_candidates"]}
        self.assertIn("$.thread_id", identities)
        self.assertIn("$.cwd", identities)
        self.assertRegex(identities["$.thread_id"]["fingerprint"], r"^[0-9a-f]{16}$")
        self.assertEqual(identities["$.thread_id"]["length"], 17)

    def test_records_shape_without_scalar_values(self):
        summary = summarize_event(
            {"hook_type": "Stop", "nested": {"items": [1, "secret"]}},
            received_at=datetime(2026, 7, 26, 8, 1, tzinfo=timezone.utc),
        )

        paths = {item["path"]: item["kind"] for item in summary["shape"]}
        self.assertEqual(paths["$"], "object")
        self.assertEqual(paths["$.nested"], "object")
        self.assertEqual(paths["$.nested.items"], "array")
        self.assertEqual(paths["$.nested.items[0]"], "number")
        self.assertEqual(paths["$.nested.items[1]"], "string")


if __name__ == "__main__":
    unittest.main()
```

**Step 2: Run the test and confirm it fails**

Command:

```powershell
python -m unittest tests.test_sanitize -v
```

Expected: `ERROR` with `ModuleNotFoundError: No module named 'tools.codex_probe.sanitize'`.

**Step 3: Write the minimal implementation**

```python
# tools/codex_probe/__init__.py
"""Privacy-safe Codex lifecycle probe."""
```

```python
# tools/codex_probe/sanitize.py
from __future__ import annotations

import hashlib
from datetime import datetime, timezone
from typing import Any


IDENTITY_KEY_PARTS = {
    "thread_id",
    "threadid",
    "task_id",
    "taskid",
    "conversation_id",
    "conversationid",
    "session_id",
    "sessionid",
    "turn_id",
    "turnid",
    "cwd",
    "project_id",
    "projectid",
}

SENSITIVE_KEY_PARTS = {
    "args",
    "arguments",
    "cmd",
    "code",
    "command",
    "content",
    "env",
    "environment",
    "input",
    "message",
    "messages",
    "output",
    "prompt",
    "text",
}


def _kind(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, dict):
        return "object"
    if isinstance(value, list):
        return "array"
    if isinstance(value, (int, float)):
        return "number"
    return "string"


def _fingerprint(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8", errors="replace")).hexdigest()[:16]


def _walk(value: Any, path: str, shape: list[dict], identities: list[dict]) -> None:
    kind = _kind(value)
    item = {"path": path, "kind": kind}
    if kind == "string":
        item["length"] = len(str(value))
    shape.append(item)

    if isinstance(value, dict):
        for key in sorted(value):
            child = value[key]
            child_path = f"{path}.{key}"
            normalized_key = str(key).lower()
            if normalized_key in SENSITIVE_KEY_PARTS:
                shape.append(
                    {
                        "path": child_path,
                        "kind": _kind(child),
                        "redacted": True,
                    }
                )
                continue
            if normalized_key in IDENTITY_KEY_PARTS and child is not None:
                text = str(child)
                identities.append(
                    {
                        "path": child_path,
                        "kind": _kind(child),
                        "length": len(text),
                        "fingerprint": _fingerprint(text),
                    }
                )
            _walk(child, child_path, shape, identities)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _walk(child, f"{path}[{index}]", shape, identities)


def summarize_event(
    payload: dict[str, Any],
    *,
    received_at: datetime | None = None,
) -> dict[str, Any]:
    timestamp = received_at or datetime.now(timezone.utc)
    shape: list[dict] = []
    identities: list[dict] = []
    _walk(payload, "$", shape, identities)

    hook_type = payload.get("hook_type", payload.get("type", "unknown"))
    tool_name = payload.get("tool_name", payload.get("tool", ""))

    return {
        "schema_version": 1,
        "received_at": timestamp.isoformat(),
        "hook_type": hook_type if isinstance(hook_type, str) else "unknown",
        "tool_name": tool_name if isinstance(tool_name, str) else "",
        "top_level_keys": sorted(str(key) for key in payload),
        "identity_candidates": identities,
        "shape": shape,
    }
```

Create an empty `tests/__init__.py`.

**Step 4: Run the test and confirm it passes**

Command:

```powershell
python -m unittest tests.test_sanitize -v
```

Expected: `Ran 2 tests ... OK`.

**Step 5: Commit**

```powershell
git add tools/codex_probe/__init__.py tools/codex_probe/sanitize.py tests/__init__.py tests/test_sanitize.py
git commit -m "test: add privacy-safe Codex event summarizer"
```

## Task 2: Append-only hook probe

**Files:**

- Create: `tools/codex_probe/probe.py`
- Test: `tests/test_probe.py`

**Step 1: Write the failing test**

```python
# tests/test_probe.py
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class ProbeCliTests(unittest.TestCase):
    def test_writes_one_sanitized_event_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            payload = {
                "hook_type": "PreToolUse",
                "thread_id": "stable-thread-id",
                "arguments": {"cmd": "SECRET COMMAND"},
            }

            result = subprocess.run(
                [sys.executable, "-m", "tools.codex_probe.probe"],
                input=json.dumps(payload),
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            files = list(Path(temp_dir).glob("*.json"))
            self.assertEqual(len(files), 1)
            saved = files[0].read_text(encoding="utf-8")
            self.assertNotIn("stable-thread-id", saved)
            self.assertNotIn("SECRET COMMAND", saved)
            self.assertEqual(json.loads(saved)["hook_type"], "PreToolUse")

    def test_direct_script_invocation_works_outside_repository(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            script = (
                Path(__file__).resolve().parents[1]
                / "tools"
                / "codex_probe"
                / "probe.py"
            )
            result = subprocess.run(
                [sys.executable, str(script)],
                input=json.dumps({"hook_type": "Stop", "thread_id": "thread-a"}),
                text=True,
                capture_output=True,
                cwd=temp_dir,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(list(Path(temp_dir).glob("*.json"))), 1)

    def test_invalid_json_exits_successfully_without_writing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            result = subprocess.run(
                [sys.executable, "-m", "tools.codex_probe.probe"],
                input="{not-json",
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0)
            self.assertEqual(list(Path(temp_dir).glob("*.json")), [])


if __name__ == "__main__":
    unittest.main()
```

**Step 2: Run the test and confirm it fails**

Command:

```powershell
python -m unittest tests.test_probe -v
```

Expected: `FAIL` because `tools.codex_probe.probe` does not exist and no event file is written.

**Step 3: Write the minimal implementation**

```python
# tools/codex_probe/probe.py
from __future__ import annotations

import json
import os
import sys
import time
import uuid
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.codex_probe.sanitize import summarize_event


def main() -> int:
    try:
        raw = sys.stdin.read()
        payload = json.loads(raw)
        if not isinstance(payload, dict):
            return 0
    except Exception:
        return 0

    spool_dir = Path(
        os.environ.get(
            "CODEX_HALO_PROBE_DIR",
            str(Path.home() / ".codex-halo" / "probe"),
        )
    )
    try:
        spool_dir.mkdir(parents=True, exist_ok=True)
        filename = f"{time.time_ns()}-{os.getpid()}-{uuid.uuid4().hex}.json"
        target = spool_dir / filename
        target.write_text(
            json.dumps(summarize_event(payload), ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
    except Exception:
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

**Step 4: Run the tests and confirm they pass**

Command:

```powershell
python -m unittest tests.test_probe -v
```

Expected: `Ran 3 tests ... OK`.

**Step 5: Commit**

```powershell
git add tools/codex_probe/probe.py tests/test_probe.py
git commit -m "feat: add append-only Codex hook probe"
```

## Task 3: Evidence analyzer

**Files:**

- Create: `tools/codex_probe/analyze.py`
- Test: `tests/test_analyze.py`

**Security amendment discovered during review:**

- A valid fingerprint is the sanitizer's actual output format: exactly 16 lowercase hexadecimal characters. Tests must use values such as `aaaaaaaaaaaaaaaa` and `bbbbbbbbbbbbbbbb`, not placeholder strings such as `aaa` or `bbb`.
- Treat spool files as untrusted even though the probe normally creates them. Reject non-string hook types, paths, and fingerprints rather than coercing them with `str()`.
- Bound file count, file bytes, candidates per event, identity paths, and distinct fingerprints. For inputs within the limits, preserve the aggregation behavior below. When a limit is exceeded, return a truncation/skipped diagnostic instead of processing without bound.
- Sort the bounded file sample before analysis. When the directory exceeds the configured file limit, set an explicit `file_limit_reached` flag; processing every file is not required.
- Publish CLI reports atomically through a same-directory temporary file and `os.replace()`.

**Step 1: Write the failing test**

```python
# tests/test_analyze.py
import json
import tempfile
import unittest
from pathlib import Path

from tools.codex_probe.analyze import analyze_directory


class AnalyzeDirectoryTests(unittest.TestCase):
    def test_groups_identity_fingerprints_by_path(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            events = [
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": [
                        {
                            "path": "$.thread_id",
                            "fingerprint": "aaaaaaaaaaaaaaaa",
                            "length": 10,
                        }
                    ],
                },
                {
                    "hook_type": "PostToolUse",
                    "identity_candidates": [
                        {
                            "path": "$.thread_id",
                            "fingerprint": "aaaaaaaaaaaaaaaa",
                            "length": 10,
                        }
                    ],
                },
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": [
                        {
                            "path": "$.thread_id",
                            "fingerprint": "bbbbbbbbbbbbbbbb",
                            "length": 10,
                        }
                    ],
                },
            ]
            for index, event in enumerate(events):
                (root / f"{index}.json").write_text(
                    json.dumps(event),
                    encoding="utf-8",
                )

            report = analyze_directory(root)

            self.assertEqual(report["event_count"], 3)
            candidate = report["identity_paths"]["$.thread_id"]
            self.assertEqual(candidate["distinct_fingerprints"], 2)
            self.assertEqual(candidate["events"], 3)
            self.assertEqual(
                candidate["hook_types"],
                {"PostToolUse": 1, "PreToolUse": 2},
            )


if __name__ == "__main__":
    unittest.main()
```

**Step 2: Run the test and confirm it fails**

Command:

```powershell
python -m unittest tests.test_analyze -v
```

Expected: `ERROR` with `ModuleNotFoundError: No module named 'tools.codex_probe.analyze'`.

**Step 3: Write the minimal implementation**

```python
# tools/codex_probe/analyze.py
from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


def analyze_directory(root: Path) -> dict[str, Any]:
    event_count = 0
    fingerprints: dict[str, set[str]] = defaultdict(set)
    event_totals: Counter[str] = Counter()
    hooks_by_path: dict[str, Counter[str]] = defaultdict(Counter)

    for path in sorted(root.glob("*.json")):
        try:
            event = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        if not isinstance(event, dict):
            continue

        event_count += 1
        hook_type = str(event.get("hook_type", "unknown"))
        event_totals[hook_type] += 1

        for candidate in event.get("identity_candidates", []):
            if not isinstance(candidate, dict):
                continue
            candidate_path = str(candidate.get("path", ""))
            fingerprint = str(candidate.get("fingerprint", ""))
            if not candidate_path or not fingerprint:
                continue
            fingerprints[candidate_path].add(fingerprint)
            hooks_by_path[candidate_path][hook_type] += 1

    identity_paths = {}
    for candidate_path in sorted(fingerprints):
        identity_paths[candidate_path] = {
            "distinct_fingerprints": len(fingerprints[candidate_path]),
            "events": sum(hooks_by_path[candidate_path].values()),
            "hook_types": dict(sorted(hooks_by_path[candidate_path].items())),
        }

    return {
        "schema_version": 1,
        "event_count": event_count,
        "hook_types": dict(sorted(event_totals.items())),
        "identity_paths": identity_paths,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    report = analyze_directory(args.dir)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

**Step 4: Run the tests and confirm they pass**

Command:

```powershell
python -m unittest tests.test_analyze -v
```

Expected: `Ran 1 test ... OK`.

**Step 5: Commit**

```powershell
git add tools/codex_probe/analyze.py tests/test_analyze.py
git commit -m "feat: summarize Codex identity evidence"
```

## Task 4: Non-destructive Codex hook installer

**Files:**

- Create: `tools/codex_probe/install_hooks.py`
- Test: `tests/test_install_hooks.py`

**Step 1: Write the failing test**

```python
# tests/test_install_hooks.py
import unittest

from tools.codex_probe.install_hooks import merge_hooks


class MergeHooksTests(unittest.TestCase):
    def test_preserves_existing_hooks_and_is_idempotent(self):
        existing = {
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "",
                        "hooks": [
                            {"type": "command", "command": "python old-hook.py"}
                        ],
                    }
                ]
            }
        }
        command = '"C:\\Python\\python.exe" -m tools.codex_probe.probe'

        first = merge_hooks(existing, command)
        second = merge_hooks(first, command)

        self.assertEqual(first, second)
        self.assertEqual(
            first["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "python old-hook.py",
        )
        for event_name in ("PreToolUse", "PostToolUse", "Stop", "Notification"):
            commands = [
                hook["command"]
                for entry in first["hooks"][event_name]
                for hook in entry.get("hooks", [])
            ]
            self.assertEqual(commands.count(command), 1)


if __name__ == "__main__":
    unittest.main()
```

**Step 2: Run the test and confirm it fails**

Command:

```powershell
python -m unittest tests.test_install_hooks -v
```

Expected: `ERROR` with `ModuleNotFoundError`.

**Step 3: Write the minimal implementation**

```python
# tools/codex_probe/install_hooks.py
from __future__ import annotations

import argparse
import copy
import json
from datetime import datetime
from pathlib import Path
from typing import Any


EVENT_NAMES = ("PreToolUse", "PostToolUse", "Stop", "Notification")


def _contains_command(entries: list[dict[str, Any]], command: str) -> bool:
    return any(
        hook.get("type") == "command" and hook.get("command") == command
        for entry in entries
        for hook in entry.get("hooks", [])
        if isinstance(hook, dict)
    )


def merge_hooks(config: dict[str, Any], command: str) -> dict[str, Any]:
    result = copy.deepcopy(config)
    hooks = result.setdefault("hooks", {})
    for event_name in EVENT_NAMES:
        entries = hooks.setdefault(event_name, [])
        if _contains_command(entries, command):
            continue
        entry: dict[str, Any] = {
            "hooks": [{"type": "command", "command": command}]
        }
        if event_name in {"PreToolUse", "PostToolUse"}:
            entry["matcher"] = ""
        entries.append(entry)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--command", required=True)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()

    current = json.loads(args.config.read_text(encoding="utf-8"))
    merged = merge_hooks(current, args.command)

    if not args.apply:
        print(json.dumps(merged, ensure_ascii=False, indent=2))
        return 0

    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    backup = args.config.with_name(f"{args.config.name}.bak.{stamp}")
    backup.write_text(
        json.dumps(current, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    args.config.write_text(
        json.dumps(merged, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    print(str(backup))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

**Step 4: Run the tests and confirm they pass**

Command:

```powershell
python -m unittest tests.test_install_hooks -v
```

Expected: `Ran 1 test ... OK`.

**Step 5: Run the complete test suite**

Command:

```powershell
python -m unittest discover -s tests -v
```

Expected: all seven tests pass.

**Step 6: Commit**

```powershell
git add tools/codex_probe/install_hooks.py tests/test_install_hooks.py
git commit -m "feat: add safe Codex hook installer"
```

## Task 5: Perform a dry-run hook installation

**Files:**

- Modify only after explicit confirmation: `%USERPROFILE%\.codex\hooks.json`
- Backup created by the installer: `%USERPROFILE%\.codex\hooks.json.bak.<timestamp>`

**Step 1: Build the exact hook command**

From the repository root:

```powershell
$pythonPath = (Get-Command python).Source
$probeRoot = (Get-Location).Path
$hookCommand = "`"$pythonPath`" -m tools.codex_probe.probe"
$hookCommand
```

Expected: an absolute Python executable path followed by `-m tools.codex_probe.probe`.

Before applying, ensure Codex launches hook commands with `D:\Project\codex-halo` as the working directory. If it does not, replace the module invocation with an absolute script invocation:

```powershell
$probeScript = Join-Path $probeRoot 'tools\codex_probe\probe.py'
$hookCommand = "`"$pythonPath`" `"$probeScript`""
```

**Step 2: Preview the merged configuration**

```powershell
python -m tools.codex_probe.install_hooks `
  --config "$env:USERPROFILE\.codex\hooks.json" `
  --command $hookCommand
```

Expected:

- Existing hook commands are still present.
- One new probe command appears under each of `PreToolUse`, `PostToolUse`, `Stop`, and `Notification`.
- The real configuration file is unchanged.

**Step 3: Stop for explicit approval**

Show the user the dry-run summary and exact target path. Do not apply until the user confirms the global hook change.

**Step 4: Apply after approval**

```powershell
python -m tools.codex_probe.install_hooks `
  --config "$env:USERPROFILE\.codex\hooks.json" `
  --command $hookCommand `
  --apply
```

Expected:

- The command prints the timestamped backup path.
- Existing hook entries remain intact.
- The new probe hook is present exactly once per event.

**Step 5: Verify the backup and config**

```powershell
Get-ChildItem -LiteralPath "$env:USERPROFILE\.codex" -Filter 'hooks.json.bak.*' |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1 FullName,Length,LastWriteTime

Get-Content -LiteralPath "$env:USERPROFILE\.codex\hooks.json" -Raw |
  ConvertFrom-Json |
  Select-Object -ExpandProperty hooks
```

Expected: a new non-empty backup and all original plus new hook groups.

Do not commit files outside the repository.

## Task 6: Capture a single-task lifecycle

**Files:**

- Generated local evidence, not committed raw: `%USERPROFILE%\.codex-halo\probe\*.json`
- Create: `docs/research/2026-07-26-single-task-analysis.json`
- Create: `docs/research/2026-07-26-codex-status-evidence.md`

**Step 1: Clear only the exact probe spool after verifying the path**

Resolve and verify:

```powershell
$probeDir = Join-Path $env:USERPROFILE '.codex-halo\probe'
$resolvedParent = (Resolve-Path -LiteralPath (Split-Path $probeDir -Parent)).Path
$probeDir
$resolvedParent
```

The target must end with `.codex-halo\probe`. If it does not, stop.

Move any existing event files into a timestamped archive instead of deleting them.

**Step 2: Start a fresh Codex task after restarting Codex if required**

Use a harmless request that causes:

- At least one read-only tool call.
- At least one normal response completion.
- No sensitive prompt, file, or environment content.

**Step 3: Confirm event files were created**

```powershell
Get-ChildItem -LiteralPath $probeDir -Filter '*.json' |
  Sort-Object LastWriteTime |
  Select-Object Name,Length,LastWriteTime
```

Expected: at least one lifecycle event file.

**Step 4: Generate the sanitized analysis**

```powershell
python -m tools.codex_probe.analyze `
  --dir $probeDir `
  --output 'docs/research/2026-07-26-single-task-analysis.json'
```

Expected: report contains a non-zero `event_count` and lists any candidate identity paths.

**Step 5: Write the evidence note**

Create `docs/research/2026-07-26-codex-status-evidence.md` with:

```markdown
# Codex Status Evidence

## Environment

- Codex app version:
- Windows version:
- Probe commit:

## Single-task result

- Captured hook types:
- Candidate identity paths:
- Candidate consistency:
- Missing lifecycle states:

## Privacy verification

- Raw prompts stored: no
- Tool arguments stored: no
- Code or file contents stored: no

## Current conclusion

Pending two-task comparison.
```

**Step 6: Commit only sanitized evidence**

First inspect both files and confirm they contain no raw values or sensitive text.

```powershell
git add docs/research/2026-07-26-single-task-analysis.json docs/research/2026-07-26-codex-status-evidence.md
git commit -m "docs: record single-task Codex hook evidence"
```

Never commit `%USERPROFILE%\.codex-halo\probe`.

## Task 7: Capture and compare two concurrent tasks

**Files:**

- Create: `docs/research/2026-07-26-two-task-analysis.json`
- Modify: `docs/research/2026-07-26-codex-status-evidence.md`

**Step 1: Prepare two harmless tasks**

The user must explicitly create or start two separate Codex tasks. Do not create user-owned Codex tasks without that request.

Each task should:

- Use a distinct project or harmless label.
- Perform at least two tool events.
- Overlap in execution time.
- Avoid secrets and private file contents.

**Step 2: Record the two task windows**

Write down start and end timestamps for Task A and Task B. Do not place task prompt contents in the evidence note.

**Step 3: Generate the combined sanitized analysis**

```powershell
python -m tools.codex_probe.analyze `
  --dir $probeDir `
  --output 'docs/research/2026-07-26-two-task-analysis.json'
```

Expected: one or more candidate paths show two distinct fingerprints.

**Step 4: Evaluate the identity gate**

PASS only if all are true:

- The same candidate fingerprint remains stable across lifecycle events for Task A.
- A different fingerprint remains stable across lifecycle events for Task B.
- At least `PreToolUse` and `PostToolUse` or equivalent activity events carry the candidate.
- Normal completion or stop can be attributed to the correct task.
- No raw content is required to perform the mapping.

FAIL if:

- No candidate identity exists.
- Candidate identity changes within one task without a documented relationship.
- Stop/completion cannot be attributed.
- Differentiation requires storing prompts, code, or tool arguments.

**Step 5: Update the evidence note**

Append:

```markdown
## Two-task result

- Candidate selected:
- Stable within Task A:
- Stable within Task B:
- Distinct between tasks:
- Completion attributable:
- Gate result: PASS | FAIL

## Next adapter decision

- PASS: define the production hook adapter contract around the validated field.
- FAIL: do not build automatic four-ring binding yet; plan a task-list/app-server identity spike.
```

**Step 6: Commit**

```powershell
git add docs/research/2026-07-26-two-task-analysis.json docs/research/2026-07-26-codex-status-evidence.md
git commit -m "docs: decide Codex multi-task identity gate"
```

## Task 8: Close the spike and write the next plan

**Files:**

- Modify: `docs/plans/2026-07-26-codex-halo-design.md`
- Create only after a PASS: `docs/plans/<date>-desktop-domain-and-virtual-device.md`
- Create only after a FAIL: `docs/plans/<date>-codex-task-inventory-spike.md`

**Step 1: Run all probe tests**

```powershell
python -m unittest discover -s tests -v
```

Expected: all tests pass.

**Step 2: Check repository state**

```powershell
git status --short
git log --oneline --decorate -8
```

Expected: no raw probe events are staged or tracked.

**Step 3: Update the design with verified facts only**

Replace unresolved wording in the Codex status section with:

- The exact verified lifecycle event names.
- The selected stable identity field, if any.
- Known missing states.
- The fallback investigation required if the gate failed.

**Step 4: Commit the verified design update**

```powershell
git add docs/plans/2026-07-26-codex-halo-design.md
git commit -m "docs: update design with Codex status evidence"
```

**Step 5: Write the next implementation plan**

- On PASS: plan the domain state machine, four-ring binding engine, virtual device, and Tauri prerequisites.
- On FAIL: plan the narrowest read-only task inventory/app-server investigation.

Do not install Rust, scaffold Tauri, or buy hardware until this spike has a documented result.
