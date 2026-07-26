from __future__ import annotations

import argparse
import copy
import json
import os
import tempfile
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

from tools.codex_probe.events import HOOK_EVENT_NAMES

EVENT_NAMES = HOOK_EVENT_NAMES
RETIRED_EVENT_NAME = "Notification"


class TransactionRecoveryError(RuntimeError):
    """Raised when an interrupted hook transaction needs explicit recovery."""


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


def _remove_command_handlers(
    entries: list[Any],
    commands: set[str],
) -> tuple[list[Any], bool]:
    retained_entries: list[Any] = []
    removed_any = False
    for entry in entries:
        if not isinstance(entry, dict):
            retained_entries.append(entry)
            continue
        nested_hooks = entry.get("hooks")
        if not isinstance(nested_hooks, list):
            retained_entries.append(entry)
            continue
        removed_target = False
        retained_hooks: list[Any] = []
        for hook in nested_hooks:
            if (
                isinstance(hook, dict)
                and hook.get("type") == "command"
                and hook.get("command") in commands
            ):
                removed_target = True
                removed_any = True
            else:
                retained_hooks.append(hook)
        if removed_target and not retained_hooks:
            continue
        if removed_target:
            entry["hooks"] = retained_hooks
        retained_entries.append(entry)
    return retained_entries, removed_any


def _validate_excluded_events(exclude_events: tuple[str, ...]) -> set[str]:
    excluded = set(exclude_events)
    invalid = [event for event in excluded if event not in EVENT_NAMES]
    if invalid:
        rendered = ", ".join(repr(event) for event in sorted(invalid))
        raise ValueError(f"Unknown or empty lifecycle event: {rendered}")
    return excluded


def _parse_event_name(value: str) -> str:
    try:
        _validate_excluded_events((value,))
    except ValueError as error:
        raise argparse.ArgumentTypeError(str(error)) from error
    return value


def merge_hooks(
    config: dict[str, Any],
    command: str,
    *,
    remove_commands: tuple[str, ...] = (),
    exclude_events: tuple[str, ...] = (),
) -> dict[str, Any]:
    """Install the probe once per non-excluded event in a copy of *config*."""
    if not isinstance(config, dict):
        raise ValueError("Hook configuration must be a JSON object")
    if not command:
        raise ValueError("Hook command must not be empty")

    result = copy.deepcopy(config)
    hooks = result.setdefault("hooks", {})
    if not isinstance(hooks, dict):
        raise ValueError("The 'hooks' value must be a JSON object")

    explicit_removals = set(remove_commands)
    if any(not item for item in explicit_removals):
        raise ValueError("Commands to remove must not be empty")
    excluded_events = _validate_excluded_events(exclude_events)

    for event_name in EVENT_NAMES:
        if event_name in hooks and not isinstance(hooks[event_name], list):
            raise ValueError(f"The hooks for {event_name} must be a JSON array")

    for event_name in list(hooks):
        entries = hooks[event_name]
        if not isinstance(entries, list):
            continue
        removals = set(explicit_removals)
        if event_name == RETIRED_EVENT_NAME:
            removals.add(command)
        if event_name in excluded_events:
            removals.add(command)
        if not removals:
            continue
        retained_entries, removed_any = _remove_command_handlers(
            entries,
            removals,
        )
        if removed_any:
            if retained_entries:
                hooks[event_name] = retained_entries
            else:
                del hooks[event_name]

    for event_name in EVENT_NAMES:
        if event_name in excluded_events:
            continue
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


def _reject_symlink(path: Path) -> None:
    if path.is_symlink():
        raise ValueError(f"Refusing to use symbolic-link configuration: {path}")


def _read_config_bytes(path: Path) -> bytes:
    _reject_symlink(path)
    content = path.read_bytes()
    _reject_symlink(path)
    return content


def _assert_config_unchanged(path: Path, expected: bytes) -> None:
    _reject_symlink(path)
    current = path.read_bytes()
    _reject_symlink(path)
    if current != expected:
        raise RuntimeError(
            "Hook configuration changed while installing; refusing to overwrite it"
        )


def _restore_displaced_if_absent(displaced: Path, path: Path) -> bool:
    try:
        os.link(displaced, path, follow_symlinks=False)
    except FileExistsError:
        return False
    displaced.unlink()
    return True


def _verify_hardlink_capability(source: Path, path: Path) -> None:
    probe_path = path.with_name(
        f".{path.name}.hardlink-probe.{uuid.uuid4().hex}"
    )
    try:
        os.link(source, probe_path)
    finally:
        if probe_path.exists() or probe_path.is_symlink():
            probe_path.unlink()


def _pending_transactions(path: Path) -> list[Path]:
    return sorted(path.parent.glob(f".{path.name}.transaction.*"))


def recover_incomplete_transaction(path: Path, *, apply: bool) -> Path | None:
    """Recover one displaced config only when an explicit apply was requested."""
    _reject_symlink(path)
    pending = _pending_transactions(path)
    if not pending:
        return None

    rendered_paths = ", ".join(str(candidate) for candidate in pending)
    if path.exists():
        raise TransactionRecoveryError(
            "The hook configuration and unresolved transaction recovery "
            "files both exist; refusing to guess which version is correct. "
            f"Configuration: {path}. Recovery files: {rendered_paths}"
        )
    if not apply:
        raise TransactionRecoveryError(
            "An interrupted hook transaction needs recovery. "
            f"Run again with --apply to recover: {rendered_paths}"
        )
    if len(pending) != 1:
        raise TransactionRecoveryError(
            "Multiple hook transaction recovery files were found; "
            f"manual recovery is required: {rendered_paths}"
        )

    displaced = pending[0]
    _reject_symlink(displaced)
    try:
        os.link(displaced, path, follow_symlinks=False)
    except OSError as error:
        raise TransactionRecoveryError(
            "Could not recover the interrupted hook transaction. "
            f"The recoverable configuration remains at: {displaced}"
        ) from error

    try:
        displaced.unlink()
    except OSError as error:
        raise TransactionRecoveryError(
            "The hook configuration was restored, but its transaction "
            f"recovery file could not be removed: {displaced}"
        ) from error
    return displaced


