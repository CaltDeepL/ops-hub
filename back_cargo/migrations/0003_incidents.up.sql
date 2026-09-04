-- =============================================================================
-- 0003_incidents (up)
-- 対応：基本設計 2.8 incidents / D-5
-- =============================================================================

CREATE TABLE incidents (
  id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  target_id      uuid NOT NULL REFERENCES targets(id) ON DELETE RESTRICT,
  severity       severity NOT NULL,
  started_at     timestamptz NOT NULL,
  resolved_at    timestamptz,
  cause_note     text,
  postmortem_url text,
  CONSTRAINT incidents_resolved_after_started CHECK (resolved_at IS NULL OR resolved_at > started_at),
  CONSTRAINT incidents_postmortem_url_scheme  CHECK (postmortem_url IS NULL OR postmortem_url <> '')
);

COMMENT ON TABLE  incidents IS '障害の発生から復旧までを1レコードで表す（要件 F-6）';
COMMENT ON COLUMN incidents.severity IS 'targets.severity のスナップショット。設定を後から変えても過去の重大度は書き換えない';

-- D-5：「1つの対象に未解決のインシデントは1件まで」をDB側で保証する。
-- 多重起動やロジックの不整合で二重に開くことを、アプリに頼らず防ぐ。
CREATE UNIQUE INDEX incidents_one_open_per_target
  ON incidents (target_id) WHERE resolved_at IS NULL;

CREATE INDEX incidents_target_started_idx ON incidents (target_id, started_at DESC);

-- 継続時間は列として持たず、resolved_at - started_at で都度算出する（基本設計 2.8）。