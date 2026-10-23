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


SCHEMA = 1
PROGRAMS = ("easytier-core", "easytier-cli", "easytier-web", "easytier-web-embed")
WEB_TREES = (
    "easytier-web/frontend/",
    "easytier-web/frontend-lib/",
    "easytier-web/config-generator/",
)
RECIPE_FILES = {
    ".gitattributes",
    ".github/workflows/node-attributes-image.yml",
    ".github/scripts/build-native-inputs.py",
    ".github/scripts/build-native.sh",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
}
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


def source_inputs():
    entries = []
    raw = subprocess.check_output(["git", "ls-files", "--stage", "-z"])
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        mode, _object, stage = metadata.decode().split()
        if stage != "0":
            raise ValueError("Native inputs contain an unresolved merge")
        entries.append((name.decode("utf-8"), mode))

    # Include complete local Cargo packages, including fonts, locales, SQL and
    # protobuf files. A future path dependency is covered without a .rs allowlist.
    crate_roots = {
        str(PurePosixPath(name).parent) + "/"
        for name, _ in entries
        if PurePosixPath(name).name == "Cargo.toml" and "/" in name
    }
    inputs = []
    for name, mode in sorted(entries):
        if name.startswith(WEB_TREES):
            continue
        selected = (
            name in RECIPE_FILES
            or name.startswith((".cargo/", ".github/actions/prepare-build/"))
            or PurePosixPath(name).name in ("Cargo.toml", "Cargo.lock")
            or any(name.startswith(root) for root in crate_roots)
        )
        if not selected:
            continue
        path = Path(name)
        if mode == "120000":
            payload = os.readlink(path).encode()
            digest = hashlib.sha256(payload).hexdigest()
        elif mode in ("100644", "100755"):
            digest = digest_file(path)
        else:
            raise ValueError(f"Unsupported native input mode {mode}: {name}")
        inputs.append({"path": name, "mode": mode, "sha256": digest})
    return inputs


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
        "program_features": [f"easytier/{args.allocator}"],
        "web_features": ["web-static", f"easytier/{args.allocator}"],
        "runner_os": os.environ.get("RUNNER_OS", ""),
        "runner_arch": os.environ.get("RUNNER_ARCH", ""),
        "runner_image": os.environ.get("ImageOS", ""),
        "runner_image_version": os.environ.get("ImageVersion", ""),
        "build_jobs": os.environ.get("CARGO_BUILD_JOBS", ""),
        "incremental": os.environ.get("CARGO_INCREMENTAL", ""),
    }
    content = {"recipe": recipe, "files": source_inputs()}
    encoded = json.dumps(content, sort_keys=True, separators=(",", ":")).encode()
    key = hashlib.sha256(encoded).hexdigest()
    record = {"key": key, **content}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + "\n")
    output("key", key)
    print(f"Native input key: {key} ({len(content['files'])} tracked inputs)")


def record_artifacts(args):
    inputs = json.loads(args.inputs.read_text())
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
