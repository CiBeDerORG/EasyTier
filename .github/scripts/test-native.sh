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
# libtest accepts multiple filters after '--' and runs their union. A second
# Cargo invocation can rerun build scripts and compile the workspace again,
# even with identical flags; build and execute all selected suites in one invocation.
cargo test "${test_args[@]}" -- publication discovery::luci_client::tests asset_agent server_agent_entry automation::tests::t49_ \
    --nocapture --test-threads=2 --color never --format pretty \
    2>&1 | tee "$REPORT_DIR/native-tests.log"

for test_suite in publication discovery::luci_client::tests:: asset_agent server_agent_entry automation::tests::t49_; do
    if ! awk -v suite="$test_suite" '
        NF >= 4 && $1 == "test" && $(NF - 1) == "..." && $NF == "ok" && index($2, suite) { passed++ }
        END { exit passed == 0 }
    ' "$REPORT_DIR/native-tests.log"; then
        printf 'Required regression suite did not execute any passing tests: %s\n' "$test_suite" >&2
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
