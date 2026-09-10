# migration / SQLx レビューチェックリスト

対象: `migrations/`, schema変更, `.sqlx/`, SQLx compile-time query変更。

- [ ] 適用済みmigrationを編集していない。新規migrationになっている。
- [ ] `NOT NULL` / DROP / RENAME / 型縮小は段階適用とbackfillがtask docにある。
- [ ] 既存行が新constraintを満たす移行順になっている。
- [ ] deploy前後に旧/新binaryが混在し得る場合の互換性を考慮している。
- [ ] constraint / unique indexがdomain不変条件をDBでも守っている。
- [ ] transaction/lockが必要なmigrationなら明示されている。
- [ ] query変更に応じて `.sqlx/` が更新されている。
- [ ] `.sqlx/` 更新に `make sqlx-prepare` を使っている。
- [ ] `make verify` がlocal/CI PostgreSQLで成功している。
- [ ] Neon本番DBをprepare/testへ使用していない。
