# CLAUDE.md

AI エージェント（Claude Code・Cursor など）向けの作業ガイド。人向けの詳細は README.md を正とする。

## プロジェクト

複数の個人サービスを外部から定期監視し、異常時に通知する監視・通知ハブ（開発中）。

- バックエンド: `back_cargo/`（Rust / Axum / Tokio / SQLx）
- フロントエンド: `src/`（React / Vite の雛形）。`package.json` はリポジトリ直下
- 常駐スケジューラを持たない。外部のスケジューラが `POST /v1/runs` で 1 巡を開始する
- 同時実行は PostgreSQL の advisory lock で 1 巡に制限する

## よく使うコマンド

```bash
# 起動（リポジトリ直下）
docker compose -f back_cargo/compose.yaml up --build -d
curl -fsS http://localhost:8081/health

# 品質ゲート（ローカルと CI で同じ。リポジトリ直下）
make verify          # DB 接続先の確認 → Ruleset 契約 → fmt → clippy → sqlx prepare --check → test
make sqlx-prepare    # SQL を変えたら .sqlx/ を更新する

# フロントエンド（リポジトリ直下）
npm ci
npm run lint
npm run build
```

ローカルのポート: API `8081`、PostgreSQL `5433`（ホスト）/ `5432`（コンテナ内）。
`make verify` はローカル以外の `DATABASE_URL` を拒否する（本番 DB でテストを走らせないため）。

## 守ること

- `make verify` が通らない変更をコミットしない
- SQLx はコンパイル時検証を使う。例外は `pg_locks` などシステムカタログへの問い合わせ（実行時クエリ）
- stale run（DB に残った `running`）は巡回開始時の sweeper で自動回収する。stale advisory lock は CLI の明示操作でのみ解放する。この 2 つを混ぜない
- advisory lock の競合は障害ではなく「すでに実行中」として 200 で返す
- GitHub の Ruleset を変えたら、正本 `scripts/github/main-ruleset.json` も同じ内容に直す（`scripts/github/verify_main_ruleset.py` で一致を確かめる）

## 作業の進め方

- タスクは GitHub Issue（ラベル `task`、テンプレートは [CaltDeepL/.github](https://github.com/CaltDeepL/.github)）で管理し、作業メモもその Issue に書く（2026-10-24 以降）
- 過去の作業メモ `docs/Task_*.md` は記録として残す。新しく追加しない
- 現在の動作を判断するときは、過去の文書より実コード・テスト・CI を優先する
- ブランチ → PR → CI（`verify`）→ squash マージ。`main` へ直接 push しない
- コミットは Conventional Commits。PR 本文に `Closes #NN` を書く

## やってはいけないこと

- `.env` や秘密情報（接続文字列・トークン）をコミット・出力しない
- `main` の Ruleset を直接変更しない、force push しない
- 確認なしに Issue・PR の作成や GitHub の設定変更をしない
