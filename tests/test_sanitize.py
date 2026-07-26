import json
import unittest
from datetime import datetime, timezone

from tools.codex_probe.sanitize import summarize_event


class SummarizeEventTests(unittest.TestCase):
    def test_redacts_values_and_fingerprints_candidate_ids(self):
        payload = {
            "hook_type": "PreToolUse",
            "tool_name": "exec_command",
            "thread_id": "thread-secret-123",
            "cwd": r"D:\Private\Client",
            "arguments": {
                "cmd": "print the production token",
                "content": "SECRET-CODE-CONTENT",
            },
            "env": {"SECRET_KEY_NAME": "SECRET_VALUE"},
            "message": "private prompt",
        }

        summary = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 0, tzinfo=timezone.utc),
        )
        serialized = json.dumps(summary, ensure_ascii=False)

        self.assertEqual(summary["hook_type"], "PreToolUse")
        self.assertEqual(summary["tool_name"], "exec_command")
        self.assertEqual(summary["top_level_keys"], sorted(payload))
        self.assertEqual(summary["received_at"], "2026-07-26T08:00:00+00:00")
        self.assertNotIn("thread-secret-123", serialized)
        self.assertNotIn(r"D:\Private\Client", serialized)
        self.assertNotIn("SECRET-CODE-CONTENT", serialized)
        self.assertNotIn("private prompt", serialized)
        self.assertNotIn("production token", serialized)
        self.assertNotIn("SECRET_KEY_NAME", serialized)
        self.assertNotIn("SECRET_VALUE", serialized)

        identities = {item["path"]: item for item in summary["identity_candidates"]}
        self.assertIn("$.thread_id", identities)
        self.assertIn("$.cwd", identities)
        self.assertRegex(identities["$.thread_id"]["fingerprint"], r"^[0-9a-f]{16}$")
        self.assertEqual(identities["$.thread_id"]["length"], 17)

    def test_records_shape_without_scalar_values(self):
        summary = summarize_event(
            {"hook_type": "Stop", "nested": {"items": [1, "secret"]}},
            received_at=datetime(2026, 7, 26, 8, 1, tzinfo=timezone.utc),
        )

        paths = {item["path"]: item["kind"] for item in summary["shape"]}
        self.assertEqual(paths["$"], "object")
        self.assertEqual(paths["$.nested"], "object")
        self.assertEqual(paths["$.nested.items"], "array")
        self.assertEqual(paths["$.nested.items[0]"], "number")
        self.assertEqual(paths["$.nested.items[1]"], "string")


if __name__ == "__main__":
    unittest.main()
