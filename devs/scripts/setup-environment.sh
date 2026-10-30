#!/usr/bin/env bash
# Final native mise-bootstrap task. No call back to bootstrap, no product build.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
for tool in git make cc; do
  command -v "$tool" >/dev/null || { echo "setup: required host tool '$tool' is unavailable after mise package provisioning" >&2; exit 1; }
done
if [[ "$(uname -s)" == Darwin ]]; then
  xcode-select -p >/dev/null || { echo "setup: macOS requires Apple's Command Line Tools (xcode-select --install)" >&2; exit 1; }
fi
python3 devs/scripts/tool-versions.py --check
pnpm --dir web install --frozen-lockfile
pnpm --dir pipeline install --frozen-lockfile
python3 devs/scripts/tool-versions.py
echo "Everyday environment ready. Oracle, browser and artwork tools are prepared by their own tasks."
