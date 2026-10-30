#!/usr/bin/env bash
# driver.sh — pobr 环境 bootstrap + 烟雾验证 + PoB2 vendor 参考克隆。
#
# 把"新环境从零到能跑测试 + 能查 PoB2 公式"的流程脚本化（会话 2026-06-18 实跑得出）。
# 子命令：
#   bootstrap  安装 luajit/构建依赖 + 克隆 vendor（钉定 commit）+ CLI build
#   deps       Install/cache the compatible PoB2 LuaJIT runtime
#   vendor     仅克隆/对齐 PoB2 vendor 到钉定 commit（gitignored，永不提交）
#   build      Build the application CLI, or forward explicit Cargo selectors.
#   test <args> Forward targeted cargo test arguments (package, suite, filter).
#   lint <args> Format check plus Clippy for explicitly selected Cargo targets.
#   full       fmt + clippy + all workspace tests + i18n lint
#   gate       batch gate after merging several changes; reports every failing step
#   regen      precompile-mods --report, then its tests
#   warm       copy-on-write clone of another worktree's target/debug
#   drill      version-bump-drill：数据可再生性 + 编译 + parity 可运行
#   smoke      Quick representative aggregation, parser and codec checks.
#   data       数据现状自检 + 再生管线说明（含云端约束）
#   lua <pat>  在 vendor PoB2 Lua 里 grep 一个 pattern（公式/规则裁决用）
#   status     报告 luajit / vendor / data 就绪态
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

DATA_ROOT="${POBR_DATA_ROOT:-$ROOT/data}"
DATA_VER="${POBR_DATA_VERSION:-$(head -n 1 "$DATA_ROOT/CURRENT")}"
RULES="$DATA_ROOT/$DATA_VER/overlay/mod_parser_rules.json"
VENDOR_DIR="vendor/PathOfBuilding-PoE2"
VENDOR_REPO="https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2.git"

say() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }

