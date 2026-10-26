#!/usr/bin/env bash
# Runtime used by the pinned PoB2 Dockerfile (including compound assignments).
# Installation is explicit; resolve/status never download or build anything.
set -euo pipefail

LUAJIT_COMMIT=2460b3ff93a1c955de3d62cfc825de7d68dc272e
RUNTIME_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}/pobr/runtimes"
RUNTIME_SOURCE="$RUNTIME_ROOT/luajit-$LUAJIT_COMMIT"
RUNTIME_BIN="$RUNTIME_SOURCE/src/luajit"

compatible() {
  "$1" -e 'assert(jit); local n = 1; n += 1; assert(n == 2)' </dev/null >/dev/null 2>&1
}

resolve_candidate() {
  local executable
  executable="$(command -v "$1")" || return 1
  [[ -x "$executable" ]] || return 1
  compatible "$executable" || return 1
  # Resolve relative overrides before the oracle changes its working directory.
  printf '%s/%s\n' "$(cd "$(dirname "$executable")" && pwd)" "$(basename "$executable")"
}

resolve() {
  local candidate explicit="${LUAJIT:-${POBR_LUAJIT:-}}"
  if [[ -n "$explicit" ]]; then
    resolve_candidate "$explicit" && return 0
    echo "PoB2: explicit LuaJIT '$explicit' is unavailable or lacks compound assignment (+=) support." >&2
  else
    for candidate in "$RUNTIME_BIN" /opt/homebrew/bin/luajit luajit; do
      resolve_candidate "$candidate" && return 0
    done
    echo "PoB2: no compatible LuaJIT found; stock distro versions may lack += support." >&2
  fi
  echo "Run 'mise run deps', or set LUAJIT / POBR_LUAJIT to a compatible executable." >&2
  return 127
}

install_runtime() {
  local executable
  if executable="$(resolve 2>/dev/null)"; then
    echo "PoB2 LuaJIT ready: $executable"
    return 0
  fi
  # A supplied override is authoritative, even when it is invalid.
  if [[ -n "${LUAJIT:-${POBR_LUAJIT:-}}" ]]; then resolve; return $?; fi
  for executable in git make cc; do
    command -v "$executable" >/dev/null || {
      echo "PoB2 LuaJIT setup needs $executable (Git, make and a C compiler)." >&2
      return 1
    }
  done
  if [[ -e "$RUNTIME_SOURCE" ]]; then
    echo "PoB2: incompatible runtime cache at $RUNTIME_SOURCE; remove it and rerun 'mise run deps'." >&2
    return 1
  fi
  mkdir -p "$RUNTIME_ROOT"
  # Keep this visible to EXIT even after the function unwinds on a failed build.
  RUNTIME_STAGING="$(mktemp -d "$RUNTIME_ROOT/.luajit-build.XXXXXX")"
  trap 'rm -rf -- "$RUNTIME_STAGING"' EXIT
  git -C "$RUNTIME_STAGING" init -q
  git -C "$RUNTIME_STAGING" fetch -q --depth 1 https://github.com/LuaJIT/LuaJIT.git "$LUAJIT_COMMIT"
  git -C "$RUNTIME_STAGING" checkout -q --detach FETCH_HEAD
  make -C "$RUNTIME_STAGING" -j2
  compatible "$RUNTIME_STAGING/src/luajit"
  mv "$RUNTIME_STAGING" "$RUNTIME_SOURCE"
  trap - EXIT
  echo "PoB2 LuaJIT installed: $RUNTIME_BIN"
}

case "${1:-resolve}" in
  resolve) resolve ;;
  install) install_runtime ;;
  *) echo "usage: pob2-luajit.sh [resolve|install]" >&2; exit 2 ;;
esac
