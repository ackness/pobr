"""Prepare the current checkout for a local Web preview."""

import argparse
import json
import os
import shutil
import subprocess
import sys

from e2e import Inputs, ROOT, Stop


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
    if args.tunnel:
        version = json.loads((ROOT / "pipeline/data.lock.json").read_bytes())["target"]["game_patch"]
        patch = ROOT / "data" / version / "patch"
        if patch.exists() and any(patch.iterdir()):
            parser.error("--tunnel does not publish local data patches; use a clean preview worktree")
        # Vite copies public/ verbatim, including gitignored files.
        extra = subprocess.check_output([
            "git", "ls-files", "--others", "--exclude=web/public/data/",
            "--exclude=web/public/build-references/", "--", "web/public",
        ], cwd=ROOT)
        if extra:
            parser.error("--tunnel does not publish untracked files in web/public; use a clean preview worktree")

    inputs = Inputs(env=env)
    try:
        inputs.node_modules()
        data = inputs.data()
        inputs.wasm()
        inputs.dist(data)
    except Stop as exc:
        print(f"STOP {exc}", file=sys.stderr)
        return 1

    command = ["node", "web/scripts/preview.mjs", str(args.port)]
    if args.tunnel:
        command.append("--tunnel")
    if args.allow_wegame:
        command.append("--allow-wegame")
    os.execvpe(command[0], command, env)


if __name__ == "__main__":
    sys.exit(main())
