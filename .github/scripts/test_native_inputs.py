"""CI regressions for native cache boundaries and conservative timestamp reuse."""

from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "build_native_inputs", Path(__file__).with_name("build-native-inputs.py")
)
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


@contextmanager
def working_directory(directory):
    previous = Path.cwd()
    os.chdir(directory)
    try:
        yield
    finally:
        os.chdir(previous)


class NativeInputTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        self.write("Cargo.toml", """[workspace]
members = ["easytier", "easytier-web", "easytier-core", "easytier-proto", "conditional", "unrelated"]
[workspace.dependencies]
easytier-core = { path = "easytier-core" }
conditional = { path = "conditional" }
[profile.release]
lto = true
codegen-units = 1
opt-level = 3
""")
        self.write("Cargo.lock", "version = 4\n")
        self.write("easytier/Cargo.toml", """[package]
name = "easytier"
version = "0.1.0"
[dependencies]
easytier-core.workspace = true
""")
        self.write("easytier-web/Cargo.toml", """[package]
name = "easytier-web"
version = "0.1.0"
[dependencies]
easytier = { path = "../easytier" }
[target.'cfg(windows)'.build-dependencies]
conditional = { workspace = true, optional = true }
""")
        self.write("easytier-core/Cargo.toml", """[package]
name = "easytier-core"
version = "0.1.0"
[dev-dependencies]
easytier-proto = { path = "../easytier-proto" }
""")
        for package in ("easytier-proto", "conditional", "unrelated"):
            self.write(f"{package}/Cargo.toml", f'[package]\nname = "{package}"\nversion = "0.1.0"\n')
        for package in ("easytier", "easytier-web", "easytier-core", "easytier-proto", "conditional", "unrelated"):
            self.write(f"{package}/src/lib.rs", "pub fn initial() {}\n")
        self.write("easytier-web/build.rs", "fn main() {}\n")
        self.write("easytier-web/migrations/schema.sql", "CREATE TABLE example (id INTEGER);\n")
        self.write("easytier-proto/proto/api.proto", 'syntax = "proto3";\n')
        self.write("easytier-web/frontend/src/App.vue", "<template>initial</template>\n")
        self.write("easytier-web/frontend-lib/src/index.ts", "export const initial = 1;\n")
        self.write("easytier-web/config-generator/src/index.ts", "export const initial = 1;\n")
        self.write(".cargo/config.toml", '[build]\nrustflags = ["-C", "target-feature=+crt-static"]\n')
        self.write(".github/workflows/node-attributes-image.yml", "name: Initial display name\n")
        self.write(".github/actions/prepare-build/action.yml", "name: prepare-build\n")
        self.write(".github/scripts/build-native.sh", "cargo build --release\n")
        self.write(".github/scripts/easytier-web-api.sh", '#!/bin/sh\nexec easytier-web-embed "$@"\n')
        self.track()

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def track(self):
        subprocess.run(["git", "add", "--all"], cwd=self.root, check=True)

    def keys(self):
        args = SimpleNamespace(
            arch="amd64", target="x86_64-unknown-linux-musl", allocator="jemalloc",
            rust_toolchain="1.95.0", output=self.root / ".outputs/native-inputs.json",
        )
        with working_directory(self.root), mock.patch.dict(os.environ, {"GITHUB_OUTPUT": ""}):
            NATIVE.make_key(args)
        return json.loads(args.output.read_text())

    def test_closure_contains_conditional_and_dev_local_packages(self):
        names = {item["path"] for item in NATIVE.source_inputs(self.root)}
        self.assertIn("conditional/src/lib.rs", names)
        self.assertIn("easytier-proto/proto/api.proto", names)
        self.assertIn("easytier-web/build.rs", names)
        self.assertIn("easytier-web/migrations/schema.sql", names)
        self.assertNotIn("unrelated/src/lib.rs", names)
        self.assertNotIn("unrelated/Cargo.toml", names)
        self.assertNotIn(".github/workflows/node-attributes-image.yml", names)

    def test_frontend_unrelated_code_and_workflow_display_do_not_invalidate(self):
        before = self.keys()
        for name in (
            "easytier-web/frontend/src/App.vue",
            "easytier-web/frontend-lib/src/index.ts",
            "easytier-web/config-generator/src/index.ts",
            "unrelated/src/lib.rs",
            ".github/workflows/node-attributes-image.yml",
        ):
            self.write(name, "changed\n")
        after = self.keys()
        self.assertEqual(before["key"], after["key"])
        self.assertEqual(before["dependencies_key"], after["dependencies_key"])

    def test_backend_resources_rotate_source_key_but_keep_dependencies(self):
        for name in ("easytier-web/migrations/schema.sql", "easytier-proto/proto/api.proto", "easytier-web/src/lib.rs"):
            with self.subTest(name=name):
                before = self.keys()
                self.write(name, (self.root / name).read_text() + "changed\n")
                after = self.keys()
                self.assertNotEqual(before["key"], after["key"])
                self.assertEqual(before["dependencies_key"], after["dependencies_key"])

    def test_manifests_and_actual_recipes_rotate_both_keys(self):
        for name in ("easytier-core/Cargo.toml", ".cargo/config.toml", ".github/scripts/easytier-web-api.sh", ".github/actions/prepare-build/action.yml"):
            with self.subTest(name=name):
                before = self.keys()
                self.write(name, (self.root / name).read_text() + "# changed recipe\n")
                after = self.keys()
                self.assertNotEqual(before["key"], after["key"])
                self.assertNotEqual(before["dependencies_key"], after["dependencies_key"])

    def test_release_override_is_an_effective_recipe_input(self):
        before = self.keys()
        with mock.patch.dict(os.environ, {"CARGO_PROFILE_RELEASE_LTO": "thin"}):
            after = self.keys()
        self.assertNotEqual(before["key"], after["key"])
        self.assertNotEqual(before["dependencies_key"], after["dependencies_key"])

    def test_unknown_inherited_or_missing_local_dependencies_fail_closed(self):
        original = (self.root / "easytier/Cargo.toml").read_text()
        self.write("easytier/Cargo.toml", original + "missing.workspace = true\n")
        with self.assertRaises(ValueError):
            NATIVE.source_inputs(self.root)
        self.write("easytier/Cargo.toml", original + 'missing = { path = "../missing" }\n')
        with self.assertRaises((ValueError, OSError)):
            NATIVE.source_inputs(self.root)

    def test_resource_symlink_tracks_target_bytes_outside_package(self):
        self.write("assets/fonts/example.ttf", "font bytes\n")
        (self.root / "easytier/assets").symlink_to("../assets", target_is_directory=True)
        self.track()
        before = self.keys()
        self.assertIn("assets/fonts/example.ttf", {item["path"] for item in before["files"]})
        self.write("assets/fonts/example.ttf", "new font bytes\n")
        after = self.keys()
        self.assertNotEqual(before["key"], after["key"])
        self.assertEqual(before["dependencies_key"], after["dependencies_key"])

    def test_path_dependency_through_tracked_directory_link_is_covered(self):
        self.write("shared/Cargo.toml", '[package]\nname = "shared"\nversion = "0.1.0"\n')
        self.write("shared/src/lib.rs", "pub fn shared() {}\n")
        (self.root / "alias").symlink_to("shared", target_is_directory=True)
        original = (self.root / "easytier/Cargo.toml").read_text()
        self.write("easytier/Cargo.toml", original + 'shared = { path = "../alias" }\n')
        self.track()
        names = {item["path"] for item in NATIVE.source_inputs(self.root)}
        self.assertIn("alias", names)
        self.assertIn("shared/src/lib.rs", names)

    def test_symlink_chain_and_directory_cycles_fail_closed(self):
        first, second = self.root / "easytier/first", self.root / "easytier/second"
        first.symlink_to("second")
        second.symlink_to("first")
        self.track()
        with self.assertRaises(ValueError):
            NATIVE.source_inputs(self.root)
        first.unlink()
        second.unlink()
        self.write("assets/example.txt", "resource\n")
        (self.root / "easytier/assets").symlink_to("../assets", target_is_directory=True)
        (self.root / "assets/back").symlink_to("../easytier", target_is_directory=True)
        self.track()
        with self.assertRaises(ValueError):
            NATIVE.source_inputs(self.root)

    def test_external_and_frontend_link_targets_fail_closed(self):
        link = self.root / "easytier/external"
        link.symlink_to("../../outside")
        self.track()
        with self.assertRaises(ValueError):
            NATIVE.source_inputs(self.root)
        link.unlink()
        link.symlink_to("../easytier-web/frontend", target_is_directory=True)
        with self.assertRaises(ValueError):
            NATIVE.source_inputs(self.root)

    def test_timestamps_only_restore_matching_current_tracked_bytes(self):
        unchanged = self.root / "easytier/src/lib.rs"
        changed = self.root / "easytier-web/src/lib.rs"
        untracked = self.write("outside.txt", "untracked\n")
        metadata = self.root / ".ci-cache/source-mtimes.json"
        old = time.time_ns() - 10_000_000_000
        recent = old + 5_000_000_000
        for path in (unchanged, changed, untracked):
            os.utime(path, ns=(old, old))
        args = SimpleNamespace(file=metadata)
        with working_directory(self.root):
            NATIVE.save_timestamps(args)
            saved = json.loads(metadata.read_text())
            saved["files"].extend([
                {"path": "outside.txt", "mode": "100644", "sha256": NATIVE.digest_file(untracked), "mtime_ns": old},
                {"path": "../outside.txt", "mode": "100644", "sha256": "0" * 64, "mtime_ns": old},
            ])
            metadata.write_text(json.dumps(saved))
            changed.write_text("pub fn modified() {}\n")
            for path in (unchanged, changed, untracked):
                os.utime(path, ns=(recent, recent))
            NATIVE.restore_timestamps(args)
        self.assertEqual(unchanged.stat().st_mtime_ns, old)
        self.assertEqual(changed.stat().st_mtime_ns, recent)
        self.assertEqual(untracked.stat().st_mtime_ns, recent)
        self.assertEqual(changed.read_text(), "pub fn modified() {}\n")

    def test_missing_corrupt_and_legacy_timestamp_metadata_are_ignored(self):
        args = SimpleNamespace(file=self.root / "timestamps.json")
        with working_directory(self.root):
            NATIVE.restore_timestamps(args)
            args.file.write_text("not JSON")
            NATIVE.restore_timestamps(args)
            args.file.write_text(json.dumps({"schema": 1, "files": []}))
            NATIVE.restore_timestamps(args)

    def test_redirected_symlink_disables_the_entire_timestamp_restore(self):
        first = self.write("assets/first.txt", "first resource\n")
        second = self.write("assets/second.txt", "different resource\n")
        (self.root / "easytier/assets").symlink_to("../assets", target_is_directory=True)
        redirected = self.root / "easytier/resource.txt"
        redirected.symlink_to("../assets/first.txt")
        self.track()
        unchanged = self.root / "easytier/src/lib.rs"
        old = time.time_ns() - 10_000_000_000
        recent = old + 5_000_000_000
        for path in (first, second, unchanged):
            os.utime(path, ns=(old, old))
        args = SimpleNamespace(file=self.root / ".ci-cache/source-mtimes.json")
        with working_directory(self.root):
            NATIVE.save_timestamps(args)
            redirected.unlink()
            redirected.symlink_to("../assets/second.txt")
            for path in (first, second, unchanged):
                os.utime(path, ns=(recent, recent))
            NATIVE.restore_timestamps(args)
        for path in (first, second, unchanged):
            self.assertEqual(path.stat().st_mtime_ns, recent)


if __name__ == "__main__":
    unittest.main()
