-- =============================================================================
-- 0004_events_outbox (up)
-- 対応：基本設計 2.9 events / 2.10 outbox / D-4 / D-9
-- =============================================================================

-- ---------------------------------------------------------------- events
-- 通知受付API（F-5）が受理したイベント。受理と配信を分ける（配信は outbox）。
CREATE TABLE events (
  id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  source          text NOT NULL,
  level           event_level NOT NULL,
  title           text NOT NULL,
  body            text,
  idempotency_key text NOT NULL,
  received_at     timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT events_source_not_blank          CHECK (btrim(source) <> ''),
  CONSTRAINT events_idempotency_key_not_blank CHECK (btrim(idempotency_key) <> ''),
  CONSTRAINT events_title_not_blank           CHECK (btrim(title) <> ''),
  CONSTRAINT events_title_len                 CHECK (length(title) <= 200),
  CONSTRAINT events_body_len                  CHECK (body IS NULL OR length(body) <= 2000)
);

COMMENT ON COLUMN events.source IS 'トークンから解決した送信元。リクエストボディの値は採用しない（詳細設計 2.3）';

-- D-9：冪等キーは source でスコープする。グローバル一意にすると、別サービスが
-- 偶然同じキー（"daily-report" など）を使ったときに通知が消える。
CREATE UNIQUE INDEX events_source_idempotency_key ON events (source, idempotency_key);

-- ---------------------------------------------------------------- outbox
-- 配信待ちの通知キュー（N-1〜N-3）。incidents / events から 1:N で生成される。
CREATE TABLE outbox (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  source_kind outbox_source_kind NOT NULL,
  source_id   uuid NOT NULL,
  dedupe_key  text NOT NULL,
  payload     jsonb NOT NULL,
  status      outbox_status NOT NULL DEFAULT 'pending',
  attempts    integer NOT NULL DEFAULT 0,
  not_before  timestamptz NOT NULL DEFAULT now(),
  last_error  text,
  created_at  timestamptz NOT NULL DEFAULT now(),
  sent_at     timestamptz,
  CONSTRAINT outbox_attempts_range      CHECK (attempts BETWEEN 0 AND 5),
  CONSTRAINT outbox_sent_has_timestamp  CHECK ((status = 'sent') = (sent_at IS NOT NULL)),
  CONSTRAINT outbox_dedupe_key_not_blank CHECK (btrim(dedupe_key) <> '')
);

COMMENT ON COLUMN outbox.dedupe_key IS '二重通知を防ぐ最終防衛線（D-4）。incident:{id}:opened / :resolved / event:{id}';
COMMENT ON COLUMN outbox.not_before IS 'リトライ待機（詳細設計 4.4）と Sev2 の夜間キューイング（要件 9.1）を兼ねる';
COMMENT ON COLUMN outbox.source_id IS 'incidents.id または events.id への論理参照。FKは張らない（source_kind で解釈が変わるため）';

-- D-4：同じ状態遷移からは同じキーが生成されるため、多重起動しても
-- 2件目の INSERT が 23505 で弾かれる。アプリのロジックが間違えても二重通知にならない。
CREATE UNIQUE INDEX outbox_dedupe_key ON outbox (dedupe_key);

-- 配信対象（status='pending' かつ not_before <= now()）の取り出しに使う。
CREATE INDEX outbox_pending_idx ON outbox (not_before) WHERE status = 'pending';

-- 運用当番（O-2）が「諦めた通知」を確認するための索引。件数は少ないが定期的に見る。
CREATE INDEX outbox_failed_idx ON outbox (created_at DESC) WHERE status = 'failed';