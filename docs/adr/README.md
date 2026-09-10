# Architecture Decision Records

Task文書は「今回何を実装するか」、ADRは「なぜ長期的にその設計を選んだか」を残す。

ADR候補:

- advisory lockを採用した理由
- lock競合を409ではなく200にした理由
- outbox dedupe境界
- incidentの部分unique index
- Probeをtrait化しない理由
- Clockをtrait化する理由
- failure domainの分離

既存の決定を後から書き換えず、変更時は新しいADRでsupersedeする。
