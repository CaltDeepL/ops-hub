# ops-hub — CI / monitor workflow 方針

## 2種類を混ぜない

ops-hubには意味の異なるGitHub Actionsがある。

### 1. 運用監視 workflow

`monitor.yml` は `POST /v1/runs` を定期的に叩く dead-man / scheduler相当。

設計上は次の性質を持つ。

- scheduled + workflow_dispatch
- 毎時00分を避けて実行
- cold startを考慮したHTTP timeout
- 200/202を成功扱い
- workflow失敗はops-hub/Slackと独立した通知経路になる

これは **アプリコード品質CIではない**。

Task 15の範囲なので、それ以前のAI scaffold導入だけを理由に変更しない。

### 2. コード品質 CI

ロードマップ上はTask 16「OpenAPI + CI + Runbook」で本実装する。

Task 16で品質CIを確定するときは、Cargoコマンドをworkflowへ重複記述せず、ローカルPostgreSQL service/schemaを準備した後に次だけを品質ゲートとして呼ぶ。

```yaml
- name: Verify
  env:
    DATABASE_URL: postgres://ops_hub:ops_hub@localhost:5433/ops_hub
  run: make verify
```

CI側のPostgreSQL公開portが5432なら、そのCI用DATABASE_URLを使ってよい。`make verify` はhostを検査しportは固定しない。

## Task 16以前

Task 16が未実装なら、AI scaffold導入のためだけに新規quality workflowを先回りして作らない。

ローカル最終ゲートとして `make verify` を先に固定し、Task 16でそのままCIから呼ぶ。

## Security

品質CIにNeon本番 `DATABASE_URL` を渡さない。

本番secretをSQLx compile-time verificationのために使わない。
