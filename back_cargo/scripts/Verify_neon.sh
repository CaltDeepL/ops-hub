#!/usr/bin/env bash
#
# Neon 接続先が ops-hub の前提を満たしているかを確認する。
#
#   export DATABASE_URL="postgresql://ops_hub:****@ep-....aws.neon.tech/ops_hub?sslmode=require"
#   ./scripts/verify-neon.sh
#
# 詳細設計11章の宿題1〜3をここで潰す。結果は docs/neon-setup.md 3章の表に転記する。
# 読み取りしか行わない（自分が張ったセッションの終了を除く）。

set -uo pipefail

: "${DATABASE_URL:?DATABASE_URL を設定してください}"

KEY="${RUN_LOCK_KEY:-8421337}"
FAILED=0

ok()   { printf '  \033[32mOK\033[0m   %s\n' "$1"; }
ng()   { printf '  \033[31mNG\033[0m   %s\n' "$1"; FAILED=1; }
warn() { printf '  \033[33mWARN\033[0m %s\n' "$1"; }
head_() { printf '\n\033[1m%s\033[0m\n' "$1"; }

q() { psql "$DATABASE_URL" -At -c "$1"; }

# --- 1. エンドポイントの種別 --------------------------------------------------
head_ "1. 接続先の確認"

HOST="$(printf '%s' "$DATABASE_URL" | sed -E 's|^[^:]+://||; s|^[^@]*@||; s|[:/?].*$||')"
echo "  host = $HOST"

if [[ "$HOST" == *-pooler.* ]]; then
  ng "プール済みエンドポイントです。セッションレベルの advisory lock が機能しません"
  echo
  echo "  Neon Console の Connection Details で Connection pooling を OFF にし、"
  echo "  -pooler の付かないホスト名を使ってください。ここから先は実行しません。"
  exit 1
fi
ok "直接エンドポイントです（宿題3）"

if ! q "select 1" > /dev/null 2>&1; then
  ng "接続できません。DATABASE_URL とパスワードを確認してください"
  exit 1
fi

echo "  version          = $(q 'select version()' | cut -d, -f1)"
echo "  database / user  = $(q "select current_database() || ' / ' || current_user")"
echo "  max_connections  = $(q 'show max_connections')"

# --- 2. advisory lock の競合 --------------------------------------------------
head_ "2. advisory lock（key=${KEY}）"

# 保持側を10秒だけ張る
psql "$DATABASE_URL" -At -c "select pg_try_advisory_lock($KEY); select pg_sleep(10);" \
  > /tmp/ops-hub-holder.out 2>&1 &
HOLDER=$!
sleep 3

HELD="$(q "select pg_try_advisory_lock($KEY)")"
if [[ "$HELD" == "f" ]]; then
  ok "保持中は別セッションから取得できない（競合が再現）"
else
  ng "保持中なのに取得できてしまった（値: ${HELD}）。プーラ経由でないか確認してください"
fi

# psql は1回の呼び出しごとに別セッションなので、取得したロックはそのプロセスの
# 終了時に自動で解放される。明示的な unlock は不要（別セッションから叩くと
# "you don't own a lock" の WARNING が出るだけ）。

# --- 3. pg_locks からの可視性 -------------------------------------------------
head_ "3. pg_locks でのキーの見え方（タスク7の脱出ハッチ用）"

LOCK_ROW="$(q "select classid || ' / ' || objid || ' / ' || objsubid || ' / ' || granted
               from pg_locks
               where locktype='advisory'
                 and objsubid = 1
                 and ((classid::bigint << 32) | objid::bigint) = $KEY
                 and granted")"
if [[ -n "$LOCK_ROW" ]]; then
  ok "復元述語でロックを特定できる（classid/objid/objsubid/granted = ${LOCK_ROW}）"
else
  ng "pg_locks からロックを特定できません。脱出ハッチが作れません"
fi

# --- 4. 宿題2: pg_terminate_backend ------------------------------------------
head_ "4. pg_terminate_backend の可否（宿題2 / タスク7）"

HOLDER_PID="$(q "select pid from pg_locks
                 where locktype='advisory' and objsubid = 1
                   and ((classid::bigint << 32) | objid::bigint) = $KEY
                   and granted and pid <> pg_backend_pid()
                 limit 1")"

if [[ -z "$HOLDER_PID" ]]; then
  warn "保持セッションの pid が取れませんでした（3 が NG のはず）。この項目はスキップします"
else
  TERM_RESULT="$(psql "$DATABASE_URL" -At -c "select pg_terminate_backend($HOLDER_PID)" 2>&1)"
  case "$TERM_RESULT" in
    t) ok "同一ロールの別セッションを強制終了できる → 脱出ハッチを実装できる" ;;
    f) warn "pid $HOLDER_PID は既に終了していました。時間を置いて再実行してください" ;;
    *) ng "強制終了できません: $TERM_RESULT"
       echo "     → 脱出ハッチは諦め、already_running へフォールバックする"
       echo "       （詳細設計 1.3 の但し書きのとおり）" ;;
  esac

  # 切断後にロックが解放されているか
  sleep 1
  REACQUIRED="$(q "select pg_try_advisory_lock($KEY)")"
  if [[ "$REACQUIRED" == "t" ]]; then
    ok "セッション終了でロックが自動解放される（RunLock の Drop が依存する前提）"
  else
    warn "まだ解放されていません。切断の検知に時間がかかっている可能性があります"
  fi
fi

wait "$HOLDER" 2>/dev/null || true

# --- 5. 宿題1: tcp_keepalives ------------------------------------------------
head_ "5. tcp_keepalives_idle（宿題1）"

IDLE="$(q 'show tcp_keepalives_idle')"
echo "  既定値 = $IDLE"
if [[ "$IDLE" == "0" ]]; then
  warn "0 が返りました。Unixソケット接続だと常に 0 になりますが、Neon は TCP のはずです"
else
  ok "TCP 接続として実値が見えています"
fi

SET_RESULT="$(psql "$DATABASE_URL" -At -c "set tcp_keepalives_idle = 30; show tcp_keepalives_idle;" 2>&1 | tail -1)"
if [[ "$SET_RESULT" == "30" ]]; then
  ok "SET で変更できる → 必要なら RunLock 取得前に1回投げる形にできる"
else
  warn "SET が効きません: $SET_RESULT"
  SEP='?'; [[ "$DATABASE_URL" == *'?'* ]] && SEP='&'
  OPT_RESULT="$(psql "${DATABASE_URL}${SEP}options=-c%20tcp_keepalives_idle%3D30" \
                 -At -c "show tcp_keepalives_idle" 2>&1 | tail -1)"
  if [[ "$OPT_RESULT" == "30" ]]; then
    ok "接続文字列の options なら渡せる"
  else
    warn "options でも渡せません: $OPT_RESULT"
    echo "     → 60分間隔の実行モデルではロックはプロセス終了で解放されるため、"
    echo "       宿題1は「対応不要」で閉じてよい。判断はタスク7で行う"
  fi
fi

# --- まとめ -------------------------------------------------------------------
head_ "結果"
if [[ "$FAILED" -eq 0 ]]; then
  echo "  致命的な問題はありません。sqlx migrate run へ進めます。"
else
  echo "  NG があります。docs/neon-setup.md 3章の表を見て対処してください。"
fi
exit "$FAILED"