#!/usr/bin/env bash
# driver.sh — pobr environment bootstrap, smoke checks, and vendored PoB2 reference clone.
#
# Scripted setup from a clean environment to running tests and consulting PoB2 Lua formulas.
# Subcommands:
#   bootstrap  Install LuaJIT/build deps + clone vendored PoB2 (pinned commit) + CLI build
#   deps       Install/cache the compatible PoB2 LuaJIT runtime
#   vendor     Clone/align vendored PoB2 to the pinned commit (gitignored, never committed)
#   build      Build the application CLI, or forward explicit Cargo selectors.
#   test <args> Forward targeted cargo test arguments (package, suite, filter).
#   lint <args> Format check plus Clippy for explicitly selected Cargo targets.
#   full       fmt + clippy + all workspace tests
#   gate       optional local batch validation; reports every failing step
#   regen      precompile-mods --report, then its tests
#   warm       copy another worktree's target/debug (clone or cp -a)
#   smoke      Quick representative aggregation, parser and codec checks.
#   data       Data status check + pipeline commands
#   lua <pat>  Grep a pattern in vendored PoB2 Lua (for formula/rule cross-checks)
#   status     Report luajit / vendor / data readiness
set -uo pipefail

CALLER_PWD="$PWD"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

DATA_ROOT="${POBR_DATA_ROOT:-$ROOT/data}"
DATA_VER="${POBR_DATA_VERSION:-}"
if [[ -z "$DATA_VER" && -f "$DATA_ROOT/CURRENT" ]]; then
  DATA_VER="$(head -n 1 "$DATA_ROOT/CURRENT" | tr -d '[:space:]')"
fi
VENDOR_DIR="vendor/PathOfBuilding-PoE2"