def _write_atomically(path: Path, content: str, expected: bytes) -> None:
    """Publish content atomically without ever replacing a competing writer."""
    temporary_path: Path | None = None
    displaced_path: Path | None = None
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

        # Prove the current directory and filesystem permit the exact
        # create-if-absent primitive before moving the live configuration.
        _verify_hardlink_capability(temporary_path, path)

        # Move the checked directory entry aside as one atomic operation. Any
        # writer that wins before this move is detected by validating the moved
        # bytes; any writer that wins after it prevents the no-clobber publish.
        _assert_config_unchanged(path, expected)
        displaced_path = path.with_name(
            f".{path.name}.transaction.{uuid.uuid4().hex}"
        )
        os.replace(path, displaced_path)
        _reject_symlink(displaced_path)
        displaced_content = displaced_path.read_bytes()
        _reject_symlink(displaced_path)
        if displaced_content != expected:
            raise RuntimeError(
                "Hook configuration changed during the install transaction"
            )

        try:
            # Hard-link publication is an atomic create-if-absent operation.
            # Unlike os.replace(), it cannot overwrite a path recreated by a
            # concurrent writer in the final publication window.
            os.link(temporary_path, path)
        except FileExistsError as error:
            displaced_path.unlink()
            displaced_path = None
            raise RuntimeError(
                "A concurrent writer recreated the hook configuration; "
                "refusing to overwrite it"
            ) from error

        displaced_path.unlink()
        displaced_path = None
    finally:
        recovery_failure: OSError | None = None
        cleanup_failure: OSError | None = None
        if displaced_path is not None and (
            displaced_path.exists() or displaced_path.is_symlink()
        ):
            target_exists = path.exists() or path.is_symlink()
            if not target_exists:
                try:
                    restored = _restore_displaced_if_absent(
                        displaced_path,
                        path,
                    )
                except OSError as error:
                    recovery_failure = error
                    restored = False
                if restored:
                    displaced_path = None
        if temporary_path is not None and temporary_path.exists():
            try:
                temporary_path.unlink()
            except OSError as error:
                cleanup_failure = error
        if recovery_failure is not None:
            cleanup_detail = ""
            if cleanup_failure is not None:
                cleanup_detail = (
                    " Temporary-file cleanup also failed for: "
                    f"{temporary_path}."
                )
            raise TransactionRecoveryError(
                "The hook update failed and automatic recovery also failed. "
                "Recover the configuration manually from: "
                f"{displaced_path}.{cleanup_detail}"
            ) from recovery_failure
        if cleanup_failure is not None:
            raise RuntimeError(
                "The hook transaction could not clean up its temporary file: "
                f"{temporary_path}"
            ) from cleanup_failure


def _create_backup(config_path: Path, original: bytes) -> Path:
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    unique = uuid.uuid4().hex
    backup = config_path.with_name(
        f"{config_path.name}.bak.{stamp}-{unique}"
    )
    temporary_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb",
            dir=config_path.parent,
            prefix=f".{config_path.name}.backup.",
            suffix=".tmp",
            delete=False,
        ) as temporary:
            temporary.write(original)
            temporary.flush()
            os.fsync(temporary.fileno())
            temporary_path = Path(temporary.name)
        os.replace(temporary_path, backup)
        return backup
    finally:
        if temporary_path is not None and temporary_path.exists():
            temporary_path.unlink()


def apply_hooks(config_path: Path, original: bytes, rendered: str) -> Path:
    """Back up and replace an unchanged, regular hook configuration."""
    # Check once before creating the backup so a stale preview never initiates
    # an apply operation.
    _assert_config_unchanged(config_path, original)
    backup = _create_backup(config_path, original)
    _write_atomically(config_path, rendered, original)
    return backup


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Safely merge the Codex Halo probe into a hooks file."
    )
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--command", required=True)
    parser.add_argument(
        "--remove-command",
        action="append",
        default=[],
        help=(
            "Remove an exact command handler from every event; may be repeated."
        ),
    )
    parser.add_argument(
        "--exclude-event",
        action="append",
        default=[],
        type=_parse_event_name,
        help=(
            "Do not install the current command for this lifecycle event; "
            "may be repeated."
        ),
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Back up and update the configuration (default: preview only).",
    )
    args = parser.parse_args()

    recover_incomplete_transaction(args.config, apply=args.apply)
    original = _read_config_bytes(args.config)
    current = json.loads(original.decode("utf-8"))
    merged = merge_hooks(
        current,
        args.command,
        remove_commands=tuple(args.remove_command),
        exclude_events=tuple(args.exclude_event),
    )
    rendered = json.dumps(merged, ensure_ascii=False, indent=2) + "\n"

    if not args.apply:
        print(rendered, end="")
        return 0

    backup = apply_hooks(args.config, original, rendered)
    print(backup)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
