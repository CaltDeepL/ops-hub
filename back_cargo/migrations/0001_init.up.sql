-- =============================================================================
-- 0001_init (up)
-- 対応：基本設計 2.1 ENUM / 2.2 マイグレーション分割 / 2.3 targets / 2.4 target_states
--
-- gen_random_uuid() は PostgreSQL 13 以降で組み込み。pgcrypto は不要。
-- =============================================================================

-- ---------------------------------------------------------------- ENUM（全種）
-- 後続マイグレーション（0002〜0004）で参照する型もここでまとめて作る。
-- 型だけ先に置くことで、0002以降を独立して revert できる。
CREATE TYPE severity           AS ENUM ('sev1', 'sev2', 'sev3');
CREATE TYPE target_status      AS ENUM ('up', 'down');
CREATE TYPE check_result       AS ENUM ('success', 'timeout', 'http_error', 'connection_error');
CREATE TYPE event_level        AS ENUM ('info', 'warn', 'error');
CREATE TYPE outbox_status      AS ENUM ('pending', 'sent', 'failed');
CREATE TYPE outbox_source_kind AS ENUM ('incident', 'event');
CREATE TYPE run_status         AS ENUM ('running', 'completed', 'failed');

-- ---------------------------------------------------------------- 共通関数
CREATE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END;
$$;

-- ---------------------------------------------------------------- targets
-- 監視対象の「設定」。唯一の情報源（要件 F-1）。機械が毎回書き換える状態は持たない。
CREATE TABLE targets (
  id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name                  text NOT NULL,
  url                   text NOT NULL,
  method                text NOT NULL DEFAULT 'GET',
  expected_status       integer NOT NULL DEFAULT 200,
  timeout_ms            integer NOT NULL DEFAULT 90000,
  degraded_threshold_ms integer NOT NULL DEFAULT 30000,
  severity              severity NOT NULL,
  enabled               boolean NOT NULL DEFAULT true,
  created_at            timestamptz NOT NULL DEFAULT now(),
  updated_at            timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT targets_name_not_blank        CHECK (btrim(name) <> ''),
  CONSTRAINT targets_url_https             CHECK (url LIKE 'https://%'),
  CONSTRAINT targets_method_allowed        CHECK (method IN ('GET', 'HEAD')),
  CONSTRAINT targets_expected_status_range CHECK (expected_status BETWEEN 100 AND 599),
  CONSTRAINT targets_timeout_range         CHECK (timeout_ms BETWEEN 1000 AND 120000),
  CONSTRAINT targets_degraded_lt_timeout   CHECK (degraded_threshold_ms < timeout_ms)
);

COMMENT ON TABLE  targets IS '監視対象の設定（唯一の情報源）。状態は target_states に持つ';
COMMENT ON COLUMN targets.timeout_ms IS '既定90秒。コールドスタートを正常応答として許容するため（要件 F-2）';
COMMENT ON COLUMN targets.degraded_threshold_ms IS 'これを超えた成功応答は degraded として記録のみ行う（要件 F-3）';

CREATE UNIQUE INDEX targets_name_key ON targets (lower(name));

CREATE TRIGGER targets_set_updated_at BEFORE UPDATE ON targets
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------- target_states
-- targets と 1:1 の「可変状態」。更新主体（機械）と更新頻度（毎実行）が
-- 設定とは異なるため分離する（基本設計 D-6）。
--
-- current_incident_id に外部キーを張らないのは、incidents が 0003 で作られる
-- 循環参照を避けるため。整合性はアプリ側で担保する（詳細設計 9.1）。
CREATE TABLE target_states (
  target_id            uuid PRIMARY KEY REFERENCES targets(id) ON DELETE CASCADE,
  status               target_status NOT NULL DEFAULT 'up',
  consecutive_failures integer NOT NULL DEFAULT 0,
  last_checked_at      timestamptz,
  last_notified_at     timestamptz,
  current_incident_id  uuid,
  updated_at           timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT target_states_failures_non_negative CHECK (consecutive_failures >= 0)
);

COMMENT ON COLUMN target_states.last_notified_at IS 'フラッピング抑制（30分）の基準時刻（要件 F-4）';
COMMENT ON COLUMN target_states.current_incident_id IS 'incidents への論理参照。FKは張らない（詳細設計 9.1）';

CREATE TRIGGER target_states_set_updated_at BEFORE UPDATE ON target_states
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- 1:1 をDB側で保証する。targets を1行入れれば、対応する状態行が必ず存在する。
-- アプリ側の入れ忘れで「有効な監視対象なのに状態行が無い」状態を作らせない。
CREATE FUNCTION create_target_state() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO target_states (target_id) VALUES (NEW.id)
  ON CONFLICT (target_id) DO NOTHING;
  RETURN NULL;
END;
$$;

CREATE TRIGGER targets_create_state AFTER INSERT ON targets
  FOR EACH ROW EXECUTE FUNCTION create_target_state();