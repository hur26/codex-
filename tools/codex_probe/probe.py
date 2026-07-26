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
