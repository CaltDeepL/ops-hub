#!/usr/bin/env python3
"""scripts/github/main-ruleset.json の内容でGitHub live Rulesetを上書きする。

このリポジトリでは main-ruleset.json を正本(desired)として扱う。
verify_main_ruleset.py が差分の検出のみ(未認証GET)を行うのに対し、
このスクリプトは実際にPUTしてlive側をdesiredへ一致させる。

差分表示には verify_main_ruleset.subset_differences を再利用するため、
「何がどう違うか」の判定基準は検証側と常に同一になる。

事前準備(Administration: Read and write 権限のあるトークンが必要):
    export GITHUB_TOKEN=$(gh auth token)
    # もしくは fine-grained PAT を直接:
    # export GITHUB_TOKEN=github_pat_xxx

使い方:
    python3 scripts/github/sync_main_ruleset.py --dry-run  # 差分表示のみ。適用しない
    python3 scripts/github/sync_main_ruleset.py            # 差分表示後、y/N確認して適用
    python3 scripts/github/sync_main_ruleset.py --yes      # 確認なしで適用

適用後は python3 scripts/github/verify_main_ruleset.py で一致を再確認すること。

注意: bypass_actors は未認証GETでは取得できないため verify 側では検証されないが、
このスクリプトのPUTは main-ruleset.json の bypass_actors([] = バイパス無し)で
live側を上書きする。live側に設定済みのバイパスがある場合は消える。
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).parent))
from verify_main_ruleset import subset_differences  # noqa: E402

API_BASE = "https://api.github.com/repos/CaltDeepL/ops-hub"
RULESETS_URL = f"{API_BASE}/rulesets"
DESIRED_PATH = Path(__file__).with_name("main-ruleset.json")

# PUTで送るフィールド。target は作成後に変更できないため送らない。
BODY_FIELDS = ("name", "enforcement", "conditions", "bypass_actors", "rules")



def ruleset_differences(desired: Any, live: Any) -> list[str]:
    """desired/live の差分を返す。rules は type でペアリングして項目単位で比較する。

    subset_differences は rules を「順不同の配列」として扱うため、1項目でも
    違うと「ルール丸ごと一致なし」としか報告できない。Rulesetでは type が
    実質的な主キーなので、先に type で突き合わせてから各パラメータを比較する。
    """
    differences = subset_differences(
        {k: v for k, v in desired.items() if k != "rules"},
        live,
        "$.ruleset",
    )

    desired_rules = desired.get("rules", [])
    live_rules = live.get("rules", [])
    if not isinstance(live_rules, list):
        return differences + ["$.ruleset.rules: live側が配列ではありません"]

    live_by_type = {
        rule.get("type"): rule for rule in live_rules if isinstance(rule, dict)
    }

    for desired_rule in desired_rules:
        if not isinstance(desired_rule, dict):
            continue
        rule_type = desired_rule.get("type")
        live_rule = live_by_type.get(rule_type)
        if live_rule is None:
            differences.append(
                f"$.ruleset.rules[type={rule_type}]: live側に存在しません"
            )
            continue
        differences.extend(
            subset_differences(
                desired_rule, live_rule, f"$.ruleset.rules[type={rule_type}]"
            )
        )

    desired_types = {
        rule.get("type") for rule in desired_rules if isinstance(rule, dict)
    }
    for extra_type in sorted(set(live_by_type) - desired_types, key=str):
        differences.append(
            f"$.ruleset.rules[type={extra_type}]: desiredに無いルールがliveに存在します"
        )

    return differences


def build_headers(token: str) -> dict[str, str]:
    return {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "User-Agent": "ops-hub-main-ruleset-sync",
        "X-GitHub-Api-Version": "2022-11-28",
    }


def request_json(
    url: str, headers: dict[str, str], method: str = "GET", body: Any = None
) -> Any:
    data = json.dumps(body).encode("utf-8") if body is not None else None
    request = Request(url, headers=headers, method=method, data=data)
    if data is not None:
        request.add_header("Content-Type", "application/json")
    with urlopen(request, timeout=20) as response:
        return json.load(response)


def find_ruleset_id(headers: dict[str, str], name: str, target: str) -> int:
    rulesets = request_json(RULESETS_URL, headers)
    if not isinstance(rulesets, list):
        raise SystemExit("GET /rulesets did not return an array")
    matches = [
        r
        for r in rulesets
        if isinstance(r, dict) and r.get("name") == name and r.get("target") == target
    ]
    if len(matches) != 1:
        raise SystemExit(
            f"expected exactly one ruleset name={name!r} target={target!r}, "
            f"found {len(matches)}"
        )
    ruleset_id = matches[0].get("id")
    if not isinstance(ruleset_id, int):
        raise SystemExit("matching ruleset has no integer id")
    return ruleset_id


def api_error_message(error: HTTPError) -> str:
    detail = error.read().decode(errors="replace")
    message = f"HTTP {error.code} {error.reason}"
    if error.code == 403:
        message += (
            " (トークンの権限不足か、未認証時のrate limitです。"
            "Administration: Read and write のトークンを設定してください)"
        )
    elif error.code == 404:
        message += " (リポジトリ名、またはトークンのスコープを確認してください)"
    return f"{message}: {detail}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--yes", action="store_true", help="確認なしで適用する")
    parser.add_argument(
        "--dry-run", action="store_true", help="差分の表示のみ。適用しない"
    )
    args = parser.parse_args()

    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        print(
            "GITHUB_TOKEN が未設定です。書き込み権限(Administration: Read and write)"
            "のあるトークンを設定してください。例: export GITHUB_TOKEN=$(gh auth token)",
            file=sys.stderr,
        )
        return 1

    try:
        desired = json.loads(DESIRED_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"could not load desired Ruleset: {error}", file=sys.stderr)
        return 1

    headers = build_headers(token)

    try:
        ruleset_id = find_ruleset_id(headers, desired["name"], desired["target"])
        live = request_json(f"{RULESETS_URL}/{ruleset_id}", headers)
    except HTTPError as error:
        print(f"GitHub API error: {api_error_message(error)}", file=sys.stderr)
        return 1
    except URLError as error:
        print(f"GitHub API error: {error.reason}", file=sys.stderr)
        return 1

    print(f"live ruleset id = {ruleset_id}")

    differences = ruleset_differences(desired, live)
    if not differences:
        print("live Ruleset は既に desired と一致しています。適用の必要はありません。")
        return 0

    print(f"\n差分 {len(differences)}件 (desired = main-ruleset.json):")
    for difference in differences:
        print(f"- {difference}")

    if args.dry_run:
        print("\n--dry-run のため適用しません。")
        return 0

    if not args.yes:
        answer = input(
            "\nmain-ruleset.json の内容で live Ruleset を上書きしますか？ [y/N] "
        )
        if answer.strip().lower() != "y":
            print("中止しました。")
            return 1

    body = {field: desired[field] for field in BODY_FIELDS if field in desired}
    body.setdefault("bypass_actors", [])

    try:
        request_json(f"{RULESETS_URL}/{ruleset_id}", headers, method="PUT", body=body)
    except HTTPError as error:
        print(f"PUT failed: {api_error_message(error)}", file=sys.stderr)
        return 1
    except URLError as error:
        print(f"PUT failed: {error.reason}", file=sys.stderr)
        return 1

    print("\n適用しました。確認: python3 scripts/github/verify_main_ruleset.py")
    return 0


if __name__ == "__main__":
    sys.exit(main())