# Resolve an existing relative path against the caller's cwd first, falling back to $ROOT.
resolve_existing_path() {
  local target="$1"
  if [[ "$target" != /* ]]; then
    if [[ -e "$CALLER_PWD/$target" ]]; then
      printf '%s\n' "$CALLER_PWD/$target"
      return 0
    elif [[ -e "$ROOT/$target" ]]; then
      printf '%s\n' "$ROOT/$target"
      return 0
    fi
  fi
  printf '%s\n' "$target"
}

say() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }

reference_field() {
  python3 - "$ROOT/devs/reference-sources.json" "$1" <<'PY'
import json, re, sys
try:
    with open(sys.argv[1], encoding="utf-8") as source:
        document = json.load(source)
    reference = document["pob2"]
    if document["schema_version"] != 1 or not re.fullmatch(r"[0-9a-f]{40}", reference["commit"]):
        raise ValueError("expected schema 1 and a full commit SHA")
    if not re.fullmatch(r"https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\.git", reference["repository"]):
        raise ValueError("expected an HTTPS GitHub repository URL")
    if reference["protocol"] != "pob2-oracle/v1":
        raise ValueError("unsupported oracle protocol")
    print(reference[sys.argv[2]])
except (OSError, ValueError, KeyError, TypeError) as error:
    raise SystemExit(f"Invalid reference source {sys.argv[1]}: {error}") from error
PY
}

vendor_commit() { reference_field commit; }

cmd_deps() {
  say "PoB2 LuaJIT runtime"
  local managed="$ROOT/.cache/runtime/bin/luajit" runtime
  # The project requests this alias before first setup. Only its installer may
  # refresh the managed alias; arbitrary explicit overrides remain fatal.
  if [[ "${POBR_LUAJIT:-}" == "$managed" && -z "${LUAJIT:-}" ]]; then
    env -u POBR_LUAJIT bash "$ROOT/devs/scripts/pob2-luajit.sh" install || return $?
    runtime="$(env -u POBR_LUAJIT bash "$ROOT/devs/scripts/pob2-luajit.sh" resolve)" || return $?
  else
    bash "$ROOT/devs/scripts/pob2-luajit.sh" install || return $?
    runtime="$(bash "$ROOT/devs/scripts/pob2-luajit.sh" resolve)" || return $?
  fi
  mkdir -p "$(dirname "$managed")"
  # Refresh even when the runtime came from the shared installation cache.
  [[ "$runtime" == "$managed" ]] || ln -sfn "$runtime" "$managed"
}

cmd_vendor() {
  say "PoB2 vendor reference clone (gitignored, never committed)"
  local sha repo
  sha="$(vendor_commit)" || return 1
  repo="$(reference_field repository)" || return 1
  mkdir -p "$VENDOR_DIR" || return 1
  local vendor_root
  vendor_root="$(git -C "$VENDOR_DIR" rev-parse --show-toplevel 2>/dev/null)" || vendor_root=""
  if [[ -n "$vendor_root" && "$(cd "$vendor_root" && pwd -P)" != "$(cd "$VENDOR_DIR" && pwd -P)" ]]; then
    vendor_root=""
  fi
  if [[ -z "$vendor_root" ]]; then
    [[ -z "$(ls -A "$VENDOR_DIR")" ]] || { echo "ERROR: vendor directory is non-empty and not an isolated Git repo" >&2; return 1; }
    git -C "$VENDOR_DIR" init -q || return 1
  fi
  local vendor_changes
  vendor_changes="$(git -C "$VENDOR_DIR" status --porcelain --untracked-files=no)" || return 1
  [[ -z "$vendor_changes" ]] \
    || { echo "ERROR: vendor checkout has uncommitted changes; refusing to switch commit" >&2; return 1; }
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ] \
     && [ "$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)" = "$sha" ]; then
    printf '%s\n' "$sha" > vendor/.pob2-version.txt || return 1
    echo "vendor is already at pinned commit $sha"; return 0
  fi
  echo "Cloning PoB2 @ ${sha} (depth 1 fetch-by-sha)..."
  git -C "$VENDOR_DIR" remote get-url origin >/dev/null 2>&1 \
    || git -C "$VENDOR_DIR" remote add origin "$repo" || return 1
  local origin; origin="$(git -C "$VENDOR_DIR" remote get-url origin)" || return 1
  [[ "${origin%.git}" == "${repo%.git}" ]] \
    || { echo "ERROR: vendor origin differs from devs/reference-sources.json" >&2; return 1; }
  git -C "$VENDOR_DIR" fetch -q --depth 1 origin "$sha" || return 1
  git -C "$VENDOR_DIR" checkout -q FETCH_HEAD || return 1
  [[ "$(git -C "$VENDOR_DIR" rev-parse HEAD)" == "$sha" ]] || return 1
  printf '%s\n' "$sha" > vendor/.pob2-version.txt || return 1
  echo "OK: $(git -C "$VENDOR_DIR" log -1 --oneline)"
}

cmd_build() {
  if [[ $# -eq 0 ]]; then set -- -p pobr-cli; fi
  say "cargo build $*"
  cargo build "$@"
}

# Accept a package/suite shorthand without changing the legacy Cargo interface.
# Keep arguments in an array: filters may contain whitespace or shell characters.
select_targets() {
  CARGO_ARGS=()
  if [[ $# -eq 0 ]]; then
    echo "usage: mise run <test|lint|timings> <crate> <suite|lib> [args]" >&2
    echo "Raw Cargo selectors also work: -p <crate> --test <suite>" >&2
    return 2
  fi
  if [[ "$1" == -* ]]; then CARGO_ARGS=("$@"); return 0; fi
  if [[ $# -lt 2 || "$2" == -* ]]; then
    echo "Select a suite explicitly; use 'mise run targets' to list them." >&2
    return 2
  fi
  local package="$1" suite="$2"
  [[ "$package" == *-* ]] || package="pobr-$package"
  CARGO_ARGS=(-p "$package")
  if [[ "$suite" == lib ]]; then
    CARGO_ARGS+=(--lib)
  else
    CARGO_ARGS+=(--test "$suite")
  fi
  shift 2
  CARGO_ARGS+=("$@")
}

cmd_test() {
  select_targets "$@" || return $?
  say "cargo test ${CARGO_ARGS[*]}"
  cargo test "${CARGO_ARGS[@]}"
}

cmd_lint() {
  select_targets "$@" || return $?
  say "Targeted lint: fmt and cargo clippy ${CARGO_ARGS[*]}"
  cargo fmt --all --check && cargo clippy "${CARGO_ARGS[@]}" -- -D warnings
}

cmd_verify() {
  if [[ $# -lt 2 || $# -gt 3 || "$1" == -* || "$2" == -* || "${3:-}" == -* ]]; then
    echo "usage: mise run verify <crate> <suite|lib> [filter]" >&2
    return 2
  fi
  cmd_test "$@" && cmd_lint "$1" "$2"
}

# One build: what its import dropped, where the result differs from the
# reference its code carries, and every input the calculation lost on the way.
cmd_diagnose() {
  if [[ $# -ne 1 || "$1" == -* ]]; then
    echo "usage: mise run diagnose <build dir | code file | committed build name fragment>" >&2
    return 2
  fi
  local target="$1"
  # A relative path is the caller's; the test binary runs from its crate.
  target="$(resolve_existing_path "$target")"
  POBR_DIAGNOSE="$target" cargo test -q -p pobr-build --test parity \
    diagnose_build::diagnose_build -- --ignored --nocapture
}

# One build against the pinned PoB2, stat by stat: both totals and the
# modifier lines only one side has or has with another value.
cmd_compare() {
  if [[ $# -lt 1 || "$1" == -* ]]; then
    echo "usage: mise run compare <build dir | code file | committed build name fragment> [Stat ...] [--refresh]" >&2
    return 2
  fi
  local target="$1"
  shift
  target="$(resolve_existing_path "$target")"
  python3 devs/scripts/compare-build.py "$target" "$@"
}

# Every worked build (the benchmark and tuned sets by default) against its
# reference and the pinned PoB2 at once: which differing rows are the
# engine's, and what explains them across builds.
cmd_triage() {
  local args=() arg
  for arg in "$@"; do
    if [[ "$arg" != -* ]]; then
      arg="$(resolve_existing_path "$arg")"
    fi
    args+=("$arg")
  done
  python3 devs/scripts/triage.py ${args[@]+"${args[@]}"}
}

# Synthetic single-skill builds against both calculators; oracle results cached.
cmd_skill_sweep() {
  python3 devs/scripts/skill-sweep.py "$@"
}

# The game data records an id or a name has: a granted effect, stat, base
# item, passive node or unique.
cmd_lookup() {
  if [[ $# -lt 1 || "$1" == -* ]]; then
    echo "usage: mise run lookup <skill id | stat id | base item | passive node | unique> [--level N] [--version V]" >&2
    return 2
  fi
  python3 devs/scripts/lookup.py "$@"
}

# Rank missing game-text support, or show one line's complete research card.
cmd_gaps() {
  python3 devs/scripts/gaps.py "$@"
}

# Builds against the reference values their codes carry: the totals of every
# committed set, or one directory with each build's differences.
cmd_accuracy() {
  if [[ $# -gt 1 || "${1:-}" == -* ]]; then
    echo "usage: mise run accuracy [dir of */code.txt; default: totals of every committed set]" >&2
    return 2
  fi
  local corpus="${1:-}"
  if [[ -n "$corpus" && "$corpus" != /* ]]; then
    if [[ -e "$CALLER_PWD/$corpus" ]]; then
      corpus="$CALLER_PWD/$corpus"
    elif [[ -e "$ROOT/$corpus" ]]; then
      corpus="$ROOT/$corpus"
    else
      corpus="$CALLER_PWD/$corpus"
    fi
  fi
  ${corpus:+env POBR_REFERENCE_BUILDS="$corpus"} cargo test -q -p pobr-build --test parity \
    reference_report::reference_report -- --ignored --nocapture
}

cmd_snapshot() {
  local mode="$1"
  shift
  if [[ $# -ne 0 ]]; then
    echo "usage: mise run snapshot:<record|report>" >&2
    return 2
  fi
  POBR_SNAPSHOT_MODE="$mode" cargo test -q -p pobr-build --test parity \
    output_snapshot::snapshot_update -- --exact --ignored --nocapture
}

cmd_timings() {
  select_targets "$@" || return $?
  say "Compile only; Cargo writes target/cargo-timings/cargo-timing.html"
  cargo test --no-run --timings "${CARGO_ARGS[@]}"
}

cmd_targets() {
  cargo metadata --no-deps --format-version 1 --locked | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
members = set(metadata["workspace_members"])
for package in sorted(metadata["packages"], key=lambda p: p["name"]):
    if package["id"] not in members:
        continue
    targets = ["lib" if "lib" in t["kind"] or "rlib" in t["kind"] else t["name"]
               for t in package["targets"] if t["test"] and
               any(k in t["kind"] for k in ("lib", "rlib", "test"))]
    if targets:
        print(package["name"] + ": " + " ".join(targets))
'
}

cmd_cache() {
  if [[ $# -ne 0 ]]; then
    echo "usage: mise run cache (read-only disk usage and file counts)" >&2
    return 2
  fi
  python3 devs/scripts/cache-report.py
}

cmd_ci() {
  if [[ " $* " == *" --e2e "* ]]; then
    echo "mise run ci no longer runs Playwright; run the browser suite locally: mise run e2e [smoke|affected|default|release]" >&2
    return 2
  fi
  if [[ $# -ne 1 || "$1" == -* ]]; then
    echo "usage: mise run ci <pushed-branch-or-tag>" >&2
    echo "Runs the remote ref, not uncommitted or unpushed local changes." >&2
    echo "Browser tests run locally: mise run e2e [smoke|affected|default|release]." >&2
    return 2
  fi
  gh workflow run ci.yml --ref "$1" &&
    echo "CI requested for remote ref '$1'. Check 'mise run ci-status'; this is not a passing result."
}

cmd_help() {
  cat <<'EOF'
PoBR development commands (mise tasks; without mise run this script with the same arguments):
  mise run targets                          List Rust suites without compiling
  mise run cache                            Report cache size and file counts (read-only)
  mise run verify build skills [filter]      Targeted test + fmt + Clippy
  mise run test core parser [filter]         Test one suite (or use 'lib')
  mise run lint build skills                fmt + targeted Clippy
  mise run timings build parity             Compile only, with HTML timings
  mise run diagnose <build>                 One build: import losses, reference differences, dropped inputs
  mise run compare <build> [Stat ...]       One build against the pinned PoB2: totals and differing modifier lines
  mise run triage [set or build dir ...]    Every worked build at once: whose differences, and their shared causes
  mise run skills:sweep [--jobs N]           Every primary player active skill against the pinned PoB2
  mise run lookup <id or name> [--level N]  Game data records of a skill, stat, base item, passive node or unique
  mise run gaps [--kind unique] [--limit N] Ranked game-text gaps across the worked builds
  mise run gap "<template or text>"        Stats, full sources, ledger fates and nearby rules for one line
  mise run accuracy [corpus dir]            Totals of every committed set, or one set's builds against their reference
  mise run snapshot:report                  L2 changed stats with empty attribution cells
  mise run snapshot:record                  Re-record L2 after reviewing intentional changes
  mise run builds:fetch <set dir> [--pick first|last]  One poe.ninja build per ascendancy into a reference set
  mise run build [-p <crate>]               Build application CLI by default
  mise run smoke                            Representative environment checks
  mise run full                             Explicit full local Rust gate
  mise run gate [--only step,...]           Optional warm-checkout batch validation; remote CI preferred
  mise run regen                            Regenerate parser artifacts and test precompile-mods
  mise run warm [source worktree]           Copy another worktree's target/debug (clone or cp -a)
  mise run ci <pushed-branch-or-tag>        Request full cloud CI via gh (no Playwright; see mise run e2e)
  mise run ci-status [--branch <branch>]     List cloud CI results via gh

test/lint/timings also accept raw Cargo arguments (-p, --test, --lib, ...).
Use --test/--lib to bound compilation; a test-name filter alone cannot do that.
Full release CI runs on tags. Do not run an identical full gate locally first.
Data/setup tasks: status, deps, vendor, bootstrap, data:prepare, data:check, data:status, versions, data:diff, vendor:lua.
Script subcommands for the renamed tasks: data, diff, lua.
EOF
}

cmd_full() {
  say "Full workspace gate: fmt, clippy, tests (including doctests)"
  cargo fmt --all --check &&
    cargo clippy --workspace --all-targets --features pobr-core/benchmarks,pobr-build/benchmarks,pobr-wasm/diagnostics -- -D warnings &&
    cargo test --workspace
}

# Regenerate the parser artifacts after a rule change, then test the generator.
cmd_regen() {
  if [[ $# -gt 0 ]]; then echo "usage: mise run regen" >&2; return 2; fi
  say "precompile-mods --report (data/$DATA_VER)"
  cargo run -q -p precompile-mods -- --data "$DATA_ROOT/$DATA_VER" --report &&
    cargo test -q -p precompile-mods
}

# A new worktree starts with an empty target and rebuilds every dependency.
# Copy another worktree's target/debug (Rust tests). Prefer a copy-on-write clone
# (APFS clonefile, Linux reflink); without one, a plain `cp -a` that keeps mtimes,
# which Cargo's fingerprints compare. Never link files: Cargo rewrites artifacts in
# place and would corrupt the source. The WASM build (target/wasm32-unknown-unknown,
# target/wasm-release) is not warmed: LTO and wasm-opt dominate its time. target/release
# is not warmed either: only release packaging builds it.

# Clone $1 to $2 copy-on-write; fail quietly when the filesystem cannot.
# Incremental state is tied to absolute paths, so drop it after cloning too.
warm_clone() {
  if [[ "$(uname)" == Darwin ]]; then
    cp -ac "$1" "$2" 2>/dev/null && find "$2" -name incremental -type d -prune -exec rm -rf {} + && return 0
  else
    cp -a --reflink=always "$1" "$2" 2>/dev/null && find "$2" -name incremental -type d -prune -exec rm -rf {} + && return 0
  fi
  rm -rf "$2"; return 1
}

# Plain mtime-preserving copy of $2 (size $1 KB) to $3; refuses below twice the
# size in free space. Incremental state is tied to absolute paths, so it is dropped.
warm_copy() {
  local free_kb
  free_kb="$(df -Pk target | awk 'NR==2 {print $4}')"
  if [[ ! "$free_kb" =~ ^[0-9]+$ ]]; then echo "ERROR: cannot read free disk space" >&2; return 1; fi
  if (( free_kb < $1 * 2 )); then
    echo "ERROR: need about $(( $1 * 2 / 1024 )) MB free for the copy, only $(( free_kb / 1024 )) MB available" >&2
    return 1
  fi
  cp -a "$2" "$3" && find "$3" -name incremental -type d -prune -exec rm -rf {} +
}

cmd_warm() {
  if [[ $# -gt 1 || "${1:-}" == -* ]]; then
    echo "usage: mise run warm [source worktree; default: the repository's main worktree]" >&2
    return 2
  fi
  local source="${1:-}"
  if [[ -z "$source" ]]; then
    source="$(git worktree list --porcelain 2>/dev/null | sed -n '1s/^worktree //p')"
  elif [[ "$source" != /* ]]; then
    if [[ -d "$CALLER_PWD/$source" ]]; then
      source="$CALLER_PWD/$source"
    elif [[ -d "$ROOT/$source" ]]; then
      source="$ROOT/$source"
    else
      source="$CALLER_PWD/$source"
    fi
  fi
  [[ -n "$source" && -d "$source/target/debug" ]] \
    || { echo "ERROR: no target/debug under source worktree '${source}'" >&2; return 1; }
  if [[ "$(cd "$source" && pwd -P)" == "$(pwd -P)" ]]; then
    echo "ERROR: source is this worktree" >&2; return 1
  fi
  if [[ -e target/debug ]]; then
    echo "target/debug already exists; leaving it unchanged"; return 0
  fi
  mkdir -p target || return 1
  # Stale temps come from a killed earlier run (SIGKILL skips the cleanup below).
  local stale
  for stale in target/.debug.warm.*; do [[ -e "$stale" ]] && rm -rf "$stale"; done
  local src="$source/target/debug" size_kb start=$SECONDS
  size_kb="$(du -sk "$src" | awk '{print $1}')"
  if [[ ! "$size_kb" =~ ^[0-9]+$ ]]; then echo "ERROR: cannot measure $src" >&2; return 1; fi
  say "Warm target/debug ($((size_kb / 1024)) MB) from $source"
  # Build in a sibling temp and rename last, so an interrupted or failed copy
  # never leaves a target/debug that the next run would trust.
  WARM_TMP="target/.debug.warm.$$"
  trap 'rm -rf "$WARM_TMP"; trap - INT TERM; exit 130' INT TERM
  local method=""
  if warm_clone "$src" "$WARM_TMP"; then
    method="copy-on-write clone"
  elif warm_copy "$size_kb" "$src" "$WARM_TMP"; then
    method="no copy-on-write here; plain copy (cp -a, mtimes kept), $((size_kb / 1024)) MB of disk"
  fi
  if [[ -n "$method" ]] && mv "$WARM_TMP" target/debug; then
    trap - INT TERM
    echo "OK: $method, in $((SECONDS - start)) s; Cargo reuses unchanged dependencies"
    return 0
  fi
  rm -rf "$WARM_TMP"; trap - INT TERM
  echo "ERROR: could not warm target/debug; build normally" >&2
  return 1
}

# The batch gate a coordinator runs once after merging several changes: every
# step runs even after a failure, so one pass shows all failures, and
# `--only <step,...>` reruns just the failed ones.
GATE_STEPS=(fmt regen clippy core build parity apps web accuracy)

gate_step() {
  case "$1" in
    fmt)      cargo fmt --all --check ;;
    regen)    cmd_regen ;;
    clippy)   cargo clippy --workspace --lib --tests -- -D warnings ;;
    core)     cargo test -q -p pobr-core ;;
    build)    cargo test -q -p pobr-build --lib --test skills --test codec --test dualrun --test tree_convert ;;
    parity)   cargo test -q -p pobr-build --test parity ;;
    apps)     cargo test -q -p pobr-data -p pobr-gamedata -p pobr-i18n -p pobr-tree -p pobr-item \
                -p pobr-wasm -p pobr-cli ;;
    web)      pnpm --dir web i18n:check && pnpm --dir web typecheck && pnpm --dir web test ;;
    accuracy) cmd_accuracy ;;
  esac
}

cmd_gate() {
  local steps=("${GATE_STEPS[@]}")
  if [[ "${1:-}" == --only && -n "${2:-}" && $# -eq 2 ]]; then
    IFS=, read -r -a steps <<<"$2"
  elif [[ $# -gt 0 ]]; then
    echo "usage: mise run gate [--only step,...]   steps: ${GATE_STEPS[*]}" >&2
    return 2
  fi
  local step known
  for step in "${steps[@]}"; do
    known=0
    for name in "${GATE_STEPS[@]}"; do [[ "$step" == "$name" ]] && known=1; done
    (( known )) || { echo "unknown gate step '$step'; steps: ${GATE_STEPS[*]}" >&2; return 2; }
  done
  # A comma list, not an array: bash 3.2 (macOS) rejects an empty array under `set -u`.
  local failed="" status
  for step in "${steps[@]}"; do
    say "gate: $step"
    gate_step "$step"; status=$?
    (( status == 0 )) || failed="${failed:+$failed,}$step"
  done
  say "gate summary"
  for step in "${steps[@]}"; do
    if [[ ",$failed," == *",$step,"* ]]; then echo "  FAIL $step"; else echo "  ok   $step"; fi
  done
  if [[ -n "$failed" ]]; then
    echo "Rerun the failures after fixing: mise run gate --only $failed"
    return 1
  fi
}

cmd_smoke() {
  say "Quick smoke: aggregation, parser and Build Code roundtrip"
  cargo test -p pobr-core --test aggregation mod_db:: &&
    cargo test -p pobr-core --test parser mod_parser:: &&
    cargo test -p pobr-build --test codec build_code::
}

cmd_data() {
  say "Data status self-check"
  if [ -d "$DATA_ROOT/$DATA_VER" ]; then
    echo "✓ data/$DATA_VER is staged locally (base/overlay/generated/i18n) — pinned via pipeline/data.lock.json"
    ls "$DATA_ROOT/$DATA_VER" | sed 's/^/    /'
  else
    echo "✗ data/$DATA_VER is missing (run mise run data:prepare first)"
  fi
  cat <<'EOF'

Data comes exclusively from ackness/repoe and ackness/poe2-zh-cn-stats releases pinned in pipeline/data.lock.json (see pipeline/README.md):
  1) Active pinned input    : mise run data:prepare (stage locally); mise run data:check (verify clean worktree offline)
  2) Compact release pack   : mise run data:pack build --out <fresh>
  3) New version candidate  : mise run data:update --lock <data-lock> --out <fresh>
  4) Explicit local adoption: add --activate --expected-current <old-version> to the command above
Version, GameData, and parity gates run before adoption; failures roll back, and source acceptance/deployment are handled separately.
Official patch discovery, raw extraction, and locale generation live in the owner repositories; PoBR never reads raw table exports or vendor Lua to generate data.
EOF
}

cmd_lua() {
  local pat="${2:-}"
  [ -n "$pat" ] || { echo "usage: driver.sh lua '<pattern>'  (grep in vendor PoB2 Lua)"; return 2; }
  [ -d "$VENDOR_DIR/src" ] || { echo "vendor is not cloned; run: driver.sh vendor"; return 1; }
  say "grep vendor PoB2 Lua: $pat"
  grep -rnF -- "$pat" "$VENDOR_DIR/src/Modules/ModParser.lua" \
    "$VENDOR_DIR/src/Modules/CalcOffence.lua" \
    "$VENDOR_DIR/src/Modules/CalcDefence.lua" \
    "$VENDOR_DIR/src/Modules/CalcPerform.lua" 2>/dev/null | head -30
}

cmd_versions() {
  say "Prepared data versions + multi-version agnostic smoke test"
  echo "data/ version directories:"; ls -d "$DATA_ROOT"/[0-9]*/ 2>/dev/null | sed 's#/$##; s#.*/##' | sed 's/^/    /'
  echo "Active data version: ${DATA_VER} (tests and PoB2 oracle values target it)"
  echo "Switch to another version (zero code changes): export POBR_DATA_VERSION=<ver>  or write data/CURRENT"
  echo "→ multi_version smoke (BuildData::load + calc per version):"
  cargo test -p pobr-build --test parity multi_version -- --nocapture 2>&1 \
    | grep -E "multi-version\]|test result:"
}

cmd_diff() {
  local a="${2:-}" b="${3:-}"
  if [ -z "$a" ] || [ -z "$b" ]; then
    echo "usage: driver.sh diff <verA> <verB> [--domain tree|mods|bases|skill_levels|skill_stats|special_mods|uniques] [--limit N]"
    echo "Prepared versions:"; ls -d "$DATA_ROOT"/[0-9]*/ 2>/dev/null | sed 's#/$##; s#.*/##; s/^/    /'
    return 2
  fi
  say "Semantic data diff: $a -> ${b} (added/removed/changed nodes, skills, mods)"
  python3 -m pipeline.diagnostics.diff_data "$DATA_ROOT/$a" "$DATA_ROOT/$b" --semantic "${@:4}"
}

cmd_status() {
  say "Readiness status"
  local runtime
  if runtime="$(bash "$ROOT/devs/scripts/pob2-luajit.sh" resolve)"; then
    echo "✓ PoB2 LuaJIT: $runtime"
  else
    echo "✗ compatible PoB2 LuaJIT unavailable (mise run deps)"
  fi
  local sha; sha="$(vendor_commit)"
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ]; then
    local have; have="$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)"
    [ "$have" = "$sha" ] && echo "✓ vendor: pinned at commit $sha" || echo "~ vendor: at ${have} (pinned ${sha}; run driver.sh vendor to sync)"
  else echo "✗ vendor not cloned (driver.sh vendor)"; fi
  [ -d "$DATA_ROOT/$DATA_VER" ] && echo "✓ data: $DATA_VER ready" || echo "✗ data: $DATA_VER missing (mise run data:prepare)"
  command -v cargo >/dev/null && echo "✓ cargo: $(cargo --version)" || echo "✗ cargo missing"
}

case "${1:-}" in
  reference) reference_field "${2:-commit}" ;;
  deps)      cmd_deps ;;
  vendor)    cmd_vendor ;;
  bootstrap) cmd_deps && cmd_vendor && cmd_build ;;
  build)     shift; cmd_build "$@" ;;
  test)      shift; cmd_test "$@" ;;
  lint)      shift; cmd_lint "$@" ;;
  verify)    shift; cmd_verify "$@" ;;
  timings)   shift; cmd_timings "$@" ;;
  diagnose)  shift; cmd_diagnose "$@" ;;
  compare)   shift; cmd_compare "$@" ;;
  triage)    shift; cmd_triage "$@" ;;
  skills:sweep) shift; cmd_skill_sweep "$@" ;;
  lookup)    shift; cmd_lookup "$@" ;;
  gaps|gap)  cmd_gaps "$@" ;;
  accuracy)  shift; cmd_accuracy "$@" ;;
  snapshot:record) shift; cmd_snapshot record "$@" ;;
  snapshot:report) shift; cmd_snapshot report "$@" ;;
  targets)   cmd_targets ;;
  cache)     shift; cmd_cache "$@" ;;
  ci)        shift; cmd_ci "$@" ;;
  ci-status) shift; gh run list --workflow ci.yml "$@" ;;
  help|--help|-h) cmd_help ;;
  full)      cmd_full ;;
  gate)      shift; cmd_gate "$@" ;;
  regen)     shift; cmd_regen "$@" ;;
  warm)      shift; cmd_warm "$@" ;;
  smoke)     cmd_smoke ;;
  data)      cmd_data ;;
  versions)  cmd_versions ;;
  diff)      cmd_diff "$@" ;;
  lua)       cmd_lua "$@" ;;
  status)    cmd_status ;;
  *) cmd_help; exit 2 ;;
esac
