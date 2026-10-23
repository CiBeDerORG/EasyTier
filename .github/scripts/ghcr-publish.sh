#!/usr/bin/env bash
set -euo pipefail

archive_dir=${1:?image archive directory is required}
report_dir=${2:?report directory is required}
image=${REGISTRY_IMAGE:?REGISTRY_IMAGE is required}
tag=${IMAGE_TAG:?IMAGE_TAG is required}
revision=${GITHUB_SHA:?GITHUB_SHA is required}
mkdir -p "$report_dir"
: > "$report_dir/platform-digests.tsv"

refs=()
for arch in amd64 arm64; do
    gzip -dc "$archive_dir/easytier-$arch.tar.gz" | docker load
    local_image="easytier-ghcr:$arch"
    docker image inspect "$local_image" > "$report_dir/image-$arch.json"
    python3 - "$report_dir/image-$arch.json" "$arch" "$revision" <<'PY'
import json
import sys

with open(sys.argv[1]) as stream:
    image = json.load(stream)[0]
assert image['Os'] == 'linux', image['Os']
assert image['Architecture'] == sys.argv[2], image['Architecture']
assert image['Config']['Labels']['org.opencontainers.image.revision'] == sys.argv[3]
assert image['Config']['Entrypoint'] == ['/sbin/tini', '--', 'easytier-core']
PY
    remote_image="$image:sha-$revision-$arch"
    docker tag "$local_image" "$remote_image"
    docker push "$remote_image" 2>&1 | tee "$report_dir/push-$arch.log"
    digest_ref=$(docker image inspect "$remote_image" --format '{{index .RepoDigests 0}}')
    case "$digest_ref" in
        "$image"@sha256:*) ;;
        *) printf 'Unexpected pushed digest: %s\n' "$digest_ref" >&2; exit 1 ;;
    esac
    refs+=("$digest_ref")
    printf '%s\t%s\n' "$arch" "$digest_ref" >> "$report_dir/platform-digests.tsv"
done

docker buildx imagetools create \
    --metadata-file "$report_dir/create-metadata.json" \
    --annotation "index:org.opencontainers.image.source=$GITHUB_SERVER_URL/$GITHUB_REPOSITORY" \
    --annotation "index:org.opencontainers.image.revision=$revision" \
    --annotation 'index:org.opencontainers.image.description=EasyTier core, CLI, Web and embedded dashboard; linux/amd64 and linux/arm64' \
    --tag "$image:sha-$revision" \
    --tag "$image:$tag" \
    "${refs[@]}"

docker buildx imagetools inspect "$image:$tag" --raw > "$report_dir/manifest.json"
docker buildx imagetools inspect "$image:$tag" | tee "$report_dir/inspect-branch-tag.txt"
docker buildx imagetools inspect "$image:sha-$revision" | tee "$report_dir/inspect-sha-tag.txt"

python3 - "$report_dir" <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
manifest = json.loads((root / 'manifest.json').read_text())
expected = {'linux/amd64', 'linux/arm64'}
actual = {f"{item['platform']['os']}/{item['platform']['architecture']}" for item in manifest['manifests']}
assert actual == expected, actual
assert len(manifest['manifests']) == 2
tested = {}
for line in (root / 'platform-digests.tsv').read_text().splitlines():
    arch, reference = line.split('\t')
    tested[arch] = reference.split('@', 1)[1]
for item in manifest['manifests']:
    assert item['digest'] == tested[item['platform']['architecture']], item
metadata = json.loads((root / 'create-metadata.json').read_text())
digest = metadata['containerimage.descriptor']['digest']
for filename in ('inspect-branch-tag.txt', 'inspect-sha-tag.txt'):
    observed = re.search(r'^Digest:\s*(sha256:[0-9a-f]{64})\s*$', (root / filename).read_text(), re.MULTILINE)
    assert observed and observed[1] == digest, filename
(root / 'digest.txt').write_text(digest + '\n')
print(f'Verified multi-platform index: {digest}')
PY

digest=$(cat "$report_dir/digest.txt")
for arch in amd64 arm64; do
    docker pull --platform "linux/$arch" "$image@$digest" 2>&1 | tee "$report_dir/pull-$arch.log"
done

# A new GHCR package may be private. Record anonymous access separately from
# authenticated publication; never report private images as anonymously usable.
anonymous_config=$(mktemp -d "$RUNNER_TEMP/ghcr-anonymous.XXXXXX")
anonymous_pull=true
for arch in amd64 arm64; do
    if ! DOCKER_CONFIG="$anonymous_config" docker pull --platform "linux/$arch" "$image@$digest" \
        > "$report_dir/anonymous-pull-$arch.log" 2>&1; then
        anonymous_pull=false
        break
    fi
done
printf '%s\n' "$anonymous_pull" > "$report_dir/anonymous-pull.txt"

python3 - "$report_dir" "$image" "$tag" "$revision" "$anonymous_pull" <<'PY'
import json
import os
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
image, tag, revision = sys.argv[2:5]
anonymous = sys.argv[5] == 'true'
digest = (root / 'digest.txt').read_text().strip()
record = {
    'image': image,
    'branch_tag': tag,
    'commit_tag': f'sha-{revision}',
    'revision': revision,
    'digest': digest,
    'platforms': ['linux/amd64', 'linux/arm64'],
    'anonymous_pull_verified': anonymous,
    'run_url': f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
}
(root / 'publication.json').write_text(json.dumps(record, indent=2) + '\n')
summary = f'''# EasyTier GHCR publication

- Image: `{image}:{tag}`
- Commit tag: `{image}:sha-{revision}`
- Digest: `{digest}`
- Platforms: linux/amd64, linux/arm64
- Both native images passed binary and container startup smoke tests before upload.
- Both architectures were pulled back from GHCR by the verified index digest.
- Anonymous pull of both platforms verified: **{str(anonymous).lower()}**
- Run: {record['run_url']}

The deploy integration check uses equivalent CLI parameters and temporary data;
it does not copy or execute a private deployment repository's scripts.
'''
if not anonymous:
    summary += '\nAnonymous access is not confirmed. Inspect the package visibility and anonymous-pull-*.log before treating this as a login-free replacement.\n'
(root / 'summary.md').write_text(summary)
with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as stream:
    stream.write(summary)
print(json.dumps(record, indent=2))
PY
