"""Comprueba que release usa las mismas bases aisladas de Rust que CI."""

import re
import unittest
from pathlib import Path


WORKFLOWS = Path(__file__).resolve().parents[1] / ".github" / "workflows"
URL_KEYS = (
    "TEST_DATABASE_URL",
    "TEST_UPGRADE_DATABASE_URL",
    "TEST_AUTH_UPGRADE_DATABASE_URL",
)


def rust_test_databases(workflow):
    content = (WORKFLOWS / workflow).read_text(encoding="utf-8")
    rust_job = content.split("  rust:\n", 1)[1].split(
        "\n  docker:" if workflow.startswith("release") else "\n  control-center:", 1
    )[0]
    after_creation = rust_job.split("      - name: Crear base efímera de pruebas\n", 1)[1]
    creation_step = after_creation.split("      - ", 1)[0]
    databases = re.findall(r"CREATE DATABASE (hiqnet_\w+)'", creation_step)
    test_step = rust_job.split("run: cargo test --workspace\n", 1)[1].split("\n      - ", 1)[0]
    urls = dict(
        re.findall(
            r"^\s+(TEST_(?:DATABASE|UPGRADE_DATABASE|AUTH_UPGRADE_DATABASE)_URL): (postgres://\S+)$",
            test_step,
            re.MULTILINE,
        )
    )
    return databases, urls


class ReleasePostgresTests(unittest.TestCase):
    def test_release_rust_matches_ci_isolated_databases(self):
        ci_databases, ci_urls = rust_test_databases("ci.yml")
        release_databases, release_urls = rust_test_databases("release-platform.yml")
        self.assertEqual(release_databases, ci_databases)
        self.assertEqual(release_urls, ci_urls)
        self.assertEqual(len(set(release_databases)), len(URL_KEYS))
        self.assertTrue(all("test" in name for name in release_databases))
        expected_databases = {
            "TEST_DATABASE_URL": "hiqnet_test",
            "TEST_UPGRADE_DATABASE_URL": "hiqnet_upgrade_test",
            "TEST_AUTH_UPGRADE_DATABASE_URL": "hiqnet_auth_upgrade_test",
        }
        self.assertEqual(
            {key: url.rsplit("/", 1)[1] for key, url in release_urls.items()},
            expected_databases,
        )


if __name__ == "__main__":
    unittest.main()
