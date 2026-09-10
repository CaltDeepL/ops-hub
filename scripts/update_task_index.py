#!/usr/bin/env python3
"""既存16タスク + Q系列品質タスクから docs/task-INDEX.md を生成する。

Task classification:
- 01〜06: legacy
- Q1, Q2, ...: managed
- 07以降: managed

ROADMAP の並びをタスク実行順の正本とする。
"""

from __future__ import annotations

from pathlib import Path
import re


ROADMAP = [
    ("01", "bootstrap-health", "プロジェクト雛形・/health・compose・Dockerfile", "done", "[]"),
    ("02", "migration-0001", "migration 0001", "done", "[01]"),
    ("03", "migrations-0002-0004", "migration 0002〜0004", "done", "[02]"),
    ("04", "problem-details", "AppError / problem+json", "done", "[03]"),
    ("05", "run-lock", "RunLock / advisory lock", "done", "[04]"),
    ("06", "runs-endpoint", "POST /v1/runs 骨格", "done", "[05]"),

    ("Q1", "ci-quality-gate", "CI / Quality Gate 基盤", "planned", "[06]"),
    ("Q2", "main-branch-protection", "main branch protection", "planned", "[Q1]"),
    ("Q3", "dependabot", "Dependabot / dependency operations", "planned", "[Q2]"),

    (
        "07",
        "stale-lock-recovery",
        "取り残し lock 回収 + run入口仕様の仕上げ",
        "planned",
        "[06, Q3]",
    ),
    ("08", "probe-masking", "probe + masking", "planned", "[07]"),
    ("09", "domain-status", "domain/status", "planned", "[08]"),
    ("10", "incident-service", "incident_service", "planned", "[09]"),
    ("11", "notifier-outbox", "Notifier + Slack + outbox_service", "planned", "[10]"),
    ("12", "notifications-auth", "POST /v1/notifications + auth", "planned", "[11]"),
    ("13", "daily-aggregation", "日次集計 + 90日削除", "planned", "[12]"),
    ("14", "status-page", "status page / askama", "planned", "[13]"),
    ("15", "github-actions", "GitHub Actions", "planned", "[14]"),
    ("16", "openapi-ci-runbook", "OpenAPI + CI + Runbook", "planned", "[15]"),
]


Q_TASK_RE = re.compile(r"^Q[1-9]\d*$", re.IGNORECASE)
NUMERIC_TASK_RE = re.compile(r"^\d{2}$")


def numeric_task_id(task_id: str) -> int | None:
    """数値系列なら整数値を返し、Q系列ならNoneを返す。"""
    if not NUMERIC_TASK_RE.fullmatch(task_id):
        return None
    return int(task_id)


def is_legacy_task(task_id: str) -> bool:
    """Task 01〜06 はlegacy。"""
    number = numeric_task_id(task_id)
    return number is not None and 1 <= number <= 6


def is_managed_task(task_id: str) -> bool:
    """Q系列とTask 07以降をmanaged taskとする。"""
    if Q_TASK_RE.fullmatch(task_id):
        return True

    number = numeric_task_id(task_id)
    return number is not None and number >= 7


def parse_frontmatter(path: Path) -> dict[str, str]:
    text = path.read_text(encoding="utf-8")
    match = re.match(r"^---\n(.*?)\n---\n", text, re.DOTALL)
    if not match:
        return {}

    data: dict[str, str] = {}

    for line in match.group(1).splitlines():
        if ":" not in line or line.lstrip().startswith("#"):
            continue

        key, value = line.split(":", 1)
        data[key.strip()] = value.strip().strip("'\"")

    return data


def task_filename_pattern(task_id: str) -> re.Pattern[str]:
    """task ID に対応するlegacy/v3両対応のファイル名patternを返す。"""
    if Q_TASK_RE.fullmatch(task_id):
        # Q系列は Q1 / Q2 / ... をそのままIDとして扱う。
        return re.compile(
            rf"^task[-_ ]{re.escape(task_id)}(?:[-_ ].*)?\.md$",
            re.IGNORECASE,
        )

    number = numeric_task_id(task_id)
    if number is not None:
        # 既存legacy docsの task_07_foo.md 等も認識する。
        return re.compile(
            rf"^task[-_ ]0*{number}(?:[-_ ].*)?\.md$",
            re.IGNORECASE,
        )

    raise ValueError(f"unsupported task id: {task_id!r}")


def find_task_doc(task_id: str) -> Path | None:
    """Legacy の大文字・underscore名とv3のkebab-case名を両方見つける。"""
    pattern = task_filename_pattern(task_id)

    candidates = sorted(
        path
        for path in Path("docs").glob("*.md")
        if pattern.match(path.name)
    )

    return candidates[0] if candidates else None


rows: list[tuple[str, str, str, str, str, str]] = []

for task_id, default_slug, title, default_status, default_depends in ROADMAP:
    path = find_task_doc(task_id)

    # 01〜06はlegacy成果物を凍結し、ROADMAP上の値を使う。
    # Q系列および07以降はfrontmatterを状態の正本とする。
    meta = (
        parse_frontmatter(path)
        if path is not None and is_managed_task(task_id)
        else {}
    )

    slug = meta.get("slug", default_slug)
    status = meta.get("status", default_status)
    depends = meta.get("depends_on", default_depends)
    file_display = path.as_posix() if path else "—"

    rows.append(
        (
            task_id,
            slug,
            status,
            depends,
            title,
            file_display,
        )
    )


next_task_id = next(
    (
        task_id
        for task_id, _slug, status, _depends, _title, _file in rows
        if status != "done"
    ),
    None,
)


out = [
    "# Task Index",
    "",
    (
        "`docs/implementation-plan.md` の既存16タスクにQ系列の品質タスクを加え、"
        "`make task-index` で生成する。"
    ),
    (
        "Task 01〜06はlegacy task docsを変更せずdone扱い。"
        "Q系列およびTask 07以降はfrontmatterを状態の正本とする。"
    ),
    "",
    f"**次タスク: {next_task_id or 'なし'}**",
    "",
    "| ID | Slug | Status | Depends on | Task | File |",
    "|---:|---|---|---|---|---|",
]

for row in rows:
    out.append("| " + " | ".join(row) + " |")


Path("docs/task-INDEX.md").write_text(
    "\n".join(out) + "\n",
    encoding="utf-8",
)