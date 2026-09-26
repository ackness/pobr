#!/usr/bin/env bash
# driver.sh — pobr 环境 bootstrap + 烟雾验证 + PoB2 vendor 参考克隆。
#
# 把"新环境从零到能跑测试 + 能查 PoB2 公式"的流程脚本化（会话 2026-06-18 实跑得出）。
# 子命令：
#   bootstrap  安装 luajit/构建依赖 + 克隆 vendor（钉定 commit）+ CLI build
#   deps       仅安装系统依赖（luajit）
#   vendor     仅克隆/对齐 PoB2 vendor 到钉定 commit（gitignored，永不提交）
#   build      Build the application CLI, or forward explicit Cargo selectors.
#   test <args> Forward targeted cargo test arguments (package, suite, filter).
#   lint <args> Format check plus Clippy for explicitly selected Cargo targets.
#   full       fmt + clippy + all workspace tests + i18n lint
#   drill      version-bump-drill：数据可再生性 + 编译 + parity 可运行
#   smoke      Quick representative aggregation, parser and codec checks.
#   data       数据现状自检 + 再生管线说明（含云端约束）
#   lua <pat>  在 vendor PoB2 Lua 里 grep 一个 pattern（公式/规则裁决用）
#   status     报告 luajit / vendor / data 就绪态
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

DATA_VER="${POBR_DATA_VERSION:-$(cat data/CURRENT)}"
RULES="data/$DATA_VER/overlay/mod_parser_rules.json"
VENDOR_DIR="vendor/PathOfBuilding-PoE2"
VENDOR_REPO="https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2.git"

say() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }

vendor_commit() {
  # 路径经 argv（sys.argv[1]）传入，不插值进脚本串——避免 $RULES 含引号时的注入面。
  python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['_meta']['vendor_commit'])" "$RULES" 2>/dev/null
}

