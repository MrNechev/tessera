#!/usr/bin/env bash
# Contract test: Letopis (Go, image letopis:0.3.0) vs Tessera (Rust) on the same synthetic
# library. Run on the host (needs docker, distrobox "tessera-dev", python3, curl, openssl).
#
#   crates/tessera-core/tests/contract/run.sh [--keep]
#
# Both servers run in throwaway containers built from the letopis image (same ffmpeg/vips);
# the Rust binary is mounted into its container. Nothing touches real media or the deployed
# letopis container. Scratch data lives in <repo>/scratch/contract.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../../.." && pwd)
S=$REPO/scratch/contract
IMAGE=${LETOPIS_IMAGE:-letopis:0.3.0}
PW=contract-pw
GO=letopis-contract-go
RS=tessera-contract-rs
MIG=tessera-contract-mig
KEEP=${1:-}

cleanup() {
  if [ "$KEEP" != "--keep" ]; then
    docker rm -f "$GO" "$RS" "$MIG" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT
docker rm -f "$GO" "$RS" "$MIG" >/dev/null 2>&1 || true

echo "== build tessera-server"
distrobox enter tessera-dev -- bash -lc "cd '$REPO' && cargo build -q -p tessera-server"
BIN=$REPO/target/debug/tessera-server

echo "== web UI"
WEB=$S/../web-src/dist
if [ ! -f "$WEB/index.html" ]; then
  echo "build the Letopis web UI first: copy letopis/web to scratch/web-src, npm ci && npm run build" >&2
  exit 1
fi

if [ ! -d "$S/lib" ]; then
  echo "== generate library"
  "$HERE/gen_library.sh" "$S/lib" >/dev/null
fi
rm -rf "$S/data-go" "$S/data-rs" "$S/data-mig"
mkdir -p "$S/data-go" "$S/data-rs"

U="$(id -u):$(id -g)"
echo "== start Go ($GO) and Rust ($RS)"
docker run -d --name "$GO" --user "$U" -e TZ=UTC -e LETOPIS_PASSWORD=$PW -e LETOPIS_HTTPS_ADDR=off \
  -e LETOPIS_EXCLUDE='*.txt' -e LETOPIS_HWACCEL=none \
  -v "$S/lib:/libraries/main:ro" -v "$S/data-go:/data" -p 127.0.0.1:28470:8470 "$IMAGE" >/dev/null
# the Rust server reads the old LETOPIS_* names too (fallback) — use them here on purpose
docker run -d --name "$RS" --user "$U" -e TZ=UTC -e LETOPIS_PASSWORD=$PW -e LETOPIS_HTTPS_ADDR=off \
  -e LETOPIS_EXCLUDE='*.txt' -e TESSERA_HWACCEL=none -e TESSERA_WEB_DIR=/web \
  -v "$BIN:/usr/local/bin/tessera-server:ro" -v "$WEB:/web:ro" \
  -v "$S/lib:/libraries/main:ro" -v "$S/data-rs:/data" -p 127.0.0.1:28480:8470 \
  --entrypoint /usr/local/bin/tessera-server "$IMAGE" >/dev/null
for p in 28470 28480; do
  for _ in $(seq 60); do curl -fs "http://127.0.0.1:$p/healthz" >/dev/null && break; sleep 0.5; done
done
docker exec "$RS" /usr/local/bin/tessera-server health && echo "health subcommand: ok"
docker exec "$RS" /usr/local/bin/tessera-server version

echo "== compare APIs"
set +e
python3 -I "$HERE/compare.py" --go http://127.0.0.1:28470 --rs http://127.0.0.1:28480 --password $PW
RC=$?
set -e

echo "== web UI login flow (curl, Rust)"
J=$S/cookies.txt; rm -f "$J"
curl -fs http://127.0.0.1:28480/ | grep -o '<title>[^<]*</title>'
curl -fs -c "$J" -H 'Content-Type: application/json' -d "{\"password\":\"$PW\"}" http://127.0.0.1:28480/api/login
curl -fs -b "$J" http://127.0.0.1:28480/api/session; echo
grep -q tessera_session "$J" && echo "cookie tessera_session: ok"

echo "== migration: Rust server on a copy of the Go data dir (letopis.db, tls/, documents)"
docker stop "$GO" >/dev/null
cp -a "$S/data-go" "$S/data-mig"
GO_CA_SUM=$(sha256sum "$S/data-go/tls/ca.crt" 2>/dev/null | cut -d' ' -f1 || true)
if [ -z "$GO_CA_SUM" ]; then
  # the Go server only creates the CA when HTTPS is on: make one with the Go binary
  docker run --rm --user "$U" -v "$S/data-mig:/data" -e LETOPIS_PASSWORD=$PW -e LETOPIS_HTTP_ADDR=off \
    -e LETOPIS_TLS_HOSTS=localhost,127.0.0.1 --entrypoint sh "$IMAGE" -c 'timeout 4 letopis >/dev/null 2>&1; true'
  GO_CA_SUM=$(sha256sum "$S/data-mig/tls/ca.crt" | cut -d' ' -f1)
  rm -f "$S/data-mig/tls/server.crt" "$S/data-mig/tls/server.key"
fi
docker run -d --name "$MIG" --user "$U" -e TZ=UTC -e TESSERA_PASSWORD=$PW -e TESSERA_HWACCEL=none \
  -e TESSERA_TLS_HOSTS=localhost,127.0.0.1,tessera.test -e TESSERA_WEB_DIR=/web \
  -v "$BIN:/usr/local/bin/tessera-server:ro" -v "$WEB:/web:ro" \
  -v "$S/lib:/libraries/main:ro" -v "$S/data-mig:/data" -p 127.0.0.1:28490:8470 -p 127.0.0.1:28491:8471 \
  --entrypoint /usr/local/bin/tessera-server "$IMAGE" >/dev/null
for _ in $(seq 60); do curl -fs http://127.0.0.1:28490/healthz >/dev/null && break; sleep 0.5; done
ls "$S/data-mig" | tr '\n' ' '; echo
[ -f "$S/data-mig/tessera.db" ] && [ ! -f "$S/data-mig/letopis.db" ] && echo "letopis.db → tessera.db: ok"
[ "$(sha256sum "$S/data-mig/tls/ca.crt" | cut -d' ' -f1)" = "$GO_CA_SUM" ] && echo "existing CA kept: ok"
echo | openssl s_client -connect 127.0.0.1:28491 -servername localhost -alpn h2 2>/dev/null \
  | sed -n 's/^\(ALPN protocol\|Verify return code\).*/&/p'
openssl s_client -connect 127.0.0.1:28491 -servername localhost -CAfile "$S/data-mig/tls/ca.crt" -verify_return_error </dev/null >/dev/null 2>&1 \
  && echo "server cert signed by the Letopis CA verifies: ok"
curl -s --http2 --cacert "$S/data-mig/tls/ca.crt" -o /dev/null -w 'HTTPS %{http_version} %{http_code}\n' https://localhost:28491/healthz \
  --resolve localhost:28491:127.0.0.1
curl -s -D - -o /dev/null --cacert "$S/data-mig/tls/ca.crt" --resolve localhost:28491:127.0.0.1 https://localhost:28491/ca.crt | grep -i content-disposition
curl -fs -c "$J" -H 'Content-Type: application/json' -d "{\"password\":\"$PW\"}" http://127.0.0.1:28490/api/login >/dev/null
echo "documents after migration: $(curl -fs -b "$J" http://127.0.0.1:28490/api/documents | python3 -I -c 'import json,sys; print(len(json.load(sys.stdin)))')"
exit $RC
