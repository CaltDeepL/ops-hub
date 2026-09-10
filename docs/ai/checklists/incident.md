# incident / dead-man レビューチェックリスト

対象: target state、incident、liveness、flapping、dead-man。

- [ ] 許可される状態遷移が列挙され、その他が不可能/拒否される。
- [ ] 同じ観測の再処理でincident/eventを重複生成しない。
- [ ] recovery判定がstale/out-of-orderデータで誤復旧しない。
- [ ] 閾値の直前/一致/直後をtestしている。
- [ ] Render cold startを通常障害と区別する既存要件を壊していない。
- [ ] dead-manが「監視対象停止」と「ops-hub自身停止」を混同しない。
- [ ] clock/time-zone前提が明示され、testが実時間待ちに依存しない。
- [ ] 同一targetの未解決incident重複をDB/transactionで防ぐ。
- [ ] `target_states.current_incident_id` 等の整合性を同一transactionで守る設計と矛盾しない。
- [ ] 状態遷移理由を後から診断できる永続化/ログがある。
