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
printf 'Image: %s\nScope: standalone entrypoint and deploy CLI-interface compatibility\n' "$image" > "$summary"

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
# No host ports or non-loopback container networking are used in either mode.
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

# Confirm that every official-style executable can load in the runtime image.
for binary in easytier-core easytier-cli easytier-web easytier-web-embed; do
    version_container="${name_prefix}-${binary}-version"
    containers+=("$version_container")
    timeout 30s docker run --rm --pull never --network none \
        --name "$version_container" --entrypoint "/usr/local/bin/$binary" \
        "$image" --version > "$report_dir/${binary}-version.log" 2>&1
    pass "$binary loads and reports its version"
done

mkdir -p "$smoke_dir/standalone-config" "$smoke_dir/config" "$smoke_dir/cloud-managed-core" "$smoke_dir/data"
python3 -c 'import uuid; print(uuid.uuid4().hex)' > "$smoke_dir/machine-id"

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
