-- =============================================================================
-- 0002〜0004 の制約・索引の挙動確認スクリプト
--
--   psql "$DATABASE_URL" -f scripts/verify_0002_0004.sql
--
-- ※ このファイルは migrations/ には置かないこと。sqlx がバージョン番号を
--    読み取れず、マイグレーション履歴が壊れる（タスク02の事故）。
-- ※ 実行後にテストデータは自分で消す（末尾のクリーンアップ）。
-- =============================================================================

CREATE OR REPLACE FUNCTION expect_error(stmt text, label text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  BEGIN
    EXECUTE stmt;
    RAISE NOTICE 'NG   % : 弾かれなかった', label;
  EXCEPTION WHEN others THEN
    RAISE NOTICE 'OK   % : % [%]', label, left(SQLERRM, 55), SQLSTATE;
  END;
END; $$;

-- 検証用の土台
INSERT INTO targets (name, url, severity) VALUES ('verify-target', 'https://verify.example.com/health', 'sev2');
INSERT INTO runs (id) VALUES ('11111111-1111-1111-1111-111111111111');

-- ------------------------------------------------------------------ runs
SELECT expect_error($$INSERT INTO runs (started_at, finished_at) VALUES (now(), now() - interval '1 minute')$$, 'finished < started を拒否   ');
SELECT expect_error($$INSERT INTO runs (targets_checked) VALUES (-1)$$,                                        '負のチェック件数を拒否      ');

-- ------------------------------------------------------------------ checks
INSERT INTO checks (target_id, run_id, started_at, duration_ms, result, status_code)
SELECT id, '11111111-1111-1111-1111-111111111111', now(), 1200, 'success', 200 FROM targets WHERE name = 'verify-target';

SELECT expect_error($$INSERT INTO checks (target_id, run_id, started_at, duration_ms, result)
  SELECT id, '11111111-1111-1111-1111-111111111111', now(), -1, 'success' FROM targets WHERE name='verify-target'$$, '負の応答時間を拒否          ');
SELECT expect_error($$INSERT INTO checks (target_id, run_id, started_at, duration_ms, result, error_detail)
  SELECT id, '11111111-1111-1111-1111-111111111111', now(), 10, 'timeout', repeat('x', 513) FROM targets WHERE name='verify-target'$$, 'error_detail 513文字を拒否  ');
SELECT expect_error($$INSERT INTO checks (target_id, run_id, started_at, duration_ms, result, status_code)
  SELECT id, '11111111-1111-1111-1111-111111111111', now(), 10, 'http_error', 99 FROM targets WHERE name='verify-target'$$, '範囲外ステータスを拒否      ');
SELECT expect_error($$DELETE FROM targets WHERE name = 'verify-target'$$,                                      'checks 参照中の target 削除を拒否');

-- ------------------------------------------------------------------ check_dailies
INSERT INTO check_dailies (target_id, day, total_count, success_count, degraded_count, p50_ms, p95_ms)
SELECT id, current_date, 24, 23, 2, 800, 4200 FROM targets WHERE name = 'verify-target';

SELECT expect_error($$INSERT INTO check_dailies (target_id, day, total_count, success_count)
  SELECT id, current_date - 1, 10, 11 FROM targets WHERE name='verify-target'$$, '成功数>総数を拒否           ');
SELECT expect_error($$INSERT INTO check_dailies (target_id, day, total_count, success_count, degraded_count)
  SELECT id, current_date - 2, 10, 5, 6 FROM targets WHERE name='verify-target'$$, 'degraded>成功数を拒否       ');

-- UPSERT（詳細設計 5.1 と同じ ON CONFLICT）で当日分が上書きされること
INSERT INTO check_dailies (target_id, day, total_count, success_count, degraded_count, p50_ms, p95_ms)
SELECT id, current_date, 25, 24, 2, 810, 4300 FROM targets WHERE name = 'verify-target'
ON CONFLICT (target_id, day) DO UPDATE SET
  total_count = EXCLUDED.total_count, success_count = EXCLUDED.success_count,
  degraded_count = EXCLUDED.degraded_count, p50_ms = EXCLUDED.p50_ms, p95_ms = EXCLUDED.p95_ms;

DO $$
DECLARE n int; t int; BEGIN
  SELECT count(*), max(total_count) INTO n, t FROM check_dailies WHERE day = current_date;
  IF n = 1 AND t = 25 THEN RAISE NOTICE 'OK   日次集計のUPSERTが1行に収束（total=%）', t;
  ELSE RAISE NOTICE 'NG   UPSERTが収束していない（行数=%, total=%）', n, t; END IF;
END $$;

-- ------------------------------------------------------------------ incidents
INSERT INTO incidents (target_id, severity, started_at)
SELECT id, severity, now() - interval '2 hours' FROM targets WHERE name = 'verify-target';

SELECT expect_error($$INSERT INTO incidents (target_id, severity, started_at)
  SELECT id, severity, now() FROM targets WHERE name='verify-target'$$,          '未解決インシデント2件目を拒否');
SELECT expect_error($$INSERT INTO incidents (target_id, severity, started_at, resolved_at)
  SELECT id, severity, now(), now() - interval '1 hour' FROM targets WHERE name='verify-target'$$, 'resolved < started を拒否   ');

UPDATE incidents SET resolved_at = now() WHERE resolved_at IS NULL;
INSERT INTO incidents (target_id, severity, started_at)
SELECT id, severity, now() FROM targets WHERE name = 'verify-target';

DO $$
DECLARE n int; BEGIN
  SELECT count(*) INTO n FROM incidents;
  IF n = 2 THEN RAISE NOTICE 'OK   復旧後は次のインシデントを開ける（計%件）', n;
  ELSE RAISE NOTICE 'NG   インシデント件数が想定外（%件）', n; END IF;
END $$;

-- ------------------------------------------------------------------ events
INSERT INTO events (source, level, title, idempotency_key)
VALUES ('asset-log', 'error', '日次スナップショットの生成に失敗しました', 'snapshot-2026-09-05');

SELECT expect_error($$INSERT INTO events (source, level, title, idempotency_key)
  VALUES ('asset-log','error','再送','snapshot-2026-09-05')$$,                   '同一source+冪等キーを拒否   ');
SELECT expect_error($$INSERT INTO events (source, level, title, idempotency_key)
  VALUES ('asset-log','info', repeat('t', 201), 'long-title')$$,                 'title 201文字を拒否         ');

-- D-9：source が違えば同じ冪等キーを使える
INSERT INTO events (source, level, title, idempotency_key)
VALUES ('chess', 'info', '同じキーだが別サービス', 'snapshot-2026-09-05');

DO $$
DECLARE n int; BEGIN
  SELECT count(*) INTO n FROM events WHERE idempotency_key = 'snapshot-2026-09-05';
  IF n = 2 THEN RAISE NOTICE 'OK   別 source なら同じ冪等キーを使える（%件）', n;
  ELSE RAISE NOTICE 'NG   D-9 のスコープが効いていない（%件）', n; END IF;
END $$;

-- ------------------------------------------------------------------ outbox
INSERT INTO outbox (source_kind, source_id, dedupe_key, payload)
SELECT 'incident', id, 'incident:' || id || ':opened', '{"kind":"incident_opened"}'::jsonb
FROM incidents WHERE resolved_at IS NULL;

SELECT expect_error($$INSERT INTO outbox (source_kind, source_id, dedupe_key, payload)
  SELECT 'incident', id, 'incident:' || id || ':opened', '{}'::jsonb FROM incidents WHERE resolved_at IS NULL$$, 'dedupe_key の重複を拒否     ');
SELECT expect_error($$UPDATE outbox SET attempts = 6$$,                                                        'attempts 6 を拒否           ');
SELECT expect_error($$UPDATE outbox SET status = 'sent'$$,                                                     'sent なのに sent_at 無しを拒否');
SELECT expect_error($$UPDATE outbox SET sent_at = now()$$,                                                     'pending なのに sent_at 有りを拒否');

UPDATE outbox SET status = 'sent', sent_at = now();
DO $$
DECLARE s text; BEGIN
  SELECT status::text INTO s FROM outbox;
  IF s = 'sent' THEN RAISE NOTICE 'OK   status と sent_at を同時更新すれば通る';
  ELSE RAISE NOTICE 'NG   配信済みへの更新に失敗'; END IF;
END $$;

-- ------------------------------------------------------------------ 索引の確認
DO $$
DECLARE missing text;
BEGIN
  SELECT string_agg(expected, ', ') INTO missing
  FROM (VALUES
    ('runs_started_at_idx'), ('runs_running_idx'),
    ('checks_target_started_idx'), ('checks_run_idx'), ('checks_started_at_idx'),
    ('incidents_one_open_per_target'), ('incidents_target_started_idx'),
    ('events_source_idempotency_key'),
    ('outbox_dedupe_key'), ('outbox_pending_idx'), ('outbox_failed_idx')
  ) AS t(expected)
  WHERE NOT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname = 'public' AND indexname = t.expected);

  IF missing IS NULL THEN RAISE NOTICE 'OK   索引11件がすべて生成されている';
  ELSE RAISE NOTICE 'NG   索引が不足: %', missing; END IF;
END $$;

-- ------------------------------------------------------------------ クリーンアップ
DELETE FROM outbox;
DELETE FROM events;
DELETE FROM incidents WHERE target_id IN (SELECT id FROM targets WHERE name = 'verify-target');
DELETE FROM check_dailies WHERE target_id IN (SELECT id FROM targets WHERE name = 'verify-target');
DELETE FROM checks WHERE target_id IN (SELECT id FROM targets WHERE name = 'verify-target');
DELETE FROM runs WHERE id = '11111111-1111-1111-1111-111111111111';
DELETE FROM targets WHERE name = 'verify-target';

DROP FUNCTION expect_error(text, text);