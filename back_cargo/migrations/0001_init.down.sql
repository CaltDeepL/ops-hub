-- =============================================================================
-- 0001_init (down)
-- 作成と逆順に落とす。テーブルを先に落とすとトリガも消えるため、
-- 明示的な DROP TRIGGER はテーブルより前に置く。
-- =============================================================================

DROP TRIGGER IF EXISTS targets_create_state          ON targets;
DROP TRIGGER IF EXISTS target_states_set_updated_at  ON target_states;
DROP TRIGGER IF EXISTS targets_set_updated_at        ON targets;

DROP TABLE IF EXISTS target_states;
DROP TABLE IF EXISTS targets;

DROP FUNCTION IF EXISTS create_target_state();
DROP FUNCTION IF EXISTS set_updated_at();

DROP TYPE IF EXISTS run_status;
DROP TYPE IF EXISTS outbox_source_kind;
DROP TYPE IF EXISTS outbox_status;
DROP TYPE IF EXISTS event_level;
DROP TYPE IF EXISTS check_result;
DROP TYPE IF EXISTS target_status;
DROP TYPE IF EXISTS severity;