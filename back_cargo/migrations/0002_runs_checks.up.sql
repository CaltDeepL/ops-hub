-- =============================================================================
-- 0002_runs_checks (up)
-- 対応：基本設計 2.5 runs / 2.6 checks / 2.7 check_dailies
--
-- ENUM は 0001 で作成済み。ここでは型を定義しない。
-- =============================================================================

-- ---------------------------------------------------------------- runs
-- 「1巡の実行」の記録。観測用であり、排他制御の手段ではない（基本設計 3.2）。
CREATE TABLE runs (
  id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  started_at         timestamptz NOT NULL DEFAULT now(),
  finished_at        timestamptz,
  status             run_status NOT NULL DEFAULT 'running',
  targets_checked    integer NOT NULL DEFAULT 0,
  notifications_sent integer NOT NULL DEFAULT 0,
  error              text,
  CONSTRAINT runs_finished_after_started CHECK (finished_at IS NULL OR finished_at >= started_at),
  CONSTRAINT runs_counts_non_negative    CHECK (targets_checked >= 0 AND notifications_sent >= 0)
);

COMMENT ON TABLE  runs IS '実行の記録。排他制御は advisory lock で行い、この表では行わない（基本設計 3.2）';
COMMENT ON COLUMN runs.status IS 'running のまま10分以上放置された行はスイーパーが failed に倒す（基本設計 3.4）';

CREATE INDEX runs_started_at_idx ON runs (started_at DESC);

-- スイーパーと「実行中の run_id を引く」問い合わせ（詳細設計 2.2）が使う。
-- running は同時に高々1件なので、部分索引で十分小さい。
CREATE INDEX runs_running_idx ON runs (started_at DESC) WHERE status = 'running';

-- ---------------------------------------------------------------- checks
-- 1回ごとのチェック結果。90日で削除する原因調査用の生データ（N-14）。
CREATE TABLE checks (
  id           bigserial PRIMARY KEY,
  target_id    uuid NOT NULL REFERENCES targets(id) ON DELETE RESTRICT,
  run_id       uuid NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  started_at   timestamptz NOT NULL,
  duration_ms  integer NOT NULL,
  result       check_result NOT NULL,
  status_code  integer,
  degraded     boolean NOT NULL DEFAULT false,
  error_detail text,
  CONSTRAINT checks_duration_non_negative CHECK (duration_ms >= 0),
  CONSTRAINT checks_error_detail_len      CHECK (error_detail IS NULL OR length(error_detail) <= 512),
  CONSTRAINT checks_status_code_range     CHECK (status_code IS NULL OR status_code BETWEEN 100 AND 599)
);

COMMENT ON COLUMN checks.error_detail IS '失敗時のみ。マスキング後に512文字へ切り詰めたもの（詳細設計 4章）';
COMMENT ON COLUMN checks.degraded IS 'success かつ duration_ms > degraded_threshold_ms のとき true。状態遷移には影響しない';

-- 主キーが bigserial なのは、時系列の挿入と90日削除のレンジスキャンで uuid より扱いやすいため。
CREATE INDEX checks_target_started_idx ON checks (target_id, started_at DESC);
CREATE INDEX checks_run_idx            ON checks (run_id);

-- N-14 の `DELETE FROM checks WHERE started_at < ...`（詳細設計 5.3）が使う。
CREATE INDEX checks_started_at_idx ON checks (started_at);

-- ---------------------------------------------------------------- check_dailies
-- 日次集計（恒久保持）。ステータス画面（F-8）はこの表のみを参照する。
-- 日付境界はJSTで切る（詳細設計 5.1）。
CREATE TABLE check_dailies (
  target_id      uuid NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
  day            date NOT NULL,
  total_count    integer NOT NULL,
  success_count  integer NOT NULL,
  degraded_count integer NOT NULL DEFAULT 0,
  p50_ms         integer,
  p95_ms         integer,
  PRIMARY KEY (target_id, day),
  CONSTRAINT check_dailies_success_le_total    CHECK (success_count <= total_count),
  CONSTRAINT check_dailies_degraded_le_success CHECK (degraded_count <= success_count),
  CONSTRAINT check_dailies_counts_non_negative CHECK (total_count >= 0 AND success_count >= 0 AND degraded_count >= 0),
  CONSTRAINT check_dailies_percentiles_non_negative CHECK (
    (p50_ms IS NULL OR p50_ms >= 0) AND (p95_ms IS NULL OR p95_ms >= 0)
  )
);

COMMENT ON COLUMN check_dailies.day IS 'JSTの日付（詳細設計 5.1）。ステータス画面の1マスに対応する';
COMMENT ON COLUMN check_dailies.p95_ms IS '成功チェックのみで算出する。タイムアウトを含めるとp95が90000msに張り付くため';