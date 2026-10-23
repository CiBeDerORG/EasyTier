#!/usr/bin/env bash
set -euo pipefail

archive_dir=${1:?image archive directory is required}
report_dir=${2:?report directory is required}
image=${REGISTRY_IMAGE:?REGISTRY_IMAGE is required}
tag=${IMAGE_TAG:?IMAGE_TAG is required}
revision=${IMAGE_REVISION:-${GITHUB_SHA:?GITHUB_SHA is required}}
mkdir -p "$report_dir"

if [ "${VERIFY_ONLY:-false}" != true ]; then
: > "$report_dir/platform-digests.tsv"
refs=()
for arch in "${SELECTED_ARCH:?SELECTED_ARCH is required}"; do
    gzip -dc "$archive_dir/easytier-$arch.tar.gz" | docker load
    local_image="easytier-ghcr:$arch"
    docker image inspect "$local_image" > "$report_dir/image-$arch.json"
    python3 - "$report_dir/image-$arch.json" "$arch" "$revision" <<'PY'
import json
import re
import sys

with open(sys.argv[1]) as stream:
    image = json.load(stream)[0]
assert image['Os'] == 'linux', image['Os']
assert image['Architecture'] == sys.argv[2], image['Architecture']
assert image['Config']['Labels']['org.opencontainers.image.revision'] == sys.argv[3]
assert image['Config']['Entrypoint'] == ['/sbin/tini', '--', 'easytier-core']
labels = image['Config']['Labels']
if labels.get('io.easytier.web.mode') == 'directory':
    assert re.fullmatch(r'[0-9a-f]{40}', labels.get('io.easytier.backend.revision', ''))
    assert re.fullmatch(r'[0-9a-f]{64}', labels.get('io.easytier.backend.inputs', ''))
    assert labels.get('io.easytier.frontend.revision') == sys.argv[3]
    assert 'EASYTIER_WEB_DIR=/usr/share/easytier/web' in image['Config']['Env']
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
    --annotation "index:org.opencontainers.image.description=EasyTier selected architecture linux/$SELECTED_ARCH" \
    --tag "$image:sha-$revision" \
    --tag "$image:$tag" \
    "${refs[@]}"
fi

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
expected = {'linux/' + __import__('os').environ['SELECTED_ARCH']}
actual = {f"{item['platform']['os']}/{item['platform']['architecture']}" for item in manifest['manifests']}
assert actual == expected, actual
assert len(manifest['manifests']) == 1
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
# A classic Docker image store cannot map one index digest to two platform
# images. Pull the distinct child digests already checked against that index.
while IFS=$'\t' read -r arch reference; do
    docker pull --platform "linux/$arch" "$reference" 2>&1 | tee "$report_dir/pull-$arch.log"
    docker image inspect "$reference" > "$report_dir/pulled-image-$arch.json"
    python3 - "$report_dir/pulled-image-$arch.json" "$arch" "$revision" <<'PY'
import json
import sys

with open(sys.argv[1]) as stream:
    image = json.load(stream)[0]
assert image['Os'] == 'linux'
assert image['Architecture'] == sys.argv[2]
assert image['Config']['Labels']['org.opencontainers.image.revision'] == sys.argv[3]
assert image['Config']['Entrypoint'] == ['/sbin/tini', '--', 'easytier-core']
PY
done < "$report_dir/platform-digests.tsv"

# A new GHCR package may be private. Record anonymous access separately from
# authenticated publication; never report private images as anonymously usable.
anonymous_config=$(mktemp -d "$RUNNER_TEMP/ghcr-anonymous.XXXXXX")
trap 'rm -rf -- "$anonymous_config"' EXIT
anonymous_pull=true
if DOCKER_CONFIG="$anonymous_config" docker manifest inspect "$image@$digest" \
    > "$report_dir/anonymous-manifest.json" 2> "$report_dir/anonymous-manifest-error.log"; then
    python3 - "$report_dir" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
anonymous = json.loads((root / 'anonymous-manifest.json').read_text())
verified = json.loads((root / 'manifest.json').read_text())
# docker manifest inspect may reconstruct the list without index annotations.
# The immutable index request and its platform-to-child mapping are what matter.
def children(manifest):
    assert manifest['schemaVersion'] == 2
    assert len(manifest['manifests']) == 1
    return {
        (item['platform']['os'], item['platform']['architecture']): item['digest']
        for item in manifest['manifests']
    }
assert children(anonymous) == children(verified)
PY
else
    anonymous_pull=false
    cat "$report_dir/anonymous-manifest-error.log"
fi
while IFS=$'\t' read -r arch reference; do
    if ! DOCKER_CONFIG="$anonymous_config" docker pull --platform "linux/$arch" "$reference" \
        2>&1 | tee "$report_dir/anonymous-pull-$arch.log"; then
        anonymous_pull=false
    fi
done < "$report_dir/platform-digests.tsv"
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
selected_arch = os.environ['SELECTED_ARCH']
image_record = json.loads((root / f'image-{selected_arch}.json').read_text())[0]
labels = image_record['Config'].get('Labels') or {}
record = {
    'image': image,
    'branch_tag': tag,
    'commit_tag': f'sha-{revision}',
    'revision': revision,
    # The image/frontend revision may advance while an unchanged native bundle
    # retains the commit and toolchain it was originally built from.
    'backend_source_sha': labels.get('io.easytier.backend.revision'),
    'backend_input_hash': labels.get('io.easytier.backend.inputs'),
    'frontend_revision': labels.get('io.easytier.frontend.revision'),
    'web_asset_mode': labels.get('io.easytier.web.mode', 'embedded'),
    'digest': digest,
    'platforms': [f"linux/{os.environ['SELECTED_ARCH']}"],
    'anonymous_pull_verified': anonymous,
    'build_run_id': os.environ.get('SOURCE_RUN_ID', os.environ['GITHUB_RUN_ID']),
    'verification_revision': os.environ['GITHUB_SHA'],
    'run_url': f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
}
(root / 'publication.json').write_text(json.dumps(record, indent=2) + '\n')
summary = f'''# EasyTier GHCR publication

- Image: `{image}:{tag}`
- Commit tag: `{image}:sha-{revision}`
- Digest: `{digest}`
- Platforms: linux/{os.environ['SELECTED_ARCH']}
- The selected architecture passed binary and container startup smoke tests before upload.
- The selected architecture was pulled from GHCR by its child digest and checked against the verified index digest.
- Pulled image platform, source revision and official entrypoint were checked.
- Anonymous pull of selected platform verified: **{str(anonymous).lower()}**
- Run: {record['run_url']}

The deploy integration check uses equivalent CLI parameters and temporary data;
it does not copy or execute a private deployment repository's scripts.
'''
if not anonymous:
    summary += '\nAnonymous access is not confirmed. Inspect the package visibility and anonymous-pull-*.log before treating this as a login-free replacement.\n'
if record['backend_source_sha']:
    summary += (
        '\n## Native program provenance\n\n'
        f"- Image/frontend revision: {record['frontend_revision']}\n"
        f"- Original native source revision: {record['backend_source_sha']}\n"
        f"- Native input hash: {record['backend_input_hash']}\n"
        f"- Web assets: {record['web_asset_mode']}\n"
        '- The original native build manifest is included at /usr/share/easytier/build-manifest.json.\n'
    )
(root / 'summary.md').write_text(summary)
with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as stream:
    stream.write(summary)
print(json.dumps(record, indent=2))
PY
