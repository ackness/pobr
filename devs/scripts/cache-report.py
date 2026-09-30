"""Report build storage without compiling, deleting files or following symlinks."""

from collections import defaultdict
import json
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[2]


def usage(root, split=False):
    totals = defaultdict(lambda: [0, 0])
    if not root.exists() or root.is_symlink():
        return totals

    def fail(error):
        raise error

    for directory, _, files in os.walk(root, followlinks=False, onerror=fail):
        parts = Path(directory).relative_to(root).parts
        # Show host and cross-target profiles separately, including their large
        # deps/incremental directories; never descend into linked directories.
        depth = 2
        if parts and parts[0] in ("doc", "criterion", "cargo-timings", "tmp"):
            depth = 1
        elif len(parts) > 1 and parts[1] in ("debug", "release"):
            depth = 3
        bucket = "/".join(parts[:depth]) if split else "."
        for name in files:
            stat = (Path(directory) / name).lstat()
            totals[bucket or "."][0] += stat.st_blocks * 512 if hasattr(stat, "st_blocks") else stat.st_size
            totals[bucket or "."][1] += 1
    return totals


def size(value):
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if value < 1024 or unit == "TiB":
            return f"{value:.1f} {unit}"
        value /= 1024


def label(path):
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
        cwd=ROOT, text=True, encoding="utf-8",
    ))
    print("Allocated size      Files  Directory (read-only; symlinks not followed)")
    paths = [Path(metadata["target_directory"])]
    build = Path(metadata.get("build_directory", metadata["target_directory"]))
    if build not in paths:
        paths.append(build)
    paths.extend(ROOT / rel for rel in (
        ".cache", "web/node_modules", "web/dist", "web/src/wasm/pkg", "web/public/data",
    ))
    for path in paths:
        totals = usage(path, split=path in (paths[0], build))
        byte_count = sum(row[0] for row in totals.values())
        file_count = sum(row[1] for row in totals.values())
        print(f"{size(byte_count):>14} {file_count:>10,}  {label(path)}")
        if path in (paths[0], build):
            for bucket, (byte_count, file_count) in sorted(totals.items(), key=lambda row: -row[1][0]):
                if bucket != "." and file_count:
                    print(f"{size(byte_count):>14} {file_count:>10,}    {bucket}")
    print("Cargo artifacts are reused; dev/test incremental compilation is disabled by default.")
    print("Overrides: CARGO_INCREMENTAL/CARGO_PROFILE_*, RUSTFLAGS and local Cargo config can change this.")
    print("Independent worktrees keep their own target; Cargo commands in one worktree run sequentially.")
    print("Global Cargo/pnpm/compiler caches are separate; for existing sccache use 'sccache --show-stats'.")


if __name__ == "__main__":
    main()
