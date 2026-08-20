from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUST_SOURCE_COMMIT = "dc24aec6667ed23886cae8bd62fda5221a7e3747"
RUST_VERSION = "1.0.0-preview.4.23"


class WorkspacePolicyTests(unittest.TestCase):
    def test_all_rust_sdk_dependencies_use_the_coordinated_release_source(self) -> None:
        manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        lockfile = (ROOT / "Cargo.lock").read_text(encoding="utf-8")

        self.assertEqual(8, manifest.count(f'rev = "{RUST_SOURCE_COMMIT}"'))
        self.assertEqual(8, manifest.count('git = "https://github.com/NagareWorks/nnrp-rs.git"'))
        self.assertEqual(
            {RUST_SOURCE_COMMIT},
            set(re.findall(r"nnrp-rs\.git\?rev=([0-9a-f]{40})#", lockfile)),
        )
        self.assertEqual(
            {RUST_VERSION},
            set(re.findall(r'version = "(1\.0\.0-preview\.4\.[0-9]+)"', lockfile)),
        )

    def test_develop_and_main_pushes_run_ci(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")

        push_block = workflow.split("  pull_request:", maxsplit=1)[0]
        self.assertIn("      - develop", push_block)
        self.assertIn("      - main", push_block)


if __name__ == "__main__":
    unittest.main()
