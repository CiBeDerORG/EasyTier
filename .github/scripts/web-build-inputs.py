#!/usr/bin/env python3
"""Fingerprint build inputs and record or validate complete generated WASM output."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess


WASM_OUTPUT = Path("easytier-web/config-generator/src/generated/config-wasm")
MANIFEST = ".build-manifest.json"
INPUT_SNAPSHOT = "easytier-config-wasm-inputs.json"
REQUIRED_OUTPUTS = ("easytier_config.js", "easytier_config.d.ts", "easytier_config_bg.wasm")


def write_outputs(**values: str) -> None:
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        for name, value in values.items():
            stream.write(f"{name}={value}\n")


def output_hashes(directory: Path) -> dict[str, str]:
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("WASM output directory is missing or invalid")
    for name in REQUIRED_OUTPUTS:
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"Required WASM output is missing or empty: {name}")
    files = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("WASM output must not contain symbolic links")
        if path.is_file() and path != directory / MANIFEST:
            files[path.relative_to(directory).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return files


def record_output(root: Path, directory: Path, input_key: str) -> None:
    # Preparation steps may rewrite Cargo.lock after the key was computed.
    # Compare only source files: setup-rust may change compiler environment vars.
    snapshot = json.loads((Path(os.environ["RUNNER_TEMP"]) / INPUT_SNAPSHOT).read_text(encoding="utf-8"))
    if not isinstance(snapshot, dict) or snapshot.get("format") != 1 \
            or snapshot.get("input_key") != input_key \
            or snapshot.get("files") != source_inputs(root):
        raise SystemExit("WASM source inputs changed during preparation/build; refusing to cache output under an outdated key.")
    manifest = {"format": 1, "input_key": input_key, "files": output_hashes(directory)}
    (directory / MANIFEST).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def validate_output(directory: Path, input_key: str) -> None:
    valid = False
    try:
        manifest_path = directory / MANIFEST
        if not manifest_path.is_symlink():
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            valid = isinstance(manifest, dict) and manifest.get("format") == 1 \
                and manifest.get("input_key") == input_key \
                and manifest.get("files") == output_hashes(directory)
    except (OSError, ValueError, TypeError):
        pass
    write_outputs(valid="true" if valid else "false")
    if not valid:
        print("Cached config-generator WASM is incomplete or changed; rebuilding it.")


def source_inputs(root: Path) -> list[dict[str, str]]:
    tracked = {}
    for entry in subprocess.check_output(
        ["git", "ls-files", "--stage", "-z"], cwd=root
    ).split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        mode, _, stage = metadata.split()
        if stage != b"0":
            raise ValueError("Cannot fingerprint unmerged source inputs")
        tracked[os.fsdecode(name)] = mode.decode("ascii")
    # Include complete native package inputs, including build scripts/resources.
    # This is deliberately conservative for future local Rust dependencies.
    native_roots = {
        PurePosixPath(path).parent
        for path in tracked
        if PurePosixPath(path).name == "Cargo.toml"
        and PurePosixPath(path).parent != PurePosixPath(".")
    }
    explicit = {
        ".gitattributes",
        ".github/actions/prepare-pnpm/action.yml",
        ".github/scripts/web-build-inputs.py",
        "easytier-web/config-generator/package.json",
    }
    generated_parts = {"node_modules", "target", "dist", ".git"}
    frontend_parts = {"frontend", "frontend-lib", "config-generator"}

    def is_input(name: str) -> bool:
        path = PurePosixPath(name)
        if generated_parts.intersection(path.parts):
            return False
        if name in explicit:
            return True
        if path.name in {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"}:
            return True
        if ".cargo" in path.parts:
            return True
        if frontend_parts.intersection(path.parts):
            return False
        if path.suffix.lower() in {".rs", ".proto", ".c", ".cc", ".cpp", ".h", ".hpp", ".s"}:
            return True
        return any(package in path.parents for package in native_roots)

    return [
        {"path": name, "mode": tracked[name], "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()}
        for name in sorted(tracked)
        if is_input(name)
    ]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", nargs="?", default="key", choices=("key", "record", "validate"))
    parser.add_argument("--rust-toolchain")
    parser.add_argument("--protoc-version")
    parser.add_argument("--wasm-pack-version")
    parser.add_argument("--key", dest="input_key")
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[2]
    if args.action != "key":
        if not args.input_key:
            parser.error("--key is required for record/validate")
        if args.action == "record":
            record_output(root, root / WASM_OUTPUT, args.input_key)
        else:
            validate_output(root / WASM_OUTPUT, args.input_key)
        return
    if not all((args.rust_toolchain, args.protoc_version, args.wasm_pack_version)):
        parser.error("key requires --rust-toolchain, --protoc-version and --wasm-pack-version")

    # Do not include the commit SHA, timestamps or workspace/target paths.
    # A Vue-only change must reuse the same WASM output. Build settings that
    # can change that output remain part of the key, including caller overrides.
    compiler_env = {
        name: value
        for name, value in sorted(os.environ.items())
        if name.startswith(("CARGO_PROFILE_", "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_"))
        or name in {
            "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTFLAGS",
            "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER", "CARGO_INCREMENTAL", "SOURCE_DATE_EPOCH",
            "CC", "CXX", "CFLAGS", "CXXFLAGS", "AR", "ARFLAGS",
            "BINDGEN_EXTRA_CLANG_ARGS",
        }
    }
    settings = {
        "format": 3,
        "rust_toolchain": args.rust_toolchain,
        "protoc": args.protoc_version,
        "wasm_pack": args.wasm_pack_version,
        "target": "wasm32-unknown-unknown",
        "environment": compiler_env,
    }
    digest = hashlib.sha256()

    def add(label: str, content: bytes) -> None:
        label_bytes = label.encode("utf-8", errors="surrogateescape")
        digest.update(len(label_bytes).to_bytes(8, "big"))
        digest.update(label_bytes)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)

    add("settings", json.dumps(settings, sort_keys=True).encode())
    inputs = source_inputs(root)
    add("source_inputs", json.dumps(inputs, sort_keys=True).encode())
    input_key = digest.hexdigest()
    snapshot = {"format": 1, "input_key": input_key, "files": inputs}
    (Path(os.environ["RUNNER_TEMP"]) / INPUT_SNAPSHOT).write_text(
        json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    write_outputs(key=input_key, files=str(len(inputs)))


if __name__ == "__main__":
    main()
