# ---------- builder ----------
# ランタイムの distroless/cc-debian12 と同じ Debian 12 系で作る。
# glibc の版がずれると、実行時に「起動した瞬間に落ちる」形で表面化する。
FROM rust:1.96-slim-bookworm AS builder

WORKDIR /build

# sqlx の query! マクロ（タスク4以降）をビルド時にDBへ繋がず解決する。
# .sqlx/ が無いうちは無害。
ENV SQLX_OFFLINE=true

# 依存だけを先に解決してレイヤに固める。src/ の変更で依存の再ビルドが起きないように。
# Cargo.lock は必ずコミットしておく（バイナリなので再現性が要る）。
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && echo "fn main() {}" > src/main.rs \
    && touch src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY . .
# ダミーで作った成果物のタイムスタンプを無効化しないと、本物の src が
# 再ビルドされずダミーのバイナリが出てくる。
RUN touch src/main.rs src/lib.rs \
    && cargo build --release

# ---------- runtime ----------
# distroless: シェルも curl も無い。HEALTHCHECK は自前バイナリの
# healthcheck サブコマンドで行う（exec 形式必須）。
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

WORKDIR /app
COPY --from=builder /build/target/release/ops-hub /app/ops-hub

ENV PORT=8080
EXPOSE 8080
USER nonroot

HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/app/ops-hub", "healthcheck"]

ENTRYPOINT ["/app/ops-hub"]
CMD ["serve"]