#!/usr/bin/env python3
"""Install pinned actionlint from its official release assets on Linux CI."""

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
        raise ValueError("Expected an exact stable actionlint version")
    architecture = {"X64": "amd64", "ARM64": "arm64"}[architecture]
    asset = f"actionlint_{version}_linux_{architecture}.tar.gz"
    base = f"https://github.com/rhysd/actionlint/releases/download/v{version}/"
    checksums = fetch(base + f"actionlint_{version}_checksums.txt").decode()
    expected = {parts[1].lstrip("*"): parts[0] for line in checksums.splitlines()
                if len(parts := line.split()) == 2}.get(asset)
    if expected is None:
        raise ValueError("actionlint release checksum missing")
    archive = fetch(base + asset)
    if hashlib.sha256(archive).hexdigest() != expected:
        raise ValueError("actionlint release checksum mismatch")
    with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
        member = bundle.getmember("actionlint")
        if not member.isfile():
            raise ValueError("actionlint executable is not a regular file")
        payload = bundle.extractfile(member).read()
    target = Path(directory) / "bin" / "actionlint"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(payload)
    target.chmod(0o755)
    return target.parent


def main():
    if os.environ.get("RUNNER_OS") != "Linux":
        raise SystemExit("The CI actionlint installer supports Linux runners")
    def fetch(url):
        with urllib.request.urlopen(url, timeout=60) as response:
            return response.read()
    directory = install(sys.argv[1], os.environ["RUNNER_ARCH"],
                        Path(os.environ["RUNNER_TEMP"]) / "pobr-actionlint", fetch)
    with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as output:
        output.write(str(directory) + "\n")


if __name__ == "__main__":
    main()
