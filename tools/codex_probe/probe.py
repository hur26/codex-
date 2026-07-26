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


# Lifecycle metadata should be small; reject unexpectedly large hook payloads.
MAX_INPUT_BYTES = 1024 * 1024


def main() -> int:
    try:
        raw = sys.stdin.buffer.read(MAX_INPUT_BYTES + 1)
        if len(raw) > MAX_INPUT_BYTES:
            return 0
        payload = json.loads(raw)
        if not isinstance(payload, dict):
            return 0
    except Exception:
        return 0

    temporary_target: Path | None = None
    try:
        configured_dir = os.environ.get("CODEX_HALO_PROBE_DIR")
        spool_dir = (
            Path(configured_dir)
            if configured_dir is not None
            else Path.home() / ".codex-halo" / "probe"
        )
        serialized = json.dumps(
            summarize_event(payload),
            ensure_ascii=False,
            indent=2,
        )
        spool_dir.mkdir(parents=True, exist_ok=True)
        filename = f"{time.time_ns()}-{os.getpid()}-{uuid.uuid4().hex}.json"
        target = spool_dir / filename
        temporary_target = spool_dir / f"{filename}.tmp"
        with temporary_target.open("x", encoding="utf-8") as handle:
            handle.write(serialized)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_target, target)
    except Exception:
        if temporary_target is not None:
            try:
                temporary_target.unlink(missing_ok=True)
            except Exception:
                pass
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
