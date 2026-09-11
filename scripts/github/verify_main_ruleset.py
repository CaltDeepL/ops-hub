#!/usr/bin/env python3
"""公開GitHub REST APIでmain Rulesetの適用状態を検証する。"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


API_BASE = "https://api.github.com/repos/CaltDeepL/ops-hub"
RULESETS_URL = f"{API_BASE}/rulesets"
MAIN_RULES_URL = f"{API_BASE}/rules/branches/main"
DESIRED_PATH = Path(__file__).with_name("main-ruleset.json")
HEADERS = {
    "Accept": "application/vnd.github+json",
    "User-Agent": "ops-hub-main-ruleset-verifier",
    "X-GitHub-Api-Version": "2022-11-28",
}


def get_json(url: str) -> Any:
    """認証情報を付けず、GETだけでJSONを取得する。"""
    request = Request(url, headers=HEADERS, method="GET")
    with urlopen(request, timeout=20) as response:
        return json.load(response)


def subset_differences(desired: Any, live: Any, path: str = "$") -> list[str]:
    """desiredがliveのsubsetでない箇所を返す。listの順序は問わない。"""
    if isinstance(desired, dict):
        if not isinstance(live, dict):
            return [f"{path}: expected object, got {type(live).__name__}"]

        differences: list[str] = []
        for key, desired_value in desired.items():
            key_path = f"{path}.{key}"
            if key not in live:
                differences.append(f"{key_path}: missing from live settings")
                continue
            differences.extend(
                subset_differences(desired_value, live[key], key_path)
            )
        return differences

    if isinstance(desired, list):
        if not isinstance(live, list):
            return [f"{path}: expected array, got {type(live).__name__}"]

        differences = []
        for index, desired_item in enumerate(desired):
            if not any(
                not subset_differences(desired_item, live_item, path)
                for live_item in live
            ):
                encoded = json.dumps(desired_item, ensure_ascii=False, sort_keys=True)
                differences.append(
                    f"{path}[{index}]: no matching live item for {encoded}"
                )
        return differences

    if desired != live:
        return [f"{path}: desired {desired!r}, live {live!r}"]
    return []


def exact_array_differences(desired: Any, live: Any, path: str) -> list[str]:
    if not isinstance(desired, list) or not isinstance(live, list):
        return [
            f"{path}: exact array comparison requires arrays; "
            f"desired {type(desired).__name__}, live {type(live).__name__}"
        ]
    if desired == live:
        return []
    return [
        f"{path}: exact mismatch; "
        f"desired {json.dumps(desired, ensure_ascii=False, sort_keys=True)}, "
        f"live {json.dumps(live, ensure_ascii=False, sort_keys=True)}"
    ]


def exact_set_differences(
    desired: set[str], live: set[str], path: str
) -> list[str]:
    differences: list[str] = []
    missing = sorted(desired - live)
    unexpected = sorted(live - desired)
    if missing:
        differences.append(f"{path}: missing values {missing!r}")
    if unexpected:
        differences.append(f"{path}: unexpected live values {unexpected!r}")
    return differences


def rule_types(rules: Any) -> set[str]:
    if not isinstance(rules, list):
        return set()
    return {
        rule_type
        for rule in rules
        if isinstance(rule, dict)
        and isinstance((rule_type := rule.get("type")), str)
    }


def required_status_contexts(rules: Any) -> set[str]:
    if not isinstance(rules, list):
        return set()

    contexts: set[str] = set()
    for rule in rules:
        if not isinstance(rule, dict) or rule.get("type") != "required_status_checks":
            continue
        parameters = rule.get("parameters")
        checks = (
            parameters.get("required_status_checks")
            if isinstance(parameters, dict)
            else None
        )
        if not isinstance(checks, list):
            continue
        contexts.update(
            context
            for check in checks
            if isinstance(check, dict)
            and isinstance((context := check.get("context")), str)
        )
    return contexts


def fail(messages: list[str]) -> int:
    print("main Ruleset verification failed:", file=sys.stderr)
    for message in messages:
        print(f"- {message}", file=sys.stderr)
    return 1


def main() -> int:
    try:
        desired = json.loads(DESIRED_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return fail([f"could not load desired Ruleset: {error}"])

    try:
        rulesets = get_json(RULESETS_URL)
        main_rules = get_json(MAIN_RULES_URL)
    except HTTPError as error:
        return fail([f"GitHub API GET failed: HTTP {error.code} {error.reason}"])
    except URLError as error:
        return fail([f"GitHub API GET failed: {error.reason}"])
    except (OSError, json.JSONDecodeError) as error:
        return fail([f"GitHub API response could not be read: {error}"])

    if not isinstance(rulesets, list):
        return fail(["GET /rulesets did not return an array"])
    if not isinstance(main_rules, list):
        return fail(["GET /rules/branches/main did not return an array"])

    matching_rulesets = [
        ruleset
        for ruleset in rulesets
        if isinstance(ruleset, dict)
        and ruleset.get("name") == desired.get("name")
        and ruleset.get("target") == desired.get("target")
    ]
    if not matching_rulesets:
        return fail(
            [
                "Ruleset未作成: name='main-protection', target='branch' "
                "に一致するRulesetがありません"
            ]
        )
    if len(matching_rulesets) != 1:
        return fail(
            [
                "expected exactly one matching Ruleset, "
                f"found {len(matching_rulesets)}"
            ]
        )

    ruleset_summary = matching_rulesets[0]
    ruleset_id = ruleset_summary.get("id")
    if not isinstance(ruleset_id, int):
        return fail(["matching Ruleset has no integer id"])

    summary_desired = {
        key: desired[key]
        for key in ("name", "target", "enforcement")
    }
    differences = subset_differences(summary_desired, ruleset_summary, "$.ruleset")

    try:
        ruleset_detail = get_json(f"{RULESETS_URL}/{ruleset_id}")
    except HTTPError as error:
        return fail(
            differences
            + [f"GitHub Ruleset detail GET failed: HTTP {error.code} {error.reason}"]
        )
    except URLError as error:
        return fail(
            differences + [f"GitHub Ruleset detail GET failed: {error.reason}"]
        )
    except (OSError, json.JSONDecodeError) as error:
        return fail(
            differences + [f"GitHub Ruleset detail response could not be read: {error}"]
        )

    if not isinstance(ruleset_detail, dict):
        return fail(differences + ["GET /rulesets/{id} did not return an object"])

    differences.extend(
        subset_differences(desired, ruleset_detail, "$.ruleset")
    )

    rules_from_matching_ruleset = [
        rule
        for rule in main_rules
        if isinstance(rule, dict) and rule.get("ruleset_id") == ruleset_id
    ]
    differences.extend(
        subset_differences(
            desired.get("rules", []),
            rules_from_matching_ruleset,
            "$.main.rules",
        )
    )
    desired_rules = desired.get("rules")
    differences.extend(
        exact_set_differences(
            rule_types(desired_rules),
            rule_types(rules_from_matching_ruleset),
            "$.main.rules[].type",
        )
    )
    differences.extend(
        exact_set_differences(
            required_status_contexts(desired_rules),
            required_status_contexts(rules_from_matching_ruleset),
            "$.main.required_status_checks[].context",
        )
    )

    if differences:
        return fail(differences)

    print(f"main Ruleset matches desired contract (id={ruleset_id})")
    return 0


if __name__ == "__main__":
    sys.exit(main())