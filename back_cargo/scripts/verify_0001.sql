CREATE OR REPLACE FUNCTION expect_error(stmt text, label text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  BEGIN
    EXECUTE stmt;
    RAISE NOTICE 'NG   % : 弾かれなかった', label;
  EXCEPTION WHEN others THEN
    RAISE NOTICE 'OK   % : % [%]', label, left(SQLERRM, 60), SQLSTATE;
  END;
END; $$;

-- 正常系
INSERT INTO targets (name, url, severity) VALUES ('chess-api', 'https://chess.example.com/health', 'sev2');

DO $$
DECLARE s record; BEGIN
  SELECT * INTO s FROM target_states;
  IF FOUND AND s.status = 'up' AND s.consecutive_failures = 0 THEN
    RAISE NOTICE 'OK   トリガで target_states が自動生成（status=%, failures=%）', s.status, s.consecutive_failures;
  ELSE
    RAISE NOTICE 'NG   target_states が生成されていない';
  END IF;
END $$;

-- 制約
SELECT expect_error($$INSERT INTO targets (name,url,severity) VALUES ('CHESS-API','https://a.example.com/health','sev2')$$, '大文字違いの同名を拒否');
SELECT expect_error($$INSERT INTO targets (name,url,severity) VALUES ('http-target','http://a.example.com/health','sev2')$$, 'https以外のURLを拒否  ');
SELECT expect_error($$INSERT INTO targets (name,url,method,severity) VALUES ('post-target','https://a.example.com','POST','sev2')$$, 'GET/HEAD以外を拒否   ');
SELECT expect_error($$INSERT INTO targets (name,url,timeout_ms,severity) VALUES ('short','https://a.example.com',500,'sev2')$$, 'timeout下限を拒否    ');
SELECT expect_error($$INSERT INTO targets (name,url,timeout_ms,severity) VALUES ('long','https://a.example.com',180000,'sev2')$$, 'timeout上限を拒否    ');
SELECT expect_error($$INSERT INTO targets (name,url,timeout_ms,degraded_threshold_ms,severity) VALUES ('bad','https://a.example.com',30000,30000,'sev2')$$, 'degraded>=timeoutを拒否');
SELECT expect_error($$INSERT INTO targets (name,url,severity) VALUES ('   ','https://a.example.com','sev2')$$, '空白のみの名前を拒否 ');
SELECT expect_error($$INSERT INTO targets (name,url,severity) VALUES ('x','https://a.example.com','sev4')$$, '未定義のseverityを拒否');
SELECT expect_error($$UPDATE target_states SET consecutive_failures = -1$$, '負の連続失敗数を拒否 ');

-- updated_at の独立性（D-6の根拠そのもの）
SELECT pg_sleep(0.05);
UPDATE target_states SET consecutive_failures = 1, last_checked_at = now();
DO $$
DECLARE t_upd timestamptz; t_created timestamptz; s_upd timestamptz; BEGIN
  SELECT updated_at, created_at INTO t_upd, t_created FROM targets;
  SELECT updated_at INTO s_upd FROM target_states;
  IF t_upd = t_created AND s_upd > t_upd THEN
    RAISE NOTICE 'OK   状態更新で targets.updated_at は動かず、target_states.updated_at のみ更新';
  ELSE
    RAISE NOTICE 'NG   updated_at が分離できていない (targets=%, states=%)', t_upd, s_upd;
  END IF;
END $$;

SELECT pg_sleep(0.05);
UPDATE targets SET enabled = false;
DO $$
DECLARE t_upd timestamptz; t_created timestamptz; BEGIN
  SELECT updated_at, created_at INTO t_upd, t_created FROM targets;
  IF t_upd > t_created THEN
    RAISE NOTICE 'OK   設定変更で targets.updated_at が更新される';
  ELSE
    RAISE NOTICE 'NG   targets の updated_at トリガが効いていない';
  END IF;
END $$;

-- CASCADE
DELETE FROM targets;
DO $$
DECLARE n int; BEGIN
  SELECT count(*) INTO n FROM target_states;
  IF n = 0 THEN RAISE NOTICE 'OK   targets 削除で target_states も CASCADE 削除';
  ELSE RAISE NOTICE 'NG   target_states が残った (%件)', n; END IF;
END $$;

DROP FUNCTION expect_error(text, text);