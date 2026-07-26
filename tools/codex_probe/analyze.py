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
