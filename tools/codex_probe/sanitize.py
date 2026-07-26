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
