#!/bin/sh
set -eu

# Preserve the API-only default of the ordinary entrypoint. Resolve its peer
# relative to this script so the complete native bundle also works outside the
# image. exec preserves the server's exit status and direct signal delivery.
exec "$(dirname -- "$0")/easytier-web-embed" --no-web "$@"
