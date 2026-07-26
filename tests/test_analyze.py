import json
import tempfile
import unittest
from pathlib import Path

from tools.codex_probe.analyze import analyze_directory


class AnalyzeDirectoryTests(unittest.TestCase):
    def test_groups_identity_fingerprints_by_path(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            events = [
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": [
                        {"path": "$.thread_id", "fingerprint": "aaa", "length": 10}
                    ],
                },
                {
                    "hook_type": "PostToolUse",
                    "identity_candidates": [
                        {"path": "$.thread_id", "fingerprint": "aaa", "length": 10}
                    ],
                },
                {
                    "hook_type": "PreToolUse",
                    "identity_candidates": [
                        {"path": "$.thread_id", "fingerprint": "bbb", "length": 10}
                    ],
                },
            ]
            for index, event in enumerate(events):
                (root / f"{index}.json").write_text(
                    json.dumps(event),
                    encoding="utf-8",
                )

            report = analyze_directory(root)

            self.assertEqual(report["event_count"], 3)
            candidate = report["identity_paths"]["$.thread_id"]
            self.assertEqual(candidate["distinct_fingerprints"], 2)
            self.assertEqual(candidate["events"], 3)
            self.assertEqual(
                candidate["hook_types"],
                {"PostToolUse": 1, "PreToolUse": 2},
            )


if __name__ == "__main__":
    unittest.main()
