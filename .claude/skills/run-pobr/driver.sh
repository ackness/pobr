#!/usr/bin/env bash
# Compatibility entry: the driver lives in devs/scripts/driver.sh.
exec bash "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/devs/scripts/driver.sh" "$@"
