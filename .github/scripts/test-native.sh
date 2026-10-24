#!/usr/bin/env bash
set -Eeuo pipefail

: "$TARGET" "$FEATURES" "$REPORT_DIR"
mkdir -p "$REPORT_DIR"

# Tests run in a separate job, on the same native target and static-Web feature
# set as the image. These command-local settings only speed up the test binary;
# production programs keep the repository's release profile unchanged.
started=$SECONDS
cargo test --locked --release --target "$TARGET" \
    --config 'profile.release.lto=false' \
    --config 'profile.release.codegen-units=16' \
    --config 'profile.release.opt-level=1' \
    --package easytier-web --bin easytier-web \
    --features "easytier-web/web-static,easytier/$FEATURES" \
    publication -- --nocapture --test-threads=2 \
    2>&1 | tee "$REPORT_DIR/publication-tests.log"

elapsed=$((SECONDS - started))
printf 'Publication regression build and execution: %s seconds\n' "$elapsed" \
    | tee "$REPORT_DIR/duration.txt"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    printf '### Publication regressions\n\n- Target: `%s`\n- Build and execution: %s seconds\n- Test-only optimization: opt-level=1, LTO off, codegen-units=16\n' \
        "$TARGET" "$elapsed" >> "$GITHUB_STEP_SUMMARY"
fi
