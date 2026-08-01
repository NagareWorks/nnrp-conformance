from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from validate_public_json import validate_protocol_composition


class ProtocolCompositionTests(unittest.TestCase):
    def test_accepts_unique_cases_and_vectors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_case_manifest(root / "cases.json", "case.one")
            self._write_vector_manifest(root / "vectors.json", "vector.one")

            validate_protocol_composition(root, self._protocol_manifest())

    def test_rejects_duplicate_case_ids(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_case_manifest(root / "cases.json", "case.one")
            self._write_case_manifest(root / "cases-duplicate.json", "case.one")
            self._write_vector_manifest(root / "vectors.json", "vector.one")
            manifest = self._protocol_manifest()
            manifest["case_manifests"].append("cases-duplicate.json")

            with self.assertRaisesRegex(SystemExit, "duplicate case id"):
                validate_protocol_composition(root, manifest)

    def test_rejects_duplicate_vector_names(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_case_manifest(root / "cases.json", "case.one")
            self._write_vector_manifest(root / "vectors.json", "vector.one")
            self._write_vector_manifest(root / "vectors-duplicate.json", "vector.one")
            manifest = self._protocol_manifest()
            manifest["vector_recipe_manifests"].append("vectors-duplicate.json")

            with self.assertRaisesRegex(SystemExit, "duplicate semantic vector name"):
                validate_protocol_composition(root, manifest)

    def test_rejects_non_array_cases(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_manifest(
                root / "cases.json",
                {"protocol_version": "nnrp-1-preview4", "cases": {}},
            )
            self._write_vector_manifest(root / "vectors.json", "vector.one")

            with self.assertRaisesRegex(SystemExit, "cases must be an array"):
                validate_protocol_composition(root, self._protocol_manifest())

    def test_rejects_invalid_case_id(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_manifest(
                root / "cases.json",
                {"protocol_version": "nnrp-1-preview4", "cases": [{"id": ""}]},
            )
            self._write_vector_manifest(root / "vectors.json", "vector.one")

            with self.assertRaisesRegex(SystemExit, "invalid case id"):
                validate_protocol_composition(root, self._protocol_manifest())

    def test_rejects_non_array_vectors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_case_manifest(root / "cases.json", "case.one")
            self._write_manifest(
                root / "vectors.json",
                {"protocol_version": "nnrp-1-preview4", "vectors": {}},
            )

            with self.assertRaisesRegex(SystemExit, "vectors must be an array"):
                validate_protocol_composition(root, self._protocol_manifest())

    def test_rejects_invalid_vector_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self._write_case_manifest(root / "cases.json", "case.one")
            self._write_manifest(
                root / "vectors.json",
                {"protocol_version": "nnrp-1-preview4", "vectors": [{"name": ""}]},
            )

            with self.assertRaisesRegex(SystemExit, "invalid vector name"):
                validate_protocol_composition(root, self._protocol_manifest())

    @staticmethod
    def _protocol_manifest() -> dict[str, object]:
        return {
            "protocol_version": "nnrp-1-preview4",
            "case_manifests": ["cases.json"],
            "vector_recipe_manifests": ["vectors.json"],
        }

    @staticmethod
    def _write_case_manifest(path: Path, case_id: str) -> None:
        ProtocolCompositionTests._write_manifest(
            path,
            {
                "protocol_version": "nnrp-1-preview4",
                "cases": [{"id": case_id}],
            },
        )

    @staticmethod
    def _write_vector_manifest(path: Path, name: str) -> None:
        ProtocolCompositionTests._write_manifest(
            path,
            {
                "protocol_version": "nnrp-1-preview4",
                "vectors": [{"name": name}],
            },
        )

    @staticmethod
    def _write_manifest(path: Path, manifest: dict[str, object]) -> None:
        path.write_text(
            json.dumps(manifest),
            encoding="utf-8",
        )


if __name__ == "__main__":
    unittest.main()
