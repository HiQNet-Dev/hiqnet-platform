"""Casos positivos y negativos del gate de publicación."""

import unittest

from validate_release import validate


class ReleaseValidationTests(unittest.TestCase):
    def test_valid_tag(self):
        self.assertEqual(
            validate("push", "refs/tags/platform-v0.1.0-alpha.1", "", "0.1.0"),
            "platform-v0.1.0-alpha.1",
        )

    def test_dispatch_is_only_a_dry_run(self):
        self.assertEqual(
            validate("workflow_dispatch", "refs/heads/main", "0.1.0", "0.1.0"),
            "platform-v0.1.0",
        )

    def test_reject_bad_refs_and_versions(self):
        for event, ref, version in [
            ("push", "refs/heads/main", "0.1.0"),
            ("push", "refs/tags/platform-v01.0.0", ""),
            ("push", "refs/tags/platform-v0.1.0-rc.01", ""),
            ("push", "refs/tags/platform-v0.2.0", ""),
            ("push", "refs/tags/platform-v0.1.0+build.1", ""),
            ("workflow_dispatch", "refs/heads/main", "main"),
        ]:
            with self.subTest(ref=ref, version=version), self.assertRaises(ValueError):
                validate(event, ref, version, "0.1.0")


if __name__ == "__main__":
    unittest.main()
