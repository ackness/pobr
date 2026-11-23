#!/usr/bin/env python3
"""Install a pinned CI tool (actionlint or gh) from its official release assets on Linux."""

from dataclasses import dataclass
import hashlib
import io
import os
from pathlib import Path
import re
import sys
import tarfile
import urllib.request


@dataclass(frozen=True)
class ToolSpec:
    label: str
    repo: str
    binary: str
    member_template: str


TOOLS = {
    "actionlint": ToolSpec(
        label="actionlint",
        repo="rhysd/actionlint",
        binary="actionlint",
        member_template="actionlint",
    ),
    "gh": ToolSpec(
        label="GitHub CLI",
        repo="cli/cli",
        binary="gh",
        member_template="{stem}/bin/gh",
    ),
}

ARCH = {"X64": "amd64", "ARM64": "arm64"}


def install(tool, version, architecture, directory, fetch):
    if tool not in TOOLS:
        raise ValueError(f"Unsupported CI tool: {tool!r}")
    spec = TOOLS[tool]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError(f"Expected an exact stable {spec.label} version")
    arch = ARCH[architecture]
    stem = f"{tool}_{version}_linux_{arch}"
    asset = f"{stem}.tar.gz"
    base = f"https://github.com/{spec.repo}/releases/download/v{version}/"
    checksums = fetch(base + f"{tool}_{version}_checksums.txt").decode()
    expected = {parts[1].lstrip("*"): parts[0] for line in checksums.splitlines()
                if len(parts := line.split()) == 2}.get(asset)
    if expected is None:
        raise ValueError(f"{spec.label} release checksum missing")
    archive = fetch(base + asset)
    if hashlib.sha256(archive).hexdigest() != expected:
        raise ValueError(f"{spec.label} release checksum mismatch")
    member_name = spec.member_template.format(stem=stem)
    with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
        member = bundle.getmember(member_name)
        if not member.isfile():
            raise ValueError(f"{spec.label} executable is not a regular file")
        extracted = bundle.extractfile(member)
        if extracted is None:
            raise ValueError(f"{spec.label} executable missing from release")
        payload = extracted.read()
    target = Path(directory) / "bin" / spec.binary
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(payload)
    target.chmod(0o755)
    return target.parent


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: install-tool.py <actionlint|gh> <version>")
    tool, version = sys.argv[1], sys.argv[2]
    if tool not in TOOLS:
        raise SystemExit(f"Unsupported CI tool: {tool}")
    if os.environ.get("RUNNER_OS") != "Linux":
        raise SystemExit(f"The CI {TOOLS[tool].label} installer supports Linux runners")

    def fetch(url):
        with urllib.request.urlopen(url, timeout=60) as response:
            return response.read()

    directory = install(tool, version, os.environ["RUNNER_ARCH"],
                        Path(os.environ["RUNNER_TEMP"]) / f"pobr-{tool}", fetch)
    with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as output:
        output.write(str(directory) + "\n")


if __name__ == "__main__":
    main()
