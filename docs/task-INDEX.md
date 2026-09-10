# Task Index

`docs/implementation-plan.md` の16タスクを基準に `make task-index` で生成する。
Task 01〜06はlegacy task docsを変更せずdone扱い。Task 07以降はfrontmatterを状態の正本とする。

**次タスク: 07**

| ID | Slug | Status | Depends on | Task | File |
|---:|---|---|---|---|---|
| 01 | bootstrap-health | done | [] | プロジェクト雛形・/health・compose・Dockerfile | docs/Task_01_skeleton.md |
| 02 | migration-0001 | done | [01] | migration 0001 | docs/Task_02_migration 0001 .md |
| 03 | migrations-0002-0004 | done | [02] | migration 0002〜0004 | docs/Task_03_migrations 0002 0004.md |
| 04 | problem-details | done | [03] | AppError / problem+json | — |
| 05 | run-lock | done | [04] | RunLock / advisory lock | docs/Task_05_run lock.md |
| 06 | runs-endpoint | done | [05] | POST /v1/runs 骨格 | docs/Task_06 post runs.md |
| 07 | stale-lock-recovery | planned | [06] | 取り残し lock 回収 + run入口仕様の仕上げ | — |
| 08 | probe-masking | planned | [07] | probe + masking | — |
| 09 | domain-status | planned | [08] | domain/status | — |
| 10 | incident-service | planned | [09] | incident_service | — |
| 11 | notifier-outbox | planned | [10] | Notifier + Slack + outbox_service | — |
| 12 | notifications-auth | planned | [11] | POST /v1/notifications + auth | — |
| 13 | daily-aggregation | planned | [12] | 日次集計 + 90日削除 | — |
| 14 | status-page | planned | [13] | status page / askama | — |
| 15 | github-actions | planned | [14] | GitHub Actions | — |
| 16 | openapi-ci-runbook | planned | [15] | OpenAPI + CI + Runbook | — |
