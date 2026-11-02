"""Keep full external payloads out of PoBR's Git index (including forced adds)."""

from pathlib import Path
import subprocess

from pipeline.imports.data_imports import regular, require
from pipeline.runtime.data_bundle import decode

ROOT = Path(__file__).resolve().parents[2]


def check(names, lock):
    allowed = {"data/CURRENT", *(f"data/overlay-common/{name}" for name in lock["shared_files"])}
    forbidden = sorted(name for name in names if (name.startswith("data/") and name not in allowed)
                       or name == "web/src/lib/supportDescriptions.zh-CN.json"
                       or (name.startswith("web/src/fixtures/") and name.endswith(".json")))
    require(not forbidden, "External data cannot be tracked in PoBR: " + ", ".join(forbidden))


if __name__ == "__main__":
    names = subprocess.check_output(["git", "ls-files", "-z", "--", "data",
                                     "web/src/lib/supportDescriptions.zh-CN.json", "web/src/fixtures"], cwd=ROOT).decode().split("\0")
    check([name for name in names if name], decode(regular(ROOT / "pipeline/runtime/build-data.lock.json")))
