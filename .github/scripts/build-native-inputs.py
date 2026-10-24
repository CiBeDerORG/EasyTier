#!/usr/bin/env python3
"""Fingerprint native sources and retain the provenance of reusable binaries."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import time
import tomllib


SCHEMA = 2
PROGRAMS = ("easytier-core", "easytier-cli", "easytier-web", "easytier-web-embed")
ROOT_PACKAGES = ("easytier", "easytier-web")
WEB_TREES = (
    "easytier-web/frontend/",
    "easytier-web/frontend-lib/",
    "easytier-web/config-generator/",
)
RECIPE_FILES = {
    ".gitattributes",
    ".github/scripts/build-native-inputs.py",
    ".github/scripts/build-native.sh",
    ".github/scripts/easytier-web-api.sh",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
}
DEPENDENCY_SECTIONS = ("dependencies", "build-dependencies", "dev-dependencies")
TARGETS = {
    "amd64": ("x86_64-unknown-linux-musl", "jemalloc"),
    "arm64": ("aarch64-unknown-linux-musl", "mimalloc"),
}


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def digest_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def output(name, value):
    if destination := os.environ.get("GITHUB_OUTPUT"):
        with open(destination, "a") as stream:
            stream.write(f"{name}={value}\n")


class InputTree:
    """Read tracked inputs only, including the targets of repository symlinks."""

    def __init__(self, root):
        self.root = root.resolve()
        self.tracked = {}
        raw = subprocess.check_output(
            ["git", "ls-files", "--stage", "-z"], cwd=self.root
        )
        for entry in raw.split(b"\0"):
            if not entry:
                continue
            metadata, name = entry.split(b"\t", 1)
            mode, _object, stage = metadata.decode().split()
            if stage != "0":
                raise ValueError("Native inputs contain an unresolved merge")
            self.tracked[os.fsdecode(name)] = mode

    def resolve(self, name):
        """Resolve tracked links without accepting external/untracked aliases."""
        path = PurePosixPath(name)
        if path.is_absolute():
            raise ValueError(f"Native input must be repository-relative: {name}")
        pending, parts, links = list(path.parts), [], []
        while pending:
            part = pending.pop(0)
            if part in ("", "."):
                continue
            if part == "..":
                if not parts:
                    raise ValueError(f"Native input escapes the repository: {name}")
                parts.pop()
                continue
            candidate = "/".join([*parts, part])
            if self.tracked.get(candidate) == "120000":
                if candidate in links:
                    raise ValueError(f"Cyclic native input link: {candidate}")
                links.append(candidate)
                target = PurePosixPath(os.readlink(self.root / candidate))
                if target.is_absolute():
                    raise ValueError(f"Absolute native input link: {candidate}")
                pending = [*target.parts, *pending]
            else:
                parts.append(part)
        resolved = PurePosixPath(*parts)
        physical = (self.root / resolved).resolve(strict=True)
        if physical != self.root / resolved:
            raise ValueError(f"Untracked symbolic link in native input: {name}")
        return resolved.as_posix(), links

    def under(self, directory):
        parent = PurePosixPath(directory)
        return {
            name for name in self.tracked
            if PurePosixPath(name).is_relative_to(parent)
        }

    def expand_links(self, names):
        selected, finished = set(names), set()

        def visit(name, active=()):
            if name in active:
                raise ValueError(f"Cyclic native resource directory: {name}")
            if name in finished:
                return
            target, links = self.resolve(name)
            linked = self.under(target)
            if not linked:
                raise ValueError(f"Native input link has no tracked target: {name}")
            if any(candidate.startswith(WEB_TREES) for candidate in linked):
                raise ValueError(f"Native input link reaches excluded frontend inputs: {name}")
            selected.update(links)
            selected.update(linked)
            for candidate in sorted((set(links) | linked) - {name}):
                if self.tracked[candidate] == "120000":
                    visit(candidate, (*active, name))
            # A directory link that contains itself would otherwise be hidden
            # by the selected-set de-duplication above.
            if name in linked:
                raise ValueError(f"Cyclic native resource directory: {name}")
            finished.add(name)

        for name in sorted(names):
            if self.tracked[name] == "120000":
                visit(name)
        return selected

    def manifest(self, name):
        resolved, links = self.resolve(name)
        if self.tracked.get(resolved) not in ("100644", "100755"):
            raise ValueError(f"Untracked or unsupported Cargo manifest: {name}")
        with (self.root / resolved).open("rb") as stream:
            return tomllib.load(stream), {name, resolved, *links}

    def hashes(self, names):
        inputs = []
        for name in sorted(names):
            path = self.root / name
            mode = self.tracked[name]
            if mode == "120000":
                if not path.is_symlink():
                    raise ValueError(f"Native input link changed type: {name}")
                digest = hashlib.sha256(os.fsencode(os.readlink(path))).hexdigest()
            elif mode in ("100644", "100755"):
                if path.is_symlink() or path.resolve(strict=True) != path:
                    raise ValueError(f"Native input changed to an untracked link: {name}")
                digest = digest_file(path)
            else:
                raise ValueError(f"Unsupported native input mode {mode}: {name}")
            inputs.append({"path": name, "mode": mode, "sha256": digest})
        return inputs


def dependency_tables(manifest):
    for section in DEPENDENCY_SECTIONS:
        yield manifest.get(section, {})
    # Include every target condition and optional dependency conservatively.
    # Evaluating cfg/features here would duplicate Cargo's resolver and risk
    # omitting a newly enabled local dependency.
    for target in manifest.get("target", {}).values():
        for section in DEPENDENCY_SECTIONS:
            yield target.get(section, {})


def input_sets(root=None):
    tree = InputTree(root or Path.cwd())
    workspace_manifest, manifests = tree.manifest("Cargo.toml")
    workspace_dependencies = workspace_manifest.get("workspace", {}).get("dependencies", {})
    pending = list(ROOT_PACKAGES)
    packages, links, extra_roots = set(), set(), set()

    # Local patches/replacements can add source that is not declared as a path
    # dependency in either selected package. Include them conservatively.
    overrides = list(workspace_manifest.get("patch", {}).values())
    overrides.append(workspace_manifest.get("replace", {}))
    for table in overrides:
        for spec in table.values():
            if isinstance(spec, dict) and "path" in spec:
                pending.append(spec["path"])

    configurations = {
        name for name in tree.tracked
        if name.startswith(".cargo/")
    }
    for name in sorted(configurations):
        if PurePosixPath(name).name not in ("config", "config.toml"):
            continue
        config, config_inputs = tree.manifest(name)
        configurations.update(config_inputs)
        # Cargo's path overrides are relative to the configuration's parent
        # directory (the repository for .cargo/config.toml).
        base = PurePosixPath(name).parent.parent
        for path in config.get("paths", []):
            pending.append(str(base / path))
        config_overrides = list(config.get("patch", {}).values())
        config_overrides.append(config.get("replace", {}))
        for table in config_overrides:
            for spec in table.values():
                if isinstance(spec, dict) and "path" in spec:
                    pending.append(str(base / spec["path"]))
        for source in config.get("source", {}).values():
            for field in ("directory", "local-registry"):
                if field in source:
                    location, aliases = tree.resolve(str(base / source[field]))
                    links.update(aliases)
                    if not tree.under(location):
                        raise ValueError(f"Local Cargo source has no tracked files: {location}")
                    extra_roots.add(location)

    while pending:
        requested = pending.pop()
        if not isinstance(requested, str) or not requested:
            raise ValueError("Local Cargo dependencies require a nonempty path")
        package, aliases = tree.resolve(requested)
        links.update(aliases)
        if package in packages:
            continue
        if any(package == frontend.rstrip("/") or package.startswith(frontend) for frontend in WEB_TREES):
            raise ValueError(f"Native Cargo dependency is inside excluded frontend inputs: {package}")
        manifest_name = str(PurePosixPath(package) / "Cargo.toml")
        document, manifest_inputs = tree.manifest(manifest_name)
        if not isinstance(document.get("package"), dict):
            raise ValueError(f"Local dependency is not a Cargo package: {manifest_name}")
        if "workspace" in document and package != ".":
            raise ValueError(f"Nested local workspaces need an explicit native input rule: {package}")
        owner = document["package"].get("workspace")
        if owner is not None:
            resolved_owner, owner_links = tree.resolve(str(PurePosixPath(package) / owner))
            links.update(owner_links)
            if resolved_owner != ".":
                raise ValueError(f"Unsupported local dependency workspace: {package}")
        packages.add(package)
        manifests.update(manifest_inputs)
        for table in dependency_tables(document):
            for name, spec in table.items():
                base = PurePosixPath(package)
                if not isinstance(spec, dict):
                    continue
                if spec.get("workspace") is True:
                    if name not in workspace_dependencies or "path" in spec:
                        raise ValueError(f"Unknown or conflicting inherited dependency: {package}/{name}")
                    spec = workspace_dependencies[name]
                    base = PurePosixPath(".")
                if isinstance(spec, dict) and "path" in spec:
                    path = spec["path"]
                    if not isinstance(path, str) or not path:
                        raise ValueError(f"Invalid local dependency path: {package}/{name}")
                    pending.append(str(base / path))

    dependencies = set(manifests) | configurations | links
    dependencies.update(name for name in tree.tracked if name in RECIPE_FILES)
    dependencies.update(
        name for name in tree.tracked
        if name.startswith(".github/actions/prepare-build/")
    )
    selected = set(dependencies)
    for package in packages:
        # Keep complete packages: generated-code inputs, SQL, locales, fonts,
        # build.rs/build/*.rs and future resource types are all significant.
        files = {name for name in tree.under(package) if not name.startswith(WEB_TREES)}
        selected.update(files)
        dependencies.update(
            name for name in files
            if PurePosixPath(name).name in ("Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml")
            or ".cargo" in PurePosixPath(name).parts
        )
    for directory in extra_roots:
        dependencies.update(tree.under(directory))
        selected.update(tree.under(directory))
    dependencies = tree.expand_links(dependencies)
    selected = tree.expand_links(selected | dependencies)
    return tree.hashes(selected), tree.hashes(dependencies)


def source_inputs(root=None):
    return input_sets(root)[0]


def hash_content(content):
    encoded = json.dumps(content, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def compiler_environment():
    # The workflow itself also contains UI labels, packaging and publication
    # steps. Fingerprint effective native settings instead of the whole YAML.
    fixed = {
        "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTFLAGS",
        "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER", "CARGO_INCREMENTAL", "CARGO_BUILD_JOBS",
        "SOURCE_DATE_EPOCH", "CC", "CXX", "CFLAGS", "CXXFLAGS", "AR", "ARFLAGS",
        "BINDGEN_EXTRA_CLANG_ARGS", "PROTOC", "PROTOC_INCLUDE",
    }
    prefixes = ("CARGO_PROFILE_RELEASE_", "CARGO_TARGET_", "CC_", "CXX_", "CFLAGS_", "CXXFLAGS_", "AR_")
    return {
        name: value for name, value in sorted(os.environ.items())
        if value and (name in fixed or name.startswith(prefixes))
    }


def make_key(args):
    expected_target, expected_allocator = TARGETS[args.arch]
    if (args.target, args.allocator) != (expected_target, expected_allocator):
        raise ValueError("Architecture, target and allocator do not agree")
    recipe = {
        "schema": SCHEMA,
        "arch": args.arch,
        "target": args.target,
        "rust_toolchain": args.rust_toolchain,
        "profile": "release",
        "packages": list(ROOT_PACKAGES),
        "cargo_binaries": ["easytier-core", "easytier-cli", "easytier-web"],
        "program_features": ["easytier-web/web-static", f"easytier/{args.allocator}"],
        "entrypoints": {
            "easytier-web": "api-only wrapper",
            "easytier-web-embed": "web-static executable",
        },
        "runner_os": os.environ.get("RUNNER_OS", ""),
        "runner_arch": os.environ.get("RUNNER_ARCH", ""),
        "runner_image": os.environ.get("ImageOS", ""),
        "runner_image_version": os.environ.get("ImageVersion", ""),
        "compiler_environment": compiler_environment(),
    }
    files, dependency_files = input_sets()
    content = {"recipe": recipe, "files": files}
    key = hash_content(content)
    dependencies_key = hash_content({"recipe": recipe, "files": dependency_files})
    record = {"key": key, "dependencies_key": dependencies_key, **content}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + "\n")
    output("key", key)
    output("dependencies_key", dependencies_key)
    print(f"Native input key: {key} ({len(content['files'])} tracked inputs)")
    print(f"Native dependency compatibility key: {dependencies_key}")


def record_artifacts(args):
    inputs = json.loads(args.inputs.read_text())
    if inputs["recipe"]["schema"] != SCHEMA:
        raise ValueError("Native inputs schema mismatch")
    if source_inputs() != inputs["files"]:
        raise ValueError("Native inputs changed after the cache key was computed")
    revision = command("git", "rev-parse", "HEAD")
    if os.environ.get("GITHUB_SHA", revision) != revision:
        raise ValueError("Checkout revision does not match this workflow run")
    programs = {}
    for name in PROGRAMS:
        path = args.directory / "bin" / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"Missing native output: {name}")
        programs[name] = {"sha256": digest_file(path), "size": path.stat().st_size}
    manifest = {
        "schema": SCHEMA,
        "backend_input_hash": inputs["key"],
        "dependencies_hash": inputs["dependencies_key"],
        "backend_source_sha": revision,
        "recipe": inputs["recipe"],
        "programs": programs,
        "built_at": datetime.now(timezone.utc).isoformat(),
        "tool_versions": {
            "rustc": command("rustc", "-Vv"),
            "cargo": command("cargo", "-V"),
            "protoc": command("protoc", "--version"),
            "cc": command("cc", "--version"),
        },
        "build_run_url": (
            f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
            f"{os.environ.get('GITHUB_REPOSITORY', '')}/actions/runs/"
            f"{os.environ.get('GITHUB_RUN_ID', '')}"
        ),
    }
    (args.directory / "build-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n"
    )


def read_artifacts(args):
    manifest = json.loads((args.directory / "build-manifest.json").read_text())
    if manifest["schema"] != SCHEMA:
        raise ValueError("Native manifest schema mismatch")
    if manifest["backend_input_hash"] != args.key:
        raise ValueError("Native cache does not match the requested input key")
    if not re.fullmatch(r"[0-9a-f]{40}", manifest["backend_source_sha"]):
        raise ValueError("Invalid native source revision")
    recipe = manifest["recipe"]
    if recipe["arch"] != args.arch or recipe["target"] != args.target:
        raise ValueError("Native cache architecture does not match")
    if set(manifest["programs"]) != set(PROGRAMS):
        raise ValueError("Native cache does not contain exactly four programs")
    for name in PROGRAMS:
        path = args.directory / "bin" / name
        recorded = manifest["programs"][name]
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Missing native program: {name}")
        if path.stat().st_size == 0 or path.stat().st_size != recorded["size"]:
            raise ValueError(f"Native program size mismatch: {name}")
        if digest_file(path) != recorded["sha256"]:
            raise ValueError(f"Native program checksum mismatch: {name}")
    return manifest


def inspect_artifacts(args):
    try:
        manifest = read_artifacts(args)
    except (OSError, ValueError, KeyError, TypeError) as error:
        if args.command != "restore":
            raise
        output("reusable", "false")
        print(f"Native artifacts will be rebuilt: {error}")
        return
    output("reusable", "true")
    output("backend_revision", manifest["backend_source_sha"])
    print(f"Native programs built from {manifest['backend_source_sha']}")


def save_timestamps(args):
    files = source_inputs()
    for item in files:
        item["mtime_ns"] = Path(item["path"]).lstat().st_mtime_ns
    args.file.parent.mkdir(parents=True, exist_ok=True)
    args.file.write_text(json.dumps({"schema": SCHEMA, "files": files}, indent=2) + "\n")
    print(f"Recorded timestamps for {len(files)} native inputs")


def restore_timestamps(args):
    # Timestamp metadata is an optimization, never a reason to trust or reuse
    # different source bytes. Old/missing/broken metadata simply causes Cargo
    # to do its normal freshness checks against the new checkout.
    try:
        if args.file.is_symlink():
            raise ValueError("Timestamp metadata must be a regular file")
        saved = json.loads(args.file.read_text())
        if not isinstance(saved, dict) or saved.get("schema") != SCHEMA or not isinstance(saved.get("files"), list):
            raise ValueError("Timestamp metadata schema mismatch")
        current = {item["path"]: item for item in source_inputs()}
    except (OSError, ValueError, KeyError, TypeError, RuntimeError) as error:
        print(f"Native source timestamps were not restored: {error}")
        return
    previous_links = {
        item["path"]: item.get("sha256")
        for item in saved["files"]
        if isinstance(item, dict) and item.get("mode") == "120000"
        and isinstance(item.get("path"), str)
    }
    current_links = {
        name: item["sha256"] for name, item in current.items()
        if item["mode"] == "120000"
    }
    if previous_links != current_links:
        # A link may now resolve to a different, previously cached source whose
        # own bytes are unchanged. Restoring that target's old mtime could hide
        # the redirected dependency from Cargo's freshness checks. Keep all new
        # checkout timestamps whenever the selected link topology changes.
        print("Native source timestamps were not restored: tracked symbolic links changed")
        return
    restored, now = 0, time.time_ns()
    seen = set()
    for previous in saved["files"]:
        if not isinstance(previous, dict):
            continue
        name = previous.get("path")
        if not isinstance(name, str) or name not in current or name in seen:
            continue
        seen.add(name)
        item = current[name]
        stamp = previous.get("mtime_ns")
        if (
            previous.get("mode") != item["mode"]
            or previous.get("sha256") != item["sha256"]
            or type(stamp) is not int or not 0 < stamp <= now
        ):
            continue
        try:
            path = Path(name)
            stat = path.lstat()
            os.utime(path, ns=(stat.st_atime_ns, stamp), follow_symlinks=False)
            restored += 1
        except (OSError, NotImplementedError):
            continue
    print(f"Restored timestamps for {restored} unchanged tracked native inputs")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    key = commands.add_parser("key")
    key.add_argument("--arch", choices=TARGETS, required=True)
    key.add_argument("--target", required=True)
    key.add_argument("--allocator", required=True)
    key.add_argument("--rust-toolchain", required=True)
    key.add_argument("--output", type=Path, required=True)
    key.set_defaults(action=make_key)
    record = commands.add_parser("record")
    record.add_argument("--inputs", type=Path, required=True)
    record.add_argument("--directory", type=Path, required=True)
    record.set_defaults(action=record_artifacts)
    for name, action in (("save-timestamps", save_timestamps), ("restore-timestamps", restore_timestamps)):
        timestamps = commands.add_parser(name)
        timestamps.add_argument("--file", type=Path, required=True)
        timestamps.set_defaults(action=action)
    for name in ("restore", "inspect"):
        inspect = commands.add_parser(name)
        inspect.add_argument("--directory", type=Path, required=True)
        inspect.add_argument("--key", required=True)
        inspect.add_argument("--arch", choices=TARGETS, required=True)
        inspect.add_argument("--target", required=True)
        inspect.set_defaults(action=inspect_artifacts)
    args = parser.parse_args()
    args.action(args)


if __name__ == "__main__":
    main()

