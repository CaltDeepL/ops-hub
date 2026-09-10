#!/usr/bin/env python3
"""既存16タスクの正本を維持しながらdocs/task-INDEX.mdを生成する。"""

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
    ("07", "stale-lock-recovery", "取り残し lock 回収 + run入口仕様の仕上げ", "planned", "[06]"),
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


def find_task_doc(task_id: str) -> Path | None:
    """Legacy の大文字・underscore 名と v3 の kebab-case 名を両方見つける。"""
    pattern = re.compile(rf"^task[-_ ]0*{int(task_id)}(?:[-_ ].*)?\.md$", re.IGNORECASE)
    candidates = sorted(
        path for path in Path("docs").glob("*.md") if pattern.match(path.name)
    )
    return candidates[0] if candidates else None


rows = []
for task_id, default_slug, title, default_status, default_depends in ROADMAP:
    path = find_task_doc(task_id)

    meta = parse_frontmatter(path) if path and int(task_id) >= 7 else {}
    slug = meta.get("slug", default_slug)
    status = meta.get("status", default_status)
    depends = meta.get("depends_on", default_depends)
    file_display = path.as_posix() if path else "—"

    rows.append((task_id, slug, status, depends, title, file_display))

out = [
    "# Task Index",
    "",
    "`docs/implementation-plan.md` の16タスクを基準に `make task-index` で生成する。",
    "Task 01〜06はlegacy task docsを変更せずdone扱い。Task 07以降はfrontmatterを状態の正本とする。",
    "",
    "**次タスク: 07**",
    "",
    "| ID | Slug | Status | Depends on | Task | File |",
    "|---:|---|---|---|---|---|",
]

for row in rows:
    out.append("| " + " | ".join(row) + " |")

Path("docs/task-INDEX.md").write_text("\n".join(out) + "\n", encoding="utf-8")
