# Task Index

`docs/implementation-plan.md` の既存16タスクにQ系列の品質タスクを加え、`make task-index` で生成する。
Task 01〜06はlegacy task docsを変更せずdone扱い。Q系列およびTask 07以降はfrontmatterを状態の正本とする。

**次タスク: Q2**

| ID | Slug | Status | Depends on | Task | File |
|---:|---|---|---|---|---|
| 01 | bootstrap-health | done | [] | プロジェクト雛形・/health・compose・Dockerfile | docs/Task_01_skeleton.md |
| 02 | migration-0001 | done | [01] | migration 0001 | docs/Task_02_migration 0001 .md |
| 03 | migrations-0002-0004 | done | [02] | migration 0002〜0004 | docs/Task_03_migrations 0002 0004.md |
| 04 | problem-details | done | [03] | AppError / problem+json | docs/Task_04_apperror problem json.md |
| 05 | run-lock | done | [04] | RunLock / advisory lock | docs/Task_05_run lock.md |
| 06 | runs-endpoint | done | [05] | POST /v1/runs 骨格 | docs/Task_06 post runs.md |
| Q1 | ci-quality-gate | done | ["06"] | CI / Quality Gate 基盤 | docs/task-Q1-ci-quality-gate.md |
| Q2 | main-branch-protection | implementing | ["Q1"] | main branch protection | docs/task-Q2-main-branch-protection.md |
| Q3 | dependabot | planned | [Q2] | Dependabot / dependency operations | — |
| 07 | stale-lock-recovery | planned | [06, Q3] | 取り残し lock 回収 + run入口仕様の仕上げ | — |
| 08 | probe-masking | planned | [07] | probe + masking | — |
| 09 | domain-status | planned | [08] | domain/status | — |
| 10 | incident-service | planned | [09] | incident_service | — |
| 11 | notifier-outbox | planned | [10] | Notifier + Slack + outbox_service | — |
| 12 | notifications-auth | planned | [11] | POST /v1/notifications + auth | — |
| 13 | daily-aggregation | planned | [12] | 日次集計 + 90日削除 | — |
| 14 | status-page | planned | [13] | status page / askama | — |
| 15 | github-actions | planned | [14] | GitHub Actions | — |
| 16 | openapi-ci-runbook | planned | [15] | OpenAPI + CI + Runbook | — |
