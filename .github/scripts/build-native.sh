#!/usr/bin/env bash
set -Eeuo pipefail

: "$TARGET" "$FEATURES" "$REPORT_DIR"
mkdir -p "$REPORT_DIR"
rm -rf -- native-artifacts
mkdir -p native-artifacts/bin

# Compile the Web server once with optional directory-based UI support. The
# API-only entrypoint selects --no-web at runtime, so both public entrypoints
# share the same release program without a second compilation/LTO pass.
cargo build --locked --release --target "$TARGET" \
    --package easytier --package easytier-web \
    --features "easytier-web/web-static,easytier/$FEATURES" \
    --bin easytier-core --bin easytier-cli --bin easytier-web \
    2>&1 | tee "$REPORT_DIR/build-programs.log"
release_dir="${CARGO_TARGET_DIR:-target}/$TARGET/release"
for program in easytier-core easytier-cli; do
    install -m 755 "$release_dir/$program" "native-artifacts/bin/$program"
done

# Keep the public image entrypoint consumed by deploy. In this image the Web
# files are supplied by EASYTIER_WEB_DIR, not compiled into the executable.
install -m 755 "$release_dir/easytier-web" \
    native-artifacts/bin/easytier-web-embed
install -m 755 .github/scripts/easytier-web-api.sh native-artifacts/bin/easytier-web

# Publication regressions run in the workflow's independent verification job.
# Publishing still requires that job to pass, including when this bundle is
# restored from its exact cache rather than compiled again.

python3 .github/scripts/build-native-inputs.py record \
    --inputs "$REPORT_DIR/native-inputs.json" --directory native-artifacts

