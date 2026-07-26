import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.codex_probe.analyze import (
    MAX_CANDIDATES_PER_EVENT,
    MAX_DISTINCT_FINGERPRINTS_PER_PATH,
    MAX_FILES,
    MAX_FILE_BYTES,
    MAX_IDENTITY_PATHS,
    analyze_directory,
    write_report_atomic,
)


ANALYZE_SCRIPT = (
    Path(__file__).resolve().parents[1] / "tools" / "codex_probe" / "analyze.py"
)


def write_event(root: Path, filename: str, event: object) -> None:
    (root / filename).write_text(json.dumps(event), encoding="utf-8")


class AnalyzeDirectoryTests(unittest.TestCase):
    def test_accepts_current_lifecycle_events_and_rejects_notification(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            current_events = (
                "SessionStart",
                "UserPromptSubmit",
                "PreToolUse",
                "PermissionRequest",
                "PostToolUse",
                "PreCompact",
                "PostCompact",
                "SubagentStart",
                "SubagentStop",
                "Stop",
                "SessionEnd",
            )
            for index, hook_type in enumerate(current_events):
                write_event(
                    root,
                    f"{index}.json",
                    {"hook_type": hook_type, "identity_candidates": []},
                )
            write_event(
                root,
                "retired.json",
                {"hook_type": "Notification", "identity_candidates": []},
            )

            report = analyze_directory(root)

            self.assertEqual(
                report["hook_types"],
                {
                    **{hook_type: 1 for hook_type in current_events},
                    "unknown": 1,
                },
            )
            self.assertEqual(report["invalid_records"]["hook_types"], 1)

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
                write_event(root, f"{index}.json", event)

            report = analyze_directory(root)

            self.assertEqual(report["event_count"], 3)
            candidate = report["identity_paths"]["$.thread_id"]
            self.assertEqual(candidate["distinct_fingerprints"], 2)
            self.assertEqual(candidate["events"], 3)
            self.assertEqual(
                candidate["hook_types"],
                {"PostToolUse": 1, "PreToolUse": 2},
            )

    def test_reports_anonymous_groups_in_first_seen_order(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            first_fingerprint = "bbbbbbbbbbbbbbbb"
            second_fingerprint = "aaaaaaaaaaaaaaaa"
            write_event(root, "00-array.json", [])
            (root / "00-malformed.json").write_text("{", encoding="utf-8")
            events = [
                ("01.json", "PreToolUse", first_fingerprint),
                ("02.json", "PreToolUse", second_fingerprint),
                ("03.json", "PostToolUse", first_fingerprint),
                ("04.json", "PostToolUse", second_fingerprint),
                ("05.json", "Stop", first_fingerprint),
                ("06.json", "Stop", second_fingerprint),
            ]
            for filename, hook_type, fingerprint in events:
                write_event(
                    root,
                    filename,
                    {
                        "hook_type": hook_type,
                        "identity_candidates": [
                            {
                                "path": "$.session_id",
                                "fingerprint": fingerprint,
                            }
                        ],
                    },
                )

            report = analyze_directory(root)

            groups = report["identity_paths"]["$.session_id"]["groups"]
            self.assertTrue(report["anonymous_group_labels_comparable"])
            self.assertEqual(
                groups,
                [
                    {
                        "label": "group_1",
                        "events": 3,
                        "hook_types": {
                            "PostToolUse": 1,
                            "PreToolUse": 1,
                            "Stop": 1,
                        },
                        "first_event_index": 1,
                        "last_event_index": 5,
                    },
                    {
                        "label": "group_2",
                        "events": 3,
                        "hook_types": {
                            "PostToolUse": 1,
                            "PreToolUse": 1,
                            "Stop": 1,
                        },
                        "first_event_index": 2,
                        "last_event_index": 6,
                    },
                ],
            )
            serialized = json.dumps(report)
            self.assertNotIn(first_fingerprint, serialized)
            self.assertNotIn(second_fingerprint, serialized)

    def test_counts_duplicate_identity_candidate_once_per_event_group(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            candidate = {
                "path": "$.session_id",
                "fingerprint": "aaaaaaaaaaaaaaaa",
            }
            write_event(
                root,
                "event.json",
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": [candidate, candidate.copy()],
                },
            )

            report = analyze_directory(root)

            identity = report["identity_paths"]["$.session_id"]
            self.assertEqual(identity["events"], 1)
            self.assertEqual(identity["hook_types"], {"PreToolUse": 1})
            self.assertEqual(
                identity["groups"],
                [
                    {
                        "label": "group_1",
                        "events": 1,
                        "hook_types": {"PreToolUse": 1},
                        "first_event_index": 1,
                        "last_event_index": 1,
                    }
                ],
            )

    def test_rejects_unsanitized_values_without_leaking_secrets(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            write_event(
                root,
                "unsafe.json",
                {
                    "hook_type": {"secret": "HOOK_SECRET"},
                    "identity_candidates": [
                        {
                            "path": {"secret": "PATH_SECRET"},
                            "fingerprint": "aaaaaaaaaaaaaaaa",
                        },
                        {
                            "path": "$.thread_id",
                            "fingerprint": {"secret": "FINGERPRINT_SECRET"},
                        },
                        {
                            "path": "$.unsafe_label",
                            "fingerprint": "bbbbbbbbbbbbbbbb",
                        },
                        {
                            "path": "$.thread_id",
                            "fingerprint": "CCCCCCCCCCCCCCCC",
                        },
                    ],
                },
            )
            write_event(
                root,
                "alternate.json",
                {
                    "hook_type": "x" * 65 + "HOOK_STRING_SECRET",
                    "identity_candidates": {
                        "secret": "CANDIDATE_CONTAINER_SECRET"
                    },
                },
            )

            report = analyze_directory(root)
            serialized = json.dumps(report)

            self.assertEqual(report["hook_types"], {"unknown": 2})
            self.assertEqual(report["identity_paths"], {})
            for secret in (
                "HOOK_SECRET",
                "PATH_SECRET",
                "FINGERPRINT_SECRET",
                "HOOK_STRING_SECRET",
                "CANDIDATE_CONTAINER_SECRET",
            ):
                self.assertNotIn(secret, serialized)

    def test_rejects_unrecognized_hook_type_without_leaking_it(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            write_event(
                root,
                "secret-hook.json",
                {
                    "hook_type": "OPENAI_API_KEY_SECRET123",
                    "identity_candidates": [],
                },
            )

            report = analyze_directory(root)
            serialized = json.dumps(report)

            self.assertEqual(report["hook_types"], {"unknown": 1})
            self.assertNotIn("OPENAI_API_KEY_SECRET123", serialized)

    def test_ignores_malformed_candidate_structures(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for index, candidates in enumerate((None, 7, True)):
                write_event(
                    root,
                    f"{index}.json",
                    {
                        "hook_type": "PreToolUse",
                        "identity_candidates": candidates,
                    },
                )
            write_event(
                root,
                "non-dicts.json",
                {
                    "hook_type": "PostToolUse",
                    "identity_candidates": [None, 1, False, "candidate", []],
                },
            )

            report = analyze_directory(root)

            self.assertEqual(report["event_count"], 4)
            self.assertEqual(report["identity_paths"], {})

    def test_skips_oversized_and_malformed_json_files(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "oversized.json").write_bytes(b" " * (MAX_FILE_BYTES + 1))
            (root / "malformed.json").write_text("{", encoding="utf-8")
            write_event(root, "valid.json", {"hook_type": "Stop"})
            write_event(root, "ignored.txt", {"hook_type": "PreToolUse"})

            report = analyze_directory(root)

            self.assertEqual(report["event_count"], 1)
            self.assertEqual(report["hook_types"], {"Stop": 1})
            self.assertEqual(report["skipped_files"]["oversized"], 1)
            self.assertEqual(report["skipped_files"]["malformed"], 1)

    def test_bounds_files_candidates_paths_and_fingerprints(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            fingerprint_candidates = [
                {
                    "path": "$.thread_id",
                    "fingerprint": f"{index:016x}",
                }
                for index in range(MAX_DISTINCT_FINGERPRINTS_PER_PATH + 1)
            ]
            path_candidates = [
                {
                    "path": f"$.key_{index:016x}.thread_id",
                    "fingerprint": "aaaaaaaaaaaaaaaa",
                }
                for index in range(MAX_IDENTITY_PATHS + 1)
            ]
            too_many_candidates = [
                {
                    "path": "$.thread_id",
                    "fingerprint": "aaaaaaaaaaaaaaaa",
                }
                for _ in range(MAX_CANDIDATES_PER_EVENT + 1)
            ]
            write_event(
                root,
                "fingerprints.json",
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": fingerprint_candidates,
                },
            )
            write_event(
                root,
                "paths.json",
                {
                    "hook_type": "PostToolUse",
                    "identity_candidates": path_candidates,
                },
            )
            write_event(
                root,
                "candidates.json",
                {
                    "hook_type": "Stop",
                    "identity_candidates": too_many_candidates,
                },
            )

            report = analyze_directory(root)

            self.assertLessEqual(len(report["identity_paths"]), MAX_IDENTITY_PATHS)
            self.assertLessEqual(
                report["identity_paths"]["$.thread_id"][
                    "distinct_fingerprints"
                ],
                MAX_DISTINCT_FINGERPRINTS_PER_PATH,
            )
            self.assertGreater(report["truncated"]["candidates"], 0)
            self.assertGreater(report["truncated"]["identity_paths"], 0)
            self.assertGreater(report["truncated"]["fingerprints"], 0)

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for index in range(MAX_FILES + 1):
                write_event(root, f"{index:04}.json", {"hook_type": "Stop"})

            report = analyze_directory(root)

            self.assertEqual(report["event_count"], MAX_FILES)
            self.assertTrue(report["file_limit_reached"])

    def test_file_limit_stops_after_bounded_sample_and_marks_groups_unstable(
        self,
    ):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            paths = []
            for index in range(MAX_FILES + 1):
                hook_type = "Stop"
                if index == 0:
                    hook_type = "PreToolUse"
                elif index == MAX_FILES:
                    hook_type = "SessionStart"
                path = root / f"{index:04}.json"
                write_event(root, path.name, {"hook_type": hook_type})
                paths.append(path)

            def bounded_glob():
                yield from reversed(paths)
                raise AssertionError("glob consumed beyond MAX_FILES + 1")

            with mock.patch(
                "tools.codex_probe.analyze.Path.glob",
                return_value=bounded_glob(),
            ):
                report = analyze_directory(root)

            self.assertTrue(report["file_limit_reached"])
            self.assertFalse(report["anonymous_group_labels_comparable"])
            self.assertEqual(report["event_count"], MAX_FILES)
            self.assertEqual(
                report["hook_types"],
                {"PreToolUse": 1, "Stop": MAX_FILES - 1},
            )


class AtomicReportTests(unittest.TestCase):
    def test_cleans_exact_temporary_file_when_replace_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "nested" / "report.json"
            with mock.patch(
                "tools.codex_probe.analyze.os.replace",
                side_effect=OSError("publish failed"),
            ) as replace:
                with self.assertRaises(OSError):
                    write_report_atomic(output, {"schema_version": 1})

            temporary_path, final_path = replace.call_args.args
            self.assertEqual(Path(temporary_path).parent, output.parent)
            self.assertEqual(Path(final_path), output)
            self.assertFalse(Path(temporary_path).exists())
            self.assertEqual(list(output.parent.iterdir()), [])


class AnalyzeCliTests(unittest.TestCase):
    def test_generates_valid_report_with_dir_and_output(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            input_dir = root / "events"
            input_dir.mkdir()
            output = root / "reports" / "report.json"
            write_event(
                input_dir,
                "event.json",
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": [
                        {
                            "path": "$.thread_id",
                            "fingerprint": "aaaaaaaaaaaaaaaa",
                        }
                    ],
                },
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(ANALYZE_SCRIPT),
                    "--dir",
                    str(input_dir),
                    "--output",
                    str(output),
                ],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(report["event_count"], 1)
            self.assertEqual(
                report["identity_paths"]["$.thread_id"][
                    "distinct_fingerprints"
                ],
                1,
            )


if __name__ == "__main__":
    unittest.main()