cmd_deps() {
  say "系统依赖（luajit）"
  if command -v luajit >/dev/null; then
    echo "luajit 已装：$(luajit -v 2>&1 | head -1)"
  else
    sudo apt-get update -qq && sudo apt-get install -y luajit
    luajit -v
  fi
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
    echo "usage: ./pobr <test|lint|timings> <crate> <suite|lib> [args]" >&2
    echo "Raw Cargo selectors also work: -p <crate> --test <suite>" >&2
    return 2
  fi
  if [[ "$1" == -* ]]; then CARGO_ARGS=("$@"); return 0; fi
  if [[ $# -lt 2 || "$2" == -* ]]; then
    echo "Select a suite explicitly; use './pobr targets' to list them." >&2
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
    echo "usage: ./pobr verify <crate> <suite|lib> [filter]" >&2
    return 2
  fi
  cmd_test "$@" && cmd_lint "$1" "$2"
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

cmd_ci() {
  if [[ $# -ne 1 || "$1" == -* ]]; then
    echo "usage: ./pobr ci <pushed-branch-or-tag>" >&2
    echo "Runs the remote ref, not uncommitted or unpushed local changes." >&2
    return 2
  fi
  gh workflow run ci.yml --ref "$1" &&
    echo "CI requested for remote ref '$1'. Check './pobr ci-status'; this is not a passing result."
}

cmd_help() {
  cat <<'EOF'
PoBR development commands (run from any directory via this script's path):
  ./pobr targets                          List Rust suites without compiling
  ./pobr verify build skills [filter]      Targeted test + fmt + Clippy
  ./pobr test core parser [filter]         Test one suite (or use 'lib')
  ./pobr lint build skills                fmt + targeted Clippy
  ./pobr timings build parity             Compile only, with HTML timings
  ./pobr build [-p <crate>]               Build application CLI by default
  ./pobr smoke                            Representative environment checks
  ./pobr full                             Explicit full local Rust gate
  ./pobr ci <pushed-branch-or-tag>         Request full cloud CI via gh
  ./pobr ci-status [--branch <branch>]     List cloud CI results via gh

test/lint/timings also accept raw Cargo arguments (-p, --test, --lib, ...).
Use --test/--lib to bound compilation; a test-name filter alone cannot do that.
Full release CI runs on tags. Do not run an identical full gate locally first.
Data/setup: status, deps, vendor, bootstrap, data, versions, diff, drill, lua.
EOF
}

cmd_workspace_tests() {
  if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run --workspace && cargo test --workspace --doc
  else
    say "nextest unavailable; using cargo test (including doctests)"
    cargo test --workspace
  fi
}

cmd_full() {
  say "Full workspace gate: fmt, clippy, tests (including doctests), i18n"
  cargo fmt --all --check &&
    cargo clippy --workspace --all-targets -- -D warnings &&
    cmd_workspace_tests &&
    cargo run -p lint-i18n
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
  if [ -d "data/$DATA_VER" ]; then
    echo "✓ data/$DATA_VER 已入库（base/overlay/generated/i18n）——测试无需联网下载"
    ls "data/$DATA_VER" | sed 's/^/    /'
  else
    echo "✗ data/$DATA_VER 缺失"
  fi
  cat <<'EOF'

数据再生管线（仅版本升级时需要；详见 pipeline/README.md + pipeline/regen-all.sh）：
  1) GGG .dat 下载    : cd pipeline && node download-index.mjs && npx -y pathofexile-dat@15
  2) 适配 → base JSON : cargo run -p pobr-data-adapter -- --raw pipeline/tables --out data --patch <ver>
  3) vendor Lua → overlay : pipeline/regen-all.sh（需 luajit + vendor 检出）
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
  echo "data/ 版本目录："; ls -d data/[0-9]*/ 2>/dev/null | sed 's#data/##; s#/##' | sed 's/^/    /'
  echo "本次数据版本：${DATA_VER}（golden 独立固定，不随活动版本推进）"
  echo "切到更新版本运行（零代码改动）：export POBR_DATA_VERSION=<ver>  或  写 data/CURRENT"
  echo "→ multi_version smoke（对每个版本 BuildData::load + calc）："
  cargo test -p pobr-build --test parity multi_version -- --nocapture 2>&1 \
    | grep -E "multi-version\]|test result:"
}

cmd_diff() {
  local a="${2:-}" b="${3:-}"
  if [ -z "$a" ] || [ -z "$b" ]; then
    echo "usage: driver.sh diff <verA> <verB> [--domain tree|mods|bases|skill_levels|skill_stats|special_mods|uniques] [--limit N]"
    echo "已入库版本："; ls -d data/[0-9]*/ 2>/dev/null | sed 's#data/##; s#/##; s/^/    /'
    return 2
  fi
  say "语义数据 diff: $a → ${b}（节点/技能/词条的增删改）"
  python3 pipeline/diff-data.py "data/$a" "data/$b" --semantic "${@:4}"
}

cmd_status() {
  say "就绪态"
  command -v luajit >/dev/null && echo "✓ luajit: $(luajit -v 2>&1 | head -1)" || echo "✗ luajit 未装（driver.sh deps）"
  local sha; sha="$(vendor_commit)"
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ]; then
    local have; have="$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)"
    [ "$have" = "$sha" ] && echo "✓ vendor: 钉定 commit $sha" || echo "~ vendor: 在 ${have}（钉定 ${sha}，driver.sh vendor 对齐）"
  else echo "✗ vendor 未克隆（driver.sh vendor）"; fi
  [ -d "data/$DATA_VER" ] && echo "✓ data: $DATA_VER 已入库" || echo "✗ data: $DATA_VER 缺失"
  command -v cargo >/dev/null && echo "✓ cargo: $(cargo --version)" || echo "✗ cargo 缺失"
  echo "注：oracle（tools/pob2-oracle）+ extract-lua 与旧版 4.5.0.3.4 的 vendor（2df5a74）不兼容（见 SKILL.md Gotchas）。"
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
  targets)   cmd_targets ;;
  ci)        shift; cmd_ci "$@" ;;
  ci-status) shift; gh run list --workflow ci.yml "$@" ;;
  help|--help|-h) cmd_help ;;
  full)      cmd_full ;;
  drill)     cmd_drill ;;
  smoke)     cmd_smoke ;;
  data)      cmd_data ;;
  versions)  cmd_versions ;;
  diff)      cmd_diff "$@" ;;
  lua)       cmd_lua "$@" ;;
  status)    cmd_status ;;
  *) cmd_help; exit 2 ;;
esac
