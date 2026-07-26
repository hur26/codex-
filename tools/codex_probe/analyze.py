from __future__ import annotations

import argparse
import itertools
import json
import os
import re
import uuid
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


MAX_FILES = 128
MAX_FILE_BYTES = 1024 * 1024
MAX_CANDIDATES_PER_EVENT = 128
MAX_IDENTITY_PATHS = 64
MAX_DISTINCT_FINGERPRINTS_PER_PATH = 64
MAX_HOOK_TYPE_LENGTH = 64
MAX_IDENTITY_PATH_LENGTH = 512

SAFE_PATH_LABELS = frozenset(
    {
        "args",
        "arguments",
        "cmd",
        "code",
        "command",
        "command_args",
        "content",
        "conversation_id",
        "conversationid",
        "cwd",
        "env",
        "environment",
        "hook_type",
        "input",
        "items",
        "message",
        "messages",
        "nested",
        "output",
        "project_id",
        "projectid",
        "prompt",
        "prompt_text",
        "session_id",
        "sessionid",
        "task_id",
        "taskid",
        "text",
        "thread_id",
        "threadid",
        "tool",
        "tool_input",
        "tool_name",
        "tool_output",
        "turn_id",
        "turnid",
        "type",
    }
)

_HOOK_TYPE_PATTERN = re.compile(r"[A-Za-z][A-Za-z0-9_.:-]*")
_HASHED_PATH_LABEL_PATTERN = re.compile(r"key_[0-9a-f]{16}")
_LIST_INDEX_PATTERN = re.compile(r"(?:0|[1-9][0-9]*)")
_FINGERPRINT_PATTERN = re.compile(r"[0-9a-f]{16}")


def _safe_hook_type(value: Any) -> str:
    if (
        isinstance(value, str)
        and len(value) <= MAX_HOOK_TYPE_LENGTH
        and _HOOK_TYPE_PATTERN.fullmatch(value)
    ):
        return value
    return "unknown"


def _is_safe_identity_path(value: Any) -> bool:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > MAX_IDENTITY_PATH_LENGTH
        or not value.startswith("$")
    ):
        return False

    position = 1
    while position < len(value):
        if value[position] == ".":
            end = position + 1
            while end < len(value) and value[end] not in ".[":
                end += 1
            label = value[position + 1 : end]
            if (
                label not in SAFE_PATH_LABELS
                and not _HASHED_PATH_LABEL_PATTERN.fullmatch(label)
            ):
                return False
            position = end
            continue

        if value[position] == "[":
            end = value.find("]", position + 1)
            if end == -1 or not _LIST_INDEX_PATTERN.fullmatch(
                value[position + 1 : end]
            ):
                return False
            position = end + 1
            continue

        return False
    return True


def _is_safe_fingerprint(value: Any) -> bool:
    return isinstance(value, str) and bool(_FINGERPRINT_PATTERN.fullmatch(value))


def _sample_json_files(root: Path) -> tuple[list[Path], bool]:
    sampled = list(
        itertools.islice(root.glob("*.json"), MAX_FILES + 1)
    )
    limit_reached = len(sampled) > MAX_FILES
    paths = [
        path
        for path in sampled[:MAX_FILES]
        if path.is_file() and not path.is_symlink()
    ]
    paths.sort(key=lambda path: path.name)
    return paths, limit_reached


def analyze_directory(root: Path) -> dict[str, Any]:
    event_count = 0
    fingerprints: dict[str, set[str]] = defaultdict(set)
    event_totals: Counter[str] = Counter()
    hooks_by_path: dict[str, Counter[str]] = defaultdict(Counter)
    skipped_files: Counter[str] = Counter()
    invalid_records: Counter[str] = Counter()
    truncated: Counter[str] = Counter()

    paths, file_limit_reached = _sample_json_files(root)
    for path in paths:
        try:
            with path.open("rb") as handle:
                raw = handle.read(MAX_FILE_BYTES + 1)
        except Exception:
            skipped_files["unreadable"] += 1
            continue
        if len(raw) > MAX_FILE_BYTES:
            skipped_files["oversized"] += 1
            continue
        try:
            event = json.loads(raw)
        except Exception:
            skipped_files["malformed"] += 1
            continue
        if not isinstance(event, dict):
            skipped_files["non_object"] += 1
            continue

        event_count += 1
        hook_type_value = event.get("hook_type", "unknown")
        hook_type = _safe_hook_type(hook_type_value)
        if hook_type == "unknown" and hook_type_value != "unknown":
            invalid_records["hook_types"] += 1
        event_totals[hook_type] += 1

        candidates = event.get("identity_candidates", [])
        if not isinstance(candidates, list):
            invalid_records["identity_candidate_containers"] += 1
            continue
        if len(candidates) > MAX_CANDIDATES_PER_EVENT:
            truncated["candidates"] += len(candidates) - MAX_CANDIDATES_PER_EVENT

        for index, candidate in enumerate(candidates):
            if index >= MAX_CANDIDATES_PER_EVENT:
                break
            if not isinstance(candidate, dict):
                invalid_records["candidates"] += 1
                continue
            candidate_path = candidate.get("path")
            fingerprint = candidate.get("fingerprint")
            if not _is_safe_identity_path(candidate_path):
                invalid_records["identity_paths"] += 1
                continue
            if not _is_safe_fingerprint(fingerprint):
                invalid_records["fingerprints"] += 1
                continue

            if (
                candidate_path not in fingerprints
                and len(fingerprints) >= MAX_IDENTITY_PATHS
            ):
                truncated["identity_paths"] += 1
                continue

            known_fingerprints = fingerprints[candidate_path]
            if (
                fingerprint not in known_fingerprints
                and len(known_fingerprints)
                >= MAX_DISTINCT_FINGERPRINTS_PER_PATH
            ):
                truncated["fingerprints"] += 1
            else:
                known_fingerprints.add(fingerprint)
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
        "file_limit_reached": file_limit_reached,
        "skipped_files": {
            key: skipped_files[key]
            for key in ("unreadable", "oversized", "malformed", "non_object")
        },
        "invalid_records": {
            key: invalid_records[key]
            for key in (
                "hook_types",
                "identity_candidate_containers",
                "candidates",
                "identity_paths",
                "fingerprints",
            )
        },
        "truncated": {
            key: truncated[key]
            for key in ("candidates", "identity_paths", "fingerprints")
        },
    }


def write_report_atomic(output: Path, report: dict[str, Any]) -> None:
    temporary_target: Path | None = None
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary_target = output.parent / (
            f".{output.name}.{os.getpid()}-{uuid.uuid4().hex}.tmp"
        )
        serialized = json.dumps(report, ensure_ascii=False, indent=2)
        with temporary_target.open("x", encoding="utf-8") as handle:
            handle.write(serialized)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_target, output)
    except Exception:
        if temporary_target is not None:
            try:
                temporary_target.unlink(missing_ok=True)
            except Exception:
                pass
        raise


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    report = analyze_directory(args.dir)
    write_report_atomic(args.output, report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
