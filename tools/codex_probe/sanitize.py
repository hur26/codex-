from __future__ import annotations

import hashlib
from datetime import datetime, timezone
from typing import Any

from tools.codex_probe.events import HOOK_EVENT_NAMES


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
    "command_args",
    "content",
    "env",
    "environment",
    "input",
    "message",
    "messages",
    "output",
    "prompt",
    "prompt_text",
    "text",
    "tool_input",
    "tool_output",
}

SAFE_KEY_PARTS = IDENTITY_KEY_PARTS | SENSITIVE_KEY_PARTS | {
    "hook_event_name",
    "hook_type",
    "items",
    "nested",
    "tool",
    "tool_name",
    "type",
}

MAX_DEPTH = 32
MAX_NODES = 512
MAX_CONTAINER_ITEMS = 128


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


def _safe_key_label(key: Any) -> str:
    normalized_key = str(key).lower()
    if normalized_key in SAFE_KEY_PARTS:
        return normalized_key
    return f"key_{_fingerprint(str(key))}"


def _sample_dict_entries(value: dict) -> list[tuple[Any, Any]]:
    priority_entries: dict[str, tuple[Any, Any]] = {}
    ordinary_entries: list[tuple[Any, Any]] = []

    for key, child in value.items():
        normalized_key = str(key).lower()
        if normalized_key in SAFE_KEY_PARTS:
            priority_entries.setdefault(normalized_key, (key, child))
        elif len(ordinary_entries) < MAX_CONTAINER_ITEMS:
            ordinary_entries.append((key, child))

    prioritized = list(priority_entries.values())
    ordinary_limit = MAX_CONTAINER_ITEMS - len(prioritized)
    return prioritized + ordinary_entries[:ordinary_limit]


def _walk(
    value: Any,
    path: str,
    shape: list[dict],
    identities: list[dict],
    *,
    depth: int = 0,
    dict_entries: list[tuple[Any, Any]] | None = None,
) -> bool:
    if len(shape) >= MAX_NODES:
        return False

    kind = _kind(value)
    item = {"path": path, "kind": kind}
    if kind == "string":
        item["length"] = len(str(value))
    shape.append(item)

    if depth >= MAX_DEPTH and isinstance(value, (dict, list)):
        item["truncated"] = True
        return True

    if isinstance(value, dict):
        entries = (
            dict_entries
            if dict_entries is not None
            else _sample_dict_entries(value)
        )
        entries.sort(key=lambda entry: _safe_key_label(entry[0]))
        if len(value) > len(entries):
            item["truncated"] = True
        for key, child in entries:
            if len(shape) >= MAX_NODES:
                item["truncated"] = True
                break
            normalized_key = str(key).lower()
            child_path = f"{path}.{_safe_key_label(key)}"
            if normalized_key in SENSITIVE_KEY_PARTS:
                shape.append(
                    {
                        "path": child_path,
                        "kind": _kind(child),
                        "redacted": True,
                    }
                )
                continue
            if normalized_key in IDENTITY_KEY_PARTS and isinstance(
                child, (str, int, float, bool)
            ):
                text = str(child)
                identities.append(
                    {
                        "path": child_path,
                        "kind": _kind(child),
                        "length": len(text),
                        "fingerprint": _fingerprint(text),
                    }
                )
            if not _walk(
                child,
                child_path,
                shape,
                identities,
                depth=depth + 1,
            ):
                item["truncated"] = True
                break
    elif isinstance(value, list):
        if len(value) > MAX_CONTAINER_ITEMS:
            item["truncated"] = True
        for index, child in enumerate(value[:MAX_CONTAINER_ITEMS]):
            if len(shape) >= MAX_NODES:
                item["truncated"] = True
                break
            if not _walk(
                child,
                f"{path}[{index}]",
                shape,
                identities,
                depth=depth + 1,
            ):
                item["truncated"] = True
                break
    return True


def summarize_event(
    payload: dict[str, Any],
    *,
    received_at: datetime | None = None,
) -> dict[str, Any]:
    timestamp = received_at or datetime.now(timezone.utc)
    shape: list[dict] = []
    identities: list[dict] = []
    top_level_entries = _sample_dict_entries(payload)
    _walk(payload, "$", shape, identities, dict_entries=top_level_entries)
    top_level_key_sample = [key for key, _ in top_level_entries]

    hook_type = "unknown"
    for key in ("hook_event_name", "hook_type", "type"):
        if key not in payload:
            continue
        candidate = payload[key]
        if isinstance(candidate, str) and candidate in HOOK_EVENT_NAMES:
            hook_type = candidate
        break
    tool_name = payload.get("tool_name", payload.get("tool", ""))

    return {
        "schema_version": 1,
        "received_at": timestamp.isoformat(),
        "hook_type": hook_type,
        "tool_name": tool_name if isinstance(tool_name, str) else "",
        "top_level_keys": sorted(
            _safe_key_label(key) for key in top_level_key_sample
        ),
        "top_level_keys_truncated": len(payload) > len(top_level_entries),
        "identity_candidates": identities,
        "shape": shape,
    }
