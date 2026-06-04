#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    from jsonschema import Draft202012Validator
except ImportError as exc:  # pragma: no cover - exercised in CI/runtime, not unit tests
    raise SystemExit(
        "missing Python dependency 'jsonschema'; install it with 'python -m pip install jsonschema'"
    ) from exc


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise SystemExit(f"failed to read JSON file {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"failed to parse JSON file {path}: {exc}") from exc


def validate_json(schema_path: Path, instance_path: Path) -> None:
    schema = load_json(schema_path)
    instance = load_json(instance_path)
    validator = Draft202012Validator(schema)
    errors = sorted(validator.iter_errors(instance), key=lambda error: list(error.absolute_path))
    if not errors:
        return

    details = []
    for error in errors:
        json_path = "/".join(str(part) for part in error.absolute_path) or "<root>"
        details.append(f"- {json_path}: {error.message}")

    raise SystemExit(
        f"schema validation failed for {instance_path} against {schema_path}:\n"
        + "\n".join(details)
    )


def validate_capability_tokens(
    protocol_root: Path,
    protocol_manifest: dict[str, object],
    capability_path: Path,
) -> None:
    capability_manifest = load_json(capability_path)
    if not isinstance(capability_manifest, dict):
        raise SystemExit(f"capability manifest must be a JSON object: {capability_path}")

    allowed_tokens: set[str] = set()
    for relative_path in protocol_manifest.get("case_manifests", []):
        case_manifest = load_json(protocol_root / str(relative_path))
        if not isinstance(case_manifest, dict):
            raise SystemExit(f"case manifest must be a JSON object: {protocol_root / str(relative_path)}")
        cases = case_manifest.get("cases", [])
        if not isinstance(cases, list):
            raise SystemExit(f"case manifest cases must be an array: {protocol_root / str(relative_path)}")
        for case in cases:
            if not isinstance(case, dict):
                raise SystemExit(f"case manifest contains a non-object case: {protocol_root / str(relative_path)}")
            required = case.get("required_capabilities", [])
            if not isinstance(required, list):
                raise SystemExit(
                    f"case manifest required_capabilities must be an array: {protocol_root / str(relative_path)}"
                )
            allowed_tokens.update(str(token) for token in required)

    supports = capability_manifest.get("supports", [])
    if not isinstance(supports, list):
        raise SystemExit(f"capability manifest supports must be an array: {capability_path}")

    unknown_tokens = sorted(str(token) for token in supports if str(token) not in allowed_tokens)
    if unknown_tokens:
        raise SystemExit(
            f"capability manifest {capability_path} declares unknown capability token(s): "
            + ", ".join(unknown_tokens)
        )


def find_repo_root(start: Path) -> Path:
    for candidate in (start, *start.parents):
        if (candidate / "schemas" / "protocol-manifest.schema.json").exists():
            return candidate
    raise SystemExit(f"failed to locate repository root from {start}")


def validate_protocol_baseline(protocol_manifest_path: Path) -> None:
    protocol_manifest_path = protocol_manifest_path.resolve()
    protocol_root = protocol_manifest_path.parent
    repo_root = find_repo_root(protocol_root)
    schema_root = repo_root / "schemas"

    validate_json(schema_root / "protocol-manifest.schema.json", protocol_manifest_path)
    protocol_manifest = load_json(protocol_manifest_path)
    if not isinstance(protocol_manifest, dict):
        raise SystemExit(f"protocol manifest must be a JSON object: {protocol_manifest_path}")

    for relative_path in protocol_manifest.get("case_manifests", []):
        validate_json(schema_root / "case-manifest.schema.json", protocol_root / relative_path)

    example_capabilities = protocol_root / "example-capabilities.json"
    if example_capabilities.exists():
        validate_json(schema_root / "capability-manifest.schema.json", example_capabilities)
        validate_capability_tokens(protocol_root, protocol_manifest, example_capabilities)

    for relative_path in protocol_manifest.get("vector_recipe_manifests", []):
        if relative_path:
            validate_json(
                schema_root / "semantic-vector-recipes.schema.json",
                protocol_root / relative_path,
            )

    for relative_path in protocol_manifest.get("vector_manifests", []):
        if relative_path:
            validate_json(schema_root / "vector-manifest.schema.json", protocol_root / relative_path)

    docs_examples = repo_root / "docs" / "examples"
    adapter_plan_example = docs_examples / "adapter-execution-plan.sample.json"
    if adapter_plan_example.exists():
        validate_json(schema_root / "adapter-execution-plan.schema.json", adapter_plan_example)

    adapter_results_example = docs_examples / "adapter-case-results.sample.json"
    if adapter_results_example.exists():
        validate_json(schema_root / "adapter-case-results.schema.json", adapter_results_example)

    benchmark_plan_example = docs_examples / "benchmark-execution-plan.sample.json"
    if benchmark_plan_example.exists():
        validate_json(schema_root / "benchmark-execution-plan.schema.json", benchmark_plan_example)

    benchmark_results_example = docs_examples / "benchmark-results.sample.json"
    if benchmark_results_example.exists():
        validate_json(schema_root / "benchmark-results.schema.json", benchmark_results_example)

    api_profile_capabilities_example = docs_examples / "api-profile-capabilities.sample.json"
    if api_profile_capabilities_example.exists():
        validate_json(
            schema_root / "api-profile-capabilities.schema.json",
            api_profile_capabilities_example,
        )

    api_profile_recipe_example = docs_examples / "api-profile-recipe.sample.json"
    if api_profile_recipe_example.exists():
        validate_json(schema_root / "api-profile-recipe.schema.json", api_profile_recipe_example)

    api_profile_plan_example = docs_examples / "api-profile-execution-plan.sample.json"
    if api_profile_plan_example.exists():
        validate_json(
            schema_root / "api-profile-execution-plan.schema.json",
            api_profile_plan_example,
        )

    api_profile_results_example = docs_examples / "api-profile-case-results.sample.json"
    if api_profile_results_example.exists():
        validate_json(
            schema_root / "api-profile-case-results.schema.json",
            api_profile_results_example,
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate public conformance JSON artifacts against suite-owned schemas."
    )
    parser.add_argument(
        "--protocol",
        required=True,
        type=Path,
        help="Path to protocol/<version>/manifest.json",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    validate_protocol_baseline(args.protocol)
    print(f"validated public JSON for {args.protocol}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
