#!/usr/bin/env bash
set -Eeuo pipefail

if [ "$#" -ne 2 ]; then
    echo "Usage: $0 IMAGE REPORT_DIR" >&2
    exit 2
fi

image="$1"
mkdir -p -- "$2"
report_dir="$(cd -- "$2" && pwd)"
smoke_dir="$(mktemp -d -t easytier-ghcr-smoke.XXXXXXXX)"
name_prefix="et-smoke-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}-$$-${RANDOM}"
containers=()
summary="$report_dir/smoke-results.txt"
printf 'Image: %s\nScope: standalone core, API-only Web, and deploy Web UI entrypoints\n' "$image" > "$summary"

cleanup() {
    local result=$?
    trap - EXIT INT TERM
    set +e
    local name
    for name in "${containers[@]}"; do
        if timeout 5s docker container inspect "$name" >/dev/null 2>&1; then
            timeout 10s docker logs --timestamps "$name" > "$report_dir/${name}.log" 2>&1
            timeout 5s docker inspect --format '{{json .State}}' "$name" > "$report_dir/${name}-state.json" 2>&1
            timeout 10s docker rm --force "$name" >/dev/null 2>&1
        fi
    done
    rm -rf -- "$smoke_dir"
    printf 'Overall exit code: %s\n' "$result" >> "$summary"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

pass() {
    printf 'PASS %s\n' "$1" | tee -a "$summary"
}

# The timeout bounds the entire readiness loop, including Docker commands.
# No host ports or non-loopback container networking are used in any mode.
wait_ready() {
    local name="$1"
    local mode="$2"
    timeout 60s bash -eu -o pipefail -c '
        while :; do
            state=$(docker inspect --format "{{.State.Status}}" "$1")
            case "$state" in exited|dead|removing) exit 1 ;; esac
            health=$(docker inspect --format "{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}" "$1")
            [ "$health" != missing ] || { echo "Image has no healthcheck" >&2; exit 1; }
            if [ "$health" = healthy ]; then
                if [ "$2" = core ]; then
                    exit 0
                elif timeout 5s docker exec "$1" wget -q -O - http://127.0.0.1:7007/ > "$3"; then
                    [ -s "$3" ] && exit 0
                fi
            fi
            sleep 1
        done
    ' smoke-ready "$name" "$mode" "$report_dir/cloud-index.html"
}

fetch_http() {
    timeout 10s docker exec "$cloud_container" wget -q -O - "http://127.0.0.1:7007$1" > "$2"
    test -s "$2"
}

wait_api_ready() {
    timeout 60s bash -eu -o pipefail -c '
        while :; do
            state=$(docker inspect --format "{{.State.Status}}" "$1")
            case "$state" in exited|dead|removing) exit 1 ;; esac
            if timeout 5s docker exec "$1" wget -q -O - \
                http://127.0.0.1:7017/api/v1/auth/logout > "$2"; then
                [ -s "$2" ] && exit 0
            fi
            sleep 1
        done
    ' smoke-api-ready "$1" "$report_dir/api-only-response.json"
}

assert_api_ui_absent() {
    local path="$1"
    local report_name="$2"
    local result=0
    timeout 10s docker exec "$api_container" wget -S -O - \
        "http://127.0.0.1:7017$path" \
        > "$report_dir/$report_name-body.txt" \
        2> "$report_dir/$report_name-headers.txt" || result=$?
    # A connection failure or timeout is not evidence that the UI is disabled.
    test "$result" -ne 124
    python3 - "$report_dir/$report_name-headers.txt" <<'PY'
from pathlib import Path
import re
import sys

headers = Path(sys.argv[1]).read_text()
statuses = re.findall(r"^\s*HTTP/\S+\s+(\d{3})\b", headers, re.MULTILINE)
if not statuses or statuses[-1] != "404":
    raise RuntimeError(f"API-only entrypoint must return HTTP 404 for UI resources: {headers}")
PY
}

# Confirm all four public entrypoints load, including the API-only wrapper.
for binary in easytier-core easytier-cli easytier-web easytier-web-embed; do
    version_container="${name_prefix}-${binary}-version"
    containers+=("$version_container")
    timeout 30s docker run --rm --pull never --network none \
        --name "$version_container" --entrypoint "/usr/local/bin/$binary" \
        "$image" --version > "$report_dir/${binary}-version.log" 2>&1
    pass "$binary loads and reports its version"
done
cmp "$report_dir/easytier-web-version.log" "$report_dir/easytier-web-embed-version.log"
pass 'both Web entrypoints report the same compiled version'

