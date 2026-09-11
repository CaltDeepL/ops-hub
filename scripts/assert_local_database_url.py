#!/usr/bin/env python3
"""AI/CI verificationで本番DBへ誤接続しないための最小ガード。"""

from __future__ import annotations

import os
import sys
from urllib.parse import urlparse

ALLOWED_HOSTS = {"localhost", "127.0.0.1", "::1", "db", "postgres"}

raw = os.environ.get("DATABASE_URL", "").strip()
if not raw:
    print(
        "verify: DATABASE_URL が未設定です。ローカルDocker/CI PostgreSQLを明示してください。",
        file=sys.stderr,
    )
    sys.exit(1)

parsed = urlparse(raw)
if parsed.scheme not in {"postgres", "postgresql"}:
    print("verify: PostgreSQL DATABASE_URL ではありません。", file=sys.stderr)
    sys.exit(1)

host = parsed.hostname
if host not in ALLOWED_HOSTS:
    print(
        f"verify: 非ローカルDBへの接続を拒否しました (host={host!r})。",
        file=sys.stderr,
    )
    print(
        "verify: 許可host: localhost, 127.0.0.1, ::1, db, postgres",
        file=sys.stderr,
    )
    sys.exit(1)

# password等は出力しない。
print(f"verify: local/CI PostgreSQL host accepted: {host}")