vendor_commit() {
  # 路径经 argv（sys.argv[1]）传入，不插值进脚本串——避免 $RULES 含引号时的注入面。
  python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['_meta']['vendor_commit'])" "$RULES" 2>/dev/null
}

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
  say "PoB2 vendor 参考克隆（gitignored，不提交）"
  local sha; sha="$(vendor_commit)"
  [ -n "$sha" ] || { echo "ERROR: 无法从 $RULES 读 vendor_commit"; return 1; }
  mkdir -p "$VENDOR_DIR" || return 1
  local vendor_root
  vendor_root="$(git -C "$VENDOR_DIR" rev-parse --show-toplevel 2>/dev/null)" || vendor_root=""
  if [[ -n "$vendor_root" && "$(cd "$vendor_root" && pwd -P)" != "$(cd "$VENDOR_DIR" && pwd -P)" ]]; then
    vendor_root=""
  fi
  if [[ -z "$vendor_root" ]]; then
    [[ -z "$(ls -A "$VENDOR_DIR")" ]] || { echo "ERROR: vendor 目录非空且不是独立 Git 仓库" >&2; return 1; }
    git -C "$VENDOR_DIR" init -q || return 1
  fi
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ] \
     && [ "$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)" = "$sha" ]; then
    echo "vendor 已在钉定 commit $sha"; return 0
  fi
  local vendor_changes
  vendor_changes="$(git -C "$VENDOR_DIR" status --porcelain --untracked-files=no)" || return 1
  [[ -z "$vendor_changes" ]] \
    || { echo "ERROR: vendor 有未提交修改，拒绝切换 commit" >&2; return 1; }
  echo "克隆 PoB2 @ ${sha}（depth 1 fetch-by-sha）..."
  git -C "$VENDOR_DIR" remote get-url origin >/dev/null 2>&1 \
    || git -C "$VENDOR_DIR" remote add origin "$VENDOR_REPO" || return 1
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
  if [[ -e "$target" && "$target" != /* ]]; then target="$PWD/$target"; fi
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
  if [[ -e "$target" && "$target" != /* ]]; then target="$PWD/$target"; fi
  python3 devs/scripts/compare-build.py "$target" "$@"
}

# Every worked build (the benchmark and tuned sets by default) against its
# reference and the pinned PoB2 at once: which differing rows are the
# engine's, and what explains them across builds.
cmd_triage() {
  local args=() arg
  for arg in "$@"; do
    if [[ -e "$arg" && "$arg" != /* ]]; then arg="$PWD/$arg"; fi
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
  if [[ -n "$corpus" && "$corpus" != /* ]]; then corpus="$PWD/$corpus"; fi
  ${corpus:+env POBR_REFERENCE_BUILDS="$corpus"} cargo test -q -p pobr-build --test parity \
    reference_report::reference_report -- --ignored --nocapture
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
  if [[ $# -lt 1 || $# -gt 2 || "$1" == -* || ( $# -eq 2 && "$2" != "--e2e" ) ]]; then
    echo "usage: mise run ci <pushed-branch-or-tag> [--e2e]" >&2
    echo "Runs the remote ref, not uncommitted or unpushed local changes." >&2
    echo "E2E is skipped unless --e2e is given." >&2
    return 2
  fi
  local extra=()
  if [[ $# -eq 2 ]]; then extra=(-f e2e=true); fi
  gh workflow run ci.yml --ref "$1" ${extra[@]+"${extra[@]}"} &&
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
  mise run builds:fetch <set dir> [--pick first|last]  One poe.ninja build per ascendancy into a reference set
  mise run build [-p <crate>]               Build application CLI by default
  mise run smoke                            Representative environment checks
  mise run full                             Explicit full local Rust gate
  mise run gate [--only step,...]           Batch gate after merging: every step, then a summary
  mise run regen                            Regenerate parser artifacts and test precompile-mods
  mise run warm [source worktree]           Copy-on-write clone of another worktree's target/debug
  mise run ci <pushed-branch-or-tag> [--e2e] Request full cloud CI via gh (E2E only with --e2e)
  mise run ci-status [--branch <branch>]     List cloud CI results via gh

test/lint/timings also accept raw Cargo arguments (-p, --test, --lib, ...).
Use --test/--lib to bound compilation; a test-name filter alone cannot do that.
Full release CI runs on tags. Do not run an identical full gate locally first.
Data/setup tasks: status, deps, vendor, bootstrap, data:status, versions, data:diff, drill, vendor:lua.
Script subcommands for the renamed tasks: data, diff, lua.
EOF
}

cmd_full() {
  say "Full workspace gate: fmt, clippy, tests (including doctests), i18n"
  cargo fmt --all --check &&
    cargo clippy --workspace --all-targets --features pobr-core/benchmarks,pobr-build/benchmarks,pobr-wasm/diagnostics -- -D warnings &&
    cargo test --workspace &&
    cargo run -p lint-i18n
}

# Regenerate the parser artifacts after a rule change, then test the generator.
cmd_regen() {
  if [[ $# -gt 0 ]]; then echo "usage: mise run regen" >&2; return 2; fi
  say "precompile-mods --report (data/$DATA_VER)"
  cargo run -q -p precompile-mods -- --data "$DATA_ROOT/$DATA_VER" --report &&
    cargo test -q -p precompile-mods
}

# A new worktree starts with an empty target and rebuilds every dependency.
# Clone another worktree's debug cache copy-on-write (APFS clonefile, or a Linux
# reflink); Cargo then rebuilds only the workspace crates whose paths differ.
cmd_warm() {
  if [[ $# -gt 1 || "${1:-}" == -* ]]; then
    echo "usage: mise run warm [source worktree; default: the repository's main worktree]" >&2
    return 2
  fi
  local source="${1:-}"
  if [[ -z "$source" ]]; then
    source="$(git worktree list --porcelain 2>/dev/null | sed -n '1s/^worktree //p')"
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
  say "Copy-on-write clone of $source/target/debug"
  if cp -cR "$source/target/debug" target/ 2>/dev/null ||
     cp -R --reflink=always "$source/target/debug" target/ 2>/dev/null; then
    echo "OK: target/debug cloned; Cargo reuses unchanged dependencies"
  else
    rm -rf target/debug
    echo "ERROR: this filesystem has no copy-on-write clone; build normally" >&2
    return 1
  fi
}

# The batch gate a coordinator runs once after merging several changes: every
# step runs even after a failure, so one pass shows all failures, and
# `--only <step,...>` reruns just the failed ones.
GATE_STEPS=(fmt regen clippy core build parity apps i18n web accuracy)

gate_step() {
  case "$1" in
    fmt)      cargo fmt --all --check ;;
    regen)    cmd_regen && git diff --exit-code --stat -- "$DATA_ROOT/$DATA_VER/generated" ;;
    clippy)   cargo clippy --workspace --lib --tests -- -D warnings ;;
    core)     cargo test -q -p pobr-core ;;
    build)    cargo test -q -p pobr-build --lib --test skills --test codec --test dualrun --test tree_convert ;;
    parity)   cargo test -q -p pobr-build --test parity ;;
    apps)     cargo test -q -p pobr-data -p pobr-gamedata -p pobr-i18n -p pobr-tree -p pobr-item \
                -p pobr-wasm -p pobr-cli ;;
    i18n)     cargo run -q -p lint-i18n ;;
    web)      pnpm --dir web typecheck && pnpm --dir web test ;;
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

cmd_drill() { say "version-bump-drill（数据可再生 + 编译 + parity 可运行）"; bash devs/scripts/version-bump-drill.sh; }

cmd_smoke() {
  say "Quick smoke: aggregation, parser and Build Code roundtrip"
  cargo test -p pobr-core --test aggregation mod_db:: &&
    cargo test -p pobr-core --test parser mod_parser:: &&
    cargo test -p pobr-build --test codec build_code::
}

cmd_data() {
  say "数据现状自检"
  if [ -d "$DATA_ROOT/$DATA_VER" ]; then
    echo "✓ data/$DATA_VER 已入库（base/overlay/generated/i18n）——测试无需联网下载"
    ls "$DATA_ROOT/$DATA_VER" | sed 's/^/    /'
  else
    echo "✗ data/$DATA_VER 缺失"
  fi
  cat <<'EOF'

数据再生管线（仅版本升级时需要；详见 pipeline/README.md + pipeline/regen-all.sh）：
  1) GGG .dat 下载    : (cd pipeline && node download-index.mjs) && bash pipeline/export-tables.sh
  2) 仅刷新 base JSON : mise run data:adapt（候选目录生成和校验）
  3) 完整再生成       : bash pipeline/regen-all.sh（需 luajit + vendor 检出，校验后发布）
  4) precompile 缓存  : cargo run -p precompile-mods -- --data data/<ver> --report
云端约束（本会话实测）：
  - GGG patch CDN 对旧 pin 版本（4.5.0.3.4）返回 404 → .dat 下载步不可跑；
  - extract-lua（步骤 3）与旧版 4.5.0.3.4 的 vendor commit（2df5a74）不兼容（modLib.parseMod 非全局）；
  - 故云端只用"已入库 data + precompile（步骤 4，可跑且 byte-diff=0，见 drill 步骤 4）"。
EOF
}

cmd_lua() {
  local pat="${2:-}"
  [ -n "$pat" ] || { echo "usage: driver.sh lua '<pattern>'  （在 vendor PoB2 Lua 里 grep）"; return 2; }
  [ -d "$VENDOR_DIR/src" ] || { echo "vendor 未克隆，先跑：driver.sh vendor"; return 1; }
  say "grep vendor PoB2 Lua: $pat"
  grep -rnF -- "$pat" "$VENDOR_DIR/src/Modules/ModParser.lua" \
    "$VENDOR_DIR/src/Modules/CalcOffence.lua" \
    "$VENDOR_DIR/src/Modules/CalcDefence.lua" \
    "$VENDOR_DIR/src/Modules/CalcPerform.lua" 2>/dev/null | head -30
}

cmd_versions() {
  say "已入库数据版本 + 多版本无关性 smoke"
  echo "data/ 版本目录："; ls -d "$DATA_ROOT"/[0-9]*/ 2>/dev/null | sed 's#/$##; s#.*/##' | sed 's/^/    /'
  echo "本次数据版本：${DATA_VER}（测试与 PoB2 对照值都针对它）"
  echo "切到更新版本运行（零代码改动）：export POBR_DATA_VERSION=<ver>  或  写 data/CURRENT"
  echo "→ multi_version smoke（对每个版本 BuildData::load + calc）："
  cargo test -p pobr-build --test parity multi_version -- --nocapture 2>&1 \
    | grep -E "multi-version\]|test result:"
}

cmd_diff() {
  local a="${2:-}" b="${3:-}"
  if [ -z "$a" ] || [ -z "$b" ]; then
    echo "usage: driver.sh diff <verA> <verB> [--domain tree|mods|bases|skill_levels|skill_stats|special_mods|uniques] [--limit N]"
    echo "已入库版本："; ls -d "$DATA_ROOT"/[0-9]*/ 2>/dev/null | sed 's#/$##; s#.*/##; s/^/    /'
    return 2
  fi
  say "语义数据 diff: $a → ${b}（节点/技能/词条的增删改）"
  python3 pipeline/diff-data.py "$DATA_ROOT/$a" "$DATA_ROOT/$b" --semantic "${@:4}"
}

cmd_status() {
  say "就绪态"
  local runtime
  if runtime="$(bash "$ROOT/devs/scripts/pob2-luajit.sh" resolve)"; then
    echo "✓ PoB2 LuaJIT: $runtime"
  else
    echo "✗ compatible PoB2 LuaJIT unavailable (mise run deps)"
  fi
  local sha; sha="$(vendor_commit)"
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ]; then
    local have; have="$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)"
    [ "$have" = "$sha" ] && echo "✓ vendor: 钉定 commit $sha" || echo "~ vendor: 在 ${have}（钉定 ${sha}，driver.sh vendor 对齐）"
  else echo "✗ vendor 未克隆（driver.sh vendor）"; fi
  [ -d "$DATA_ROOT/$DATA_VER" ] && echo "✓ data: $DATA_VER 已入库" || echo "✗ data: $DATA_VER 缺失"
  command -v cargo >/dev/null && echo "✓ cargo: $(cargo --version)" || echo "✗ cargo 缺失"
}

case "${1:-}" in
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
  targets)   cmd_targets ;;
  cache)     shift; cmd_cache "$@" ;;
  ci)        shift; cmd_ci "$@" ;;
  ci-status) shift; gh run list --workflow ci.yml "$@" ;;
  help|--help|-h) cmd_help ;;
  full)      cmd_full ;;
  gate)      shift; cmd_gate "$@" ;;
  regen)     shift; cmd_regen "$@" ;;
  warm)      shift; cmd_warm "$@" ;;
  drill)     cmd_drill ;;
  smoke)     cmd_smoke ;;
  data)      cmd_data ;;
  versions)  cmd_versions ;;
  diff)      cmd_diff "$@" ;;
  lua)       cmd_lua "$@" ;;
  status)    cmd_status ;;
  *) cmd_help; exit 2 ;;
esac
