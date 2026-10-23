#!/usr/bin/env bash
set -Eeuo pipefail

: "$TARGET" "$FEATURES" "$REPORT_DIR"
mkdir -p "$REPORT_DIR"
rm -rf -- native-artifacts
mkdir -p native-artifacts/bin

# Both passes enable the same allocator in easytier. Only the Web resource
# serving feature changes; keep the ordinary API-only executable as well.
cargo build --locked --release --target "$TARGET" \
    --package easytier --package easytier-web \
    --features "easytier/$FEATURES" \
    --bin easytier-core --bin easytier-cli --bin easytier-web \
    2>&1 | tee "$REPORT_DIR/build-programs.log"
for program in easytier-core easytier-cli easytier-web; do
    install -m 755 "target/$TARGET/release/$program" "native-artifacts/bin/$program"
done

# Run publication ownership/transaction regressions on this same native target
# before recording a reusable bundle or publishing any image. pipefail keeps
# a failing test from being hidden by tee. Reuse release dependency artifacts.
cargo test --locked --release --target "$TARGET" \
    --package easytier-web --bin easytier-web \
    --features "easytier/$FEATURES" \
    publication -- --nocapture --test-threads=2 \
    2>&1 | tee "$REPORT_DIR/publication-tests.log"

cargo build --locked --release --target "$TARGET" \
    --package easytier-web --bin easytier-web \
    --features "web-static,easytier/$FEATURES" \
    2>&1 | tee "$REPORT_DIR/build-web.log"
# Keep the public image entrypoint consumed by deploy. In this image the Web
# files are supplied by EASYTIER_WEB_DIR, not compiled into the executable.
install -m 755 "target/$TARGET/release/easytier-web" \
    native-artifacts/bin/easytier-web-embed

python3 .github/scripts/build-native-inputs.py record \
    --inputs "$REPORT_DIR/native-inputs.json" --directory native-artifacts
