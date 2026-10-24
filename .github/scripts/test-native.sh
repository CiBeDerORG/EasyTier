#!/usr/bin/env bash
set -Eeuo pipefail

: "$TARGET" "$FEATURES" "$REPORT_DIR"
mkdir -p "$REPORT_DIR"

# Tests run in a separate job, on the same native target and static-Web feature
# set as the image. These command-local settings only speed up the test binary;
# production programs keep the repository's release profile unchanged.
started=$SECONDS
test_args=(
    --locked --release --target "$TARGET"
    --config 'profile.release.lto=false'
    --config 'profile.release.codegen-units=16'
    --config 'profile.release.opt-level=1'
    --package easytier-web --bin easytier-web
    --features "easytier-web/web-static,easytier/$FEATURES"
)
cargo test "${test_args[@]}" publication -- --nocapture --test-threads=2 --color never \
    2>&1 | tee "$REPORT_DIR/publication-tests.log"
# Reuse the same test binary to exercise the real HTTP(S) protocol fixture.
# A restricted easyadb session must be checkable without session.access rights.
cargo test "${test_args[@]}" discovery::luci_client::tests -- --nocapture --test-threads=2 --color never \
    2>&1 | tee "$REPORT_DIR/luci-client-tests.log"

for test_log in "$REPORT_DIR/publication-tests.log" "$REPORT_DIR/luci-client-tests.log"; do
    if ! grep -Eq 'test result: ok\. [1-9][0-9]* passed; 0 failed;' "$test_log"; then
        printf 'Required regression suite did not execute any passing tests: %s\n' "$test_log" >&2
        exit 1
    fi
done

elapsed=$((SECONDS - started))
printf 'Publication and LuCI regression build and execution: %s seconds\n' "$elapsed" \
    | tee "$REPORT_DIR/duration.txt"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    printf '### Publication and LuCI regressions\n\n- Target: `%s`\n- Build and execution: %s seconds\n- Test-only optimization: opt-level=1, LTO off, codegen-units=16\n' \
        "$TARGET" "$elapsed" >> "$GITHUB_STEP_SUMMARY"
fi
