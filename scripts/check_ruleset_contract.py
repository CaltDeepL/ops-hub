#!/usr/bin/env python3
"""Rulesetのrequired checkとCI workflowの静的契約を検証する。"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CI_PATH = REPOSITORY_ROOT / ".github/workflows/ci.yml"
DEFAULT_RULESET_PATH = REPOSITORY_ROOT / "scripts/github/main-ruleset.json"


def top_level_block(text: str, key: str) -> str | None:
    start_match = re.search(rf"(?m)^{re.escape(key)}:\s*(?:#.*)?$", text)
    if start_match is None:
        return None

    block_start = start_match.end()
    next_key = re.search(r"(?m)^[A-Za-z0-9_-]+:\s*", text[block_start:])
    block_end = (
        block_start + next_key.start()
        if next_key is not None
        else len(text)
    )
    return text[block_start:block_end]


def yaml_scalar(raw: str) -> str:
    value = re.split(r"\s+#", raw, maxsplit=1)[0].strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        return value[1:-1]
    return value


def required_contexts(ruleset: object) -> tuple[set[str], list[str]]:
    errors: list[str] = []
    if not isinstance(ruleset, dict):
        return set(), ["Ruleset JSON root must be an object"]

    rules = ruleset.get("rules")
    if not isinstance(rules, list):
        return set(), ["Ruleset JSON must contain a rules array"]

    status_rules = [
        rule
        for rule in rules
        if isinstance(rule, dict) and rule.get("type") == "required_status_checks"
    ]
    if len(status_rules) != 1:
        return set(), [
            "Ruleset JSON must contain exactly one required_status_checks rule"
        ]

    parameters = status_rules[0].get("parameters")
    checks = (
        parameters.get("required_status_checks")
        if isinstance(parameters, dict)
        else None
    )
    if not isinstance(checks, list) or not checks:
        return set(), ["required_status_checks must be a non-empty array"]

    contexts: set[str] = set()
    for index, check in enumerate(checks):
        context = check.get("context") if isinstance(check, dict) else None
        if not isinstance(context, str) or not context:
            errors.append(
                f"required_status_checks[{index}].context must be a non-empty string"
            )
            continue
        contexts.add(context)
    return contexts, errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ci", type=Path, default=DEFAULT_CI_PATH)
    parser.add_argument("--ruleset", type=Path, default=DEFAULT_RULESET_PATH)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    errors: list[str] = []

    try:
        ci_text = args.ci.read_text(encoding="utf-8")
    except OSError as error:
        print(f"ruleset contract check failed: cannot read CI workflow: {error}", file=sys.stderr)
        return 1

    try:
        ruleset = json.loads(args.ruleset.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"ruleset contract check failed: cannot load Ruleset JSON: {error}", file=sys.stderr)
        return 1

    contexts, context_errors = required_contexts(ruleset)
    errors.extend(context_errors)

    jobs_block = top_level_block(ci_text, "jobs")
    if jobs_block is None:
        errors.append("CI workflow has no top-level jobs block")
        job_names: set[str] = set()
    else:
        job_names = {
            yaml_scalar(match.group(1))
            for match in re.finditer(r"(?m)^    name:\s*(.+?)\s*$", jobs_block)
        }
        if not job_names:
            errors.append("CI workflow has no jobs.*.name values")

    missing_contexts = sorted(contexts - job_names)
    if missing_contexts:
        errors.append(
            "required status check contexts missing from jobs.*.name: "
            + ", ".join(missing_contexts)
        )

    on_block = top_level_block(ci_text, "on")
    if on_block is None:
        errors.append("CI workflow has no top-level on block")
    else:
        forbidden = sorted(
            set(re.findall(r"(?m)^\s+(paths(?:-ignore)?):\s*", on_block))
        )
        if forbidden:
            errors.append(
                "CI workflow on block contains forbidden path filters: "
                + ", ".join(forbidden)
            )

    if errors:
        print("ruleset contract check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "ruleset contract check passed: "
        f"contexts={sorted(contexts)}, no paths/paths-ignore triggers"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