invalid_container="${name_prefix}-api-invalid-argument"
containers+=("$invalid_container")
if timeout 30s docker run --rm --pull never --network none \
    --name "$invalid_container" --entrypoint /usr/local/bin/easytier-web \
    "$image" --api-server-port not-a-port \
    > "$report_dir/api-invalid-argument.log" 2>&1; then
    echo 'API-only entrypoint unexpectedly accepted an invalid port' >&2
    exit 1
else
    argument_status=$?
    test "$argument_status" -eq 2
fi
pass 'API-only entrypoint preserves argument validation exit status'

mkdir -p "$smoke_dir/standalone-config" "$smoke_dir/config" "$smoke_dir/cloud-managed-core" "$smoke_dir/data" "$smoke_dir/api-data"
python3 -c 'import uuid; print(uuid.uuid4().hex)' > "$smoke_dir/machine-id"

api_container="${name_prefix}-api"
containers+=("$api_container")
# The inherited image healthcheck probes Core, so disable it for this standalone
# Web process. A nonexistent UI directory and ET_NO_WEB=false prove that the
# legacy entrypoint still defaults to API-only; the spaced DB path checks argv.
timeout 30s docker run --detach --pull never --network none --no-healthcheck \
    --name "$api_container" \
    --mount "type=bind,src=$smoke_dir/api-data,dst=/data" \
    --env EASYTIER_WEB_DIR=/missing-smoke-ui --env ET_NO_WEB=false \
    --entrypoint /usr/local/bin/easytier-web \
    "$image" --api-server-addr 127.0.0.1 --api-server-port 7017 \
    --config-server-port 7018 --config-server-protocol udp \
    --db '/data/api only.db' > "$report_dir/api-container-id.txt"
wait_api_ready "$api_container"
timeout 10s docker exec "$api_container" test -s '/data/api only.db'
api_executable="$(timeout 10s docker exec "$api_container" readlink /proc/1/exe)"
test "$api_executable" = /usr/local/bin/easytier-web-embed
assert_api_ui_absent / api-only-home
assert_api_ui_absent /api_meta.js api-only-metadata
assert_api_ui_absent /config-generator/index.html api-only-config-generator
python3 - "$report_dir/api-only-response.json" <<'PY'
from pathlib import Path
import json
import sys

if json.loads(Path(sys.argv[1]).read_text()) != {}:
    raise RuntimeError("API-only entrypoint did not return the expected API response")
PY
pass 'API-only entrypoint starts without UI assets, preserves argv, and serves no Web pages'

# The wrapper must exec the server, not leave a shell between PID 1 and it.
# SIGINT is the server's existing graceful shutdown signal (ctrl_c handler).
timeout 10s docker kill --signal INT "$api_container" > "$report_dir/api-signal.log"
timeout 15s docker wait "$api_container" > "$report_dir/api-exit-status.txt"
test "$(cat "$report_dir/api-exit-status.txt")" = 0
pass 'API-only entrypoint forwards signals and preserves the server exit status'

core_container="${name_prefix}-core"
containers+=("$core_container")
# Do not override ENTRYPOINT: this validates tini -> easytier-core from the image.
# Daemon mode deliberately has zero network instances and needs no TUN device.
timeout 30s docker run --detach --pull never --network none \
    --name "$core_container" \
    --health-interval 2s --health-timeout 5s --health-start-period 1s --health-retries 3 \
    --mount "type=bind,src=$smoke_dir/standalone-config,dst=/config" \
    "$image" --daemon --config-dir /config --rpc-portal 15888 > "$report_dir/core-container-id.txt"
wait_ready "$core_container" core
timeout 10s docker exec "$core_container" easytier-cli \
    --rpc-portal 127.0.0.1:15888 --output json logger get > "$report_dir/core-rpc.json"
pass 'official default entrypoint, RPC 15888, and zero-instance healthcheck'

# This independently written fixture checks the public CLI interface consumed by
# deploy. It is not a copy or execution of the private deploy/cloud.sh script.
cat > "$smoke_dir/start-managed.sh" <<'SH'
#!/bin/sh
set -eu
easytier-web-embed \
    --api-server-addr 0.0.0.0 \
    --api-server-port "$EASYTIER_WEB_PORT" \
    --api-host "$EASYTIER_WEB_API_HOST" \
    --config-server-port "$EASYTIER_CONFIG_PORT" \
    --config-server-protocol udp \
    --db /data/et.db &
exec easytier-core \
    --config-server "udp://127.0.0.1:${EASYTIER_CONFIG_PORT}/admin" \
    --config-dir /cloud-managed-core \
    --machine-id "$(cat /etc/machine-id)" \
    --rpc-portal "$EASYTIER_RPC_PORT"
SH

