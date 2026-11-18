"""Prepare the current checkout for a local Web preview."""

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

from e2e import Inputs, ROOT, Stop


def stage_tunnel_inputs(stage):
    # Work from verified runtime bytes, and read static files and reference
    # builds from Git so old generated files cannot become public assets.
    sys.path.insert(0, str(ROOT))
    from pipeline.runtime.data_bundle import data_inputs

    lock = json.loads((ROOT / "pipeline/data.lock.json").read_bytes())
    files, identity = data_inputs(ROOT / "data", lock["target"]["game_patch"])
    if identity != lock["runtime_identity"]:
        raise Stop("prepared data differs from the data lock; run `mise run data:prepare`")
    for name, content in files.items():
        path = stage / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)

    with subprocess.Popen([
        "git", "archive", "HEAD", "web/public", "examples/demo-bd-test/builds",
    ], cwd=ROOT, stdout=subprocess.PIPE) as git:
        with tarfile.open(fileobj=git.stdout, mode="r|") as archive:
            archive.extractall(stage, filter="data")
        if git.wait():
            raise Stop("cannot stage tracked public files and reference builds")


def terminate(signum, _frame):
    raise SystemExit(128 + signum)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=4173, help="loopback port (default: 4173)")
    parser.add_argument("--tunnel", action="store_true", help="share the Web build with a public Cloudflare Quick Tunnel")
    parser.add_argument("--allow-wegame", action="store_true", help="allow WeGame share imports through the tunnel")
    args = parser.parse_args()
    if args.allow_wegame and not args.tunnel:
        parser.error("--allow-wegame requires --tunnel")
    if not 1 <= args.port <= 65535:
        parser.error("--port must be between 1 and 65535")
    if args.tunnel and not shutil.which("cloudflared"):
        parser.error("--tunnel requires cloudflared on PATH")

    env = dict(os.environ)
    # Use the same pinned worktree data and real backend as the application.
    for key in ("POBR_DATA_ROOT", "POBR_DATA_VERSION", "POBR_RUNTIME_PACK", "POBR_RUNTIME_RELEASE_ID"):
        env.pop(key, None)
    env["VITE_POBR_BACKEND"] = "wasm"

    stage = None
    inputs = Inputs(env=env)
    try:
        inputs.node_modules()
        command = ["node", "web/scripts/preview.mjs", str(args.port)]
        if args.tunnel:
            inputs.install_locked_data()
            inputs.wasm()
            signal.signal(signal.SIGTERM, terminate)
            stage = Path(tempfile.mkdtemp(prefix="pobr-web-tunnel-"))
            stage_tunnel_inputs(stage)
            command.extend(["--tunnel", str(stage)])
        else:
            data = inputs.data()
            inputs.wasm()
            inputs.dist(data)
        if args.allow_wegame:
            command.append("--allow-wegame")
        os.execvpe(command[0], command, env)
    except (Stop, OSError, ValueError, tarfile.TarError) as exc:
        print(f"STOP {exc}", file=sys.stderr)
        return 1
    finally:
        # After exec, the preview process owns this directory.
        if stage is not None:
            shutil.rmtree(stage)


if __name__ == "__main__":
    sys.exit(main())
