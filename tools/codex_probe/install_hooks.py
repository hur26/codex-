from __future__ import annotations

import argparse
import copy
import json
import os
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any


EVENT_NAMES = ("PreToolUse", "PostToolUse", "Stop", "Notification")


def _contains_command(entries: list[Any], command: str) -> bool:
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        nested_hooks = entry.get("hooks", [])
        if not isinstance(nested_hooks, list):
            continue
        for hook in nested_hooks:
            if (
                isinstance(hook, dict)
                and hook.get("type") == "command"
                and hook.get("command") == command
            ):
                return True
    return False


def merge_hooks(config: dict[str, Any], command: str) -> dict[str, Any]:
    """Return a copy of *config* with the probe command installed once per event."""
    if not isinstance(config, dict):
        raise ValueError("Hook configuration must be a JSON object")
    if not command:
        raise ValueError("Hook command must not be empty")

    result = copy.deepcopy(config)
    hooks = result.setdefault("hooks", {})
    if not isinstance(hooks, dict):
        raise ValueError("The 'hooks' value must be a JSON object")

    for event_name in EVENT_NAMES:
        entries = hooks.setdefault(event_name, [])
        if not isinstance(entries, list):
            raise ValueError(f"The hooks for {event_name} must be a JSON array")
        if _contains_command(entries, command):
            continue

        entry: dict[str, Any] = {
            "hooks": [{"type": "command", "command": command}]
        }
        if event_name in {"PreToolUse", "PostToolUse"}:
            entry["matcher"] = ""
        entries.append(entry)

    return result


def _write_atomically(path: Path, content: str) -> None:
    temporary_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            newline="\n",
            dir=path.parent,
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as temporary:
            temporary.write(content)
            temporary.flush()
            os.fsync(temporary.fileno())
            temporary_path = Path(temporary.name)
        os.replace(temporary_path, path)
    finally:
        if temporary_path is not None and temporary_path.exists():
            temporary_path.unlink()


def _create_backup(config_path: Path, original: bytes) -> Path:
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    backup = config_path.with_name(f"{config_path.name}.bak.{stamp}")
    with backup.open("xb") as backup_file:
        backup_file.write(original)
        backup_file.flush()
        os.fsync(backup_file.fileno())
    return backup


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Safely merge the Codex Halo probe into a hooks file."
    )
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--command", required=True)
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Back up and update the configuration (default: preview only).",
    )
    args = parser.parse_args()

    original = args.config.read_bytes()
    current = json.loads(original.decode("utf-8"))
    merged = merge_hooks(current, args.command)
    rendered = json.dumps(merged, ensure_ascii=False, indent=2) + "\n"

    if not args.apply:
        print(rendered, end="")
        return 0

    backup = _create_backup(args.config, original)
    _write_atomically(args.config, rendered)
    print(backup)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
