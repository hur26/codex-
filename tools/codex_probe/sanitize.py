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


def _walk(
    value: Any,
    path: str,
    shape: list[dict],
    identities: list[dict],
    *,
    depth: int = 0,
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
        keys = sorted(value, key=_safe_key_label)
        if len(keys) > MAX_CONTAINER_ITEMS:
            item["truncated"] = True
        for key in keys[:MAX_CONTAINER_ITEMS]:
            if len(shape) >= MAX_NODES:
                item["truncated"] = True
                break
            child = value[key]
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
    _walk(payload, "$", shape, identities)

    hook_type = payload.get("hook_type", payload.get("type", "unknown"))
    tool_name = payload.get("tool_name", payload.get("tool", ""))

    return {
        "schema_version": 1,
        "received_at": timestamp.isoformat(),
        "hook_type": hook_type if isinstance(hook_type, str) else "unknown",
        "tool_name": tool_name if isinstance(tool_name, str) else "",
        "top_level_keys": sorted(_safe_key_label(key) for key in payload),
        "identity_candidates": identities,
        "shape": shape,
    }