cloud_container="${name_prefix}-cloud"
containers+=("$cloud_container")
timeout 30s docker run --detach --pull never --network none \
    --name "$cloud_container" \
    --health-interval 2s --health-timeout 5s --health-start-period 1s --health-retries 3 \
    --mount "type=bind,src=$smoke_dir/machine-id,dst=/etc/machine-id,readonly" \
    --mount "type=bind,src=$smoke_dir/config,dst=/config" \
    --mount "type=bind,src=$smoke_dir/cloud-managed-core,dst=/cloud-managed-core" \
    --mount "type=bind,src=$smoke_dir/data,dst=/data" \
    --mount "type=bind,src=$smoke_dir/start-managed.sh,dst=/opt/smoke/start-managed.sh,readonly" \
    --env EASYTIER_PORT=7005 --env EASYTIER_RPC_PORT=7006 \
    --env EASYTIER_WEB_PORT=7007 --env EASYTIER_CONFIG_PORT=7008 \
    --env EASYTIER_WEB_API_HOST=http://127.0.0.1:7007 \
    --entrypoint /sbin/tini "$image" -g -- sh /opt/smoke/start-managed.sh \
    > "$report_dir/cloud-container-id.txt"
wait_ready "$cloud_container" cloud
timeout 10s docker exec "$cloud_container" easytier-cli \
    --rpc-portal 127.0.0.1:7006 --output json logger get > "$report_dir/cloud-rpc.json"
timeout 10s docker exec "$cloud_container" test -s /data/et.db
pass 'deploy CLI interface: Web + managed Core, RPC 7006, and /data/et.db'

fetch_http /api_meta.js "$report_dir/api_meta.js"
fetch_http /config-generator/index.html "$report_dir/config-generator-index.html"

python3 - "$report_dir" <<'PY'
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urljoin, urlsplit
import json
import sys

reports = Path(sys.argv[1])
for filename in ("core-rpc.json", "cloud-rpc.json"):
    json.loads((reports / filename).read_text())
metadata = (reports / "api_meta.js").read_text().strip().removesuffix(";")
prefix = "window.apiMeta = "
if not metadata.startswith(prefix):
    raise RuntimeError("API metadata is not the expected generated JavaScript")
if json.loads(metadata[len(prefix):]) != {"api_host": "http://127.0.0.1:7007/"}:
    raise RuntimeError("Embedded UI API host does not match the configured deploy endpoint")

class Scripts(HTMLParser):
    def __init__(self):
        super().__init__()
        self.sources = []
        self.has_app = False
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        self.has_app |= attrs.get("id") == "app"
        if tag == "script" and attrs.get("src"):
            self.sources.append(attrs["src"])

assets = set()
pages = (
    ("cloud-index.html", "/", "EasyTier Dashboard"),
    ("config-generator-index.html", "/config-generator/index.html", "EasyTier Config Generator"),
)
for filename, base_path, expected_title in pages:
    body = (reports / filename).read_text()
    parser = Scripts()
    parser.feed(body)
    if expected_title not in body or not parser.has_app:
        raise RuntimeError(f"{base_path}: expected embedded application HTML")
    if not parser.sources:
        raise RuntimeError(f"{base_path}: no referenced JavaScript assets")
    for source in parser.sources:
        url = urlsplit(urljoin("http://127.0.0.1:7007" + base_path, source))
        if url.scheme != "http" or url.netloc != "127.0.0.1:7007":
            raise RuntimeError(f"{base_path}: unexpected external script reference")
        asset = url.path + (("?" + url.query) if url.query else "")
        if "\n" in asset or "\r" in asset:
            raise RuntimeError("Invalid script path")
        assets.add(asset)
(reports / "javascript-assets.txt").write_text("\n".join(sorted(assets)) + "\n")
PY

asset_number=0
while IFS= read -r asset; do
    asset_number=$((asset_number + 1))
    fetch_http "$asset" "$report_dir/asset-${asset_number}.js"
    python3 - "$report_dir/asset-${asset_number}.js" <<'PY'
from pathlib import Path
import re
import sys

body = Path(sys.argv[1]).read_text()
if re.match(r"\s*<(?:!doctype\s+html|html(?:\s|>))", body, re.IGNORECASE):
    raise RuntimeError("JavaScript asset returned an HTML fallback")
PY
done < "$report_dir/javascript-assets.txt"
pass "Web home, API metadata, config generator, and $asset_number referenced JavaScript assets"

# Both must still be alive after the HTTP checks; cleanup always saves final logs.
test "$(docker inspect --format '{{.State.Running}}' "$core_container")" = true
test "$(docker inspect --format '{{.State.Running}}' "$cloud_container")" = true
pass 'all image smoke checks completed without host networking, privileges, or real data'

