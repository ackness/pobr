#!/usr/bin/env bash
# driver.sh — pobr 环境 bootstrap + 烟雾验证 + PoB2 vendor 参考克隆。
#
# 把"新环境从零到能跑测试 + 能查 PoB2 公式"的流程脚本化（会话 2026-06-18 实跑得出）。
# 子命令：
#   bootstrap  安装 luajit/构建依赖 + 克隆 vendor（钉定 commit）+ workspace build
#   deps       仅安装系统依赖（luajit）
#   vendor     仅克隆/对齐 PoB2 vendor 到钉定 commit（gitignored，永不提交）
#   build      cargo build --workspace
#   test       定向测试：pobr-core + pobr-build（含 parity_no_regression 门禁）
#   drill      version-bump-drill：数据可再生性 + 编译 + parity 可运行
#   smoke      test + drill（最常用的"改完一遍过"验证）
#   data       数据现状自检 + 再生管线说明（含云端约束）
#   lua <pat>  在 vendor PoB2 Lua 里 grep 一个 pattern（公式/规则裁决用）
#   status     报告 luajit / vendor / data 就绪态
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

DATA_VER="$(cat data/CURRENT 2>/dev/null | tr -d '[:space:]' || echo 4.5.0.3.4)"
RULES="data/$DATA_VER/overlay/mod_parser_rules.json"
VENDOR_DIR="vendor/PathOfBuilding-PoE2"
VENDOR_REPO="https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2.git"

say() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }

vendor_commit() {
  python3 -c "import json;print(json.load(open('$RULES'))['_meta']['vendor_commit'])" 2>/dev/null
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
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ] \
     && [ "$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)" = "$sha" ]; then
    echo "vendor 已在钉定 commit $sha"; return 0
  fi
  echo "克隆 PoB2 @ $sha（depth 1 fetch-by-sha）..."
  mkdir -p "$VENDOR_DIR"
  git -C "$VENDOR_DIR" rev-parse --git-dir >/dev/null 2>&1 || git -C "$VENDOR_DIR" init -q
  git -C "$VENDOR_DIR" remote get-url origin >/dev/null 2>&1 \
    || git -C "$VENDOR_DIR" remote add origin "$VENDOR_REPO"
  git -C "$VENDOR_DIR" fetch -q --depth 1 origin "$sha"
  git -C "$VENDOR_DIR" checkout -q FETCH_HEAD
  printf '%s\n' "$sha" > vendor/.pob2-version.txt
  echo "OK: $(git -C "$VENDOR_DIR" log -1 --oneline)"
}

cmd_build() { say "cargo build --workspace"; cargo build --workspace --quiet && echo "build OK"; }

cmd_test() {
  say "定向测试 pobr-core + pobr-build（含 parity 门禁）"
  cargo test -p pobr-core -p pobr-build 2>&1 \
    | grep -E "test result:|FAILED|error\[|error:|Running " | grep -vE "^\s*$"
}

cmd_drill() { say "version-bump-drill（数据可再生 + 编译 + parity 可运行）"; bash devs/scripts/version-bump-drill.sh; }

cmd_smoke() { cmd_build && cmd_test; }   # 核心绿信号；版本升级再生检查走 `drill`

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
  - extract-lua（步骤 3）当前与 vendor commit 2df5a74 不兼容（modLib.parseMod 非全局）；
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
  echo "活动默认（data/CURRENT / pobr_data::DATA_VERSION）：$DATA_VER（= golden 校验版本）"
  echo "切到更新版本运行（零代码改动）：export POBR_DATA_VERSION=<ver>  或  写 data/CURRENT"
  echo "→ multi_version smoke（对每个版本 BuildData::load + calc）："
  cargo test -p pobr-build --test parity multi_version -- --nocapture 2>&1 \
    | grep -E "multi-version\]|test result:"
}

cmd_status() {
  say "就绪态"
  command -v luajit >/dev/null && echo "✓ luajit: $(luajit -v 2>&1 | head -1)" || echo "✗ luajit 未装（driver.sh deps）"
  local sha; sha="$(vendor_commit)"
  if [ -f "$VENDOR_DIR/src/Modules/ModParser.lua" ]; then
    local have; have="$(git -C "$VENDOR_DIR" rev-parse HEAD 2>/dev/null)"
    [ "$have" = "$sha" ] && echo "✓ vendor: 钉定 commit $sha" || echo "~ vendor: 在 $have（钉定 $sha，driver.sh vendor 对齐）"
  else echo "✗ vendor 未克隆（driver.sh vendor）"; fi
  [ -d "data/$DATA_VER" ] && echo "✓ data: $DATA_VER 已入库" || echo "✗ data: $DATA_VER 缺失"
  command -v cargo >/dev/null && echo "✓ cargo: $(cargo --version)" || echo "✗ cargo 缺失"
  echo "注：oracle（tools/pob2-oracle）+ extract-lua 与 vendor 2df5a74 不兼容（见 SKILL.md Gotchas）。"
}

case "${1:-smoke}" in
  deps)      cmd_deps ;;
  vendor)    cmd_vendor ;;
  bootstrap) cmd_deps && cmd_vendor && cmd_build ;;
  build)     cmd_build ;;
  test)      cmd_test ;;
  drill)     cmd_drill ;;
  smoke)     cmd_smoke ;;
  data)      cmd_data ;;
  versions)  cmd_versions ;;
  lua)       cmd_lua "$@" ;;
  status)    cmd_status ;;
  *) echo "usage: driver.sh {bootstrap|deps|vendor|build|test|drill|smoke|data|versions|lua <pat>|status}"; exit 2 ;;
esac
