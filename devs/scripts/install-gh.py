#!/usr/bin/env python3
"""Install the pinned GitHub CLI from official release assets on Linux CI."""

import hashlib
import io
import os
from pathlib import Path
import re
import sys
import tarfile
import urllib.request


def install(version, architecture, directory, fetch):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("Expected an exact stable GitHub CLI version")
    architecture = {"X64": "amd64", "ARM64": "arm64"}[architecture]
    stem = f"gh_{version}_linux_{architecture}"
    asset = stem + ".tar.gz"
    base = f"https://github.com/cli/cli/releases/download/v{version}/"
    checksums = fetch(base + f"gh_{version}_checksums.txt").decode()
    expected = {parts[1].lstrip("*"): parts[0] for line in checksums.splitlines()
                if len(parts := line.split()) == 2}[asset]
    archive = fetch(base + asset)
    if hashlib.sha256(archive).hexdigest() != expected:
        raise ValueError("GitHub CLI release checksum mismatch")
    target = Path(directory) / "bin" / "gh"
    target.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
        source = bundle.extractfile(stem + "/bin/gh")
        if source is None:
            raise ValueError("GitHub CLI executable missing from release")
        target.write_bytes(source.read())
    target.chmod(0o755)
    return target.parent


def main():
    if os.environ.get("RUNNER_OS") != "Linux":
        raise SystemExit("The CI GitHub CLI installer supports Linux runners")
    def fetch(url):
        with urllib.request.urlopen(url, timeout=60) as response:
            return response.read()
    directory = install(sys.argv[1], os.environ["RUNNER_ARCH"],
                        Path(os.environ["RUNNER_TEMP"]) / "pobr-gh", fetch)
    with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as output:
        output.write(str(directory) + "\n")


if __name__ == "__main__":
    main()
