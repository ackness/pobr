#!/usr/bin/env bash
# 已准备数据上词条规则预编译的检查（CI 数据防线）
#
# 在临时目录重跑 precompile-mods --report，不写入或恢复工作区文件；解析覆盖率
# 不得低于 devs/ci/parse-coverage-baseline.json（棘轮）。
# 游戏事实、keystone 派生规则与语言数据来自 owner 发布，由 data:prepare 按摘要校验；
# keystone 规则与天赋树的一致性由 RePoE 的 mise run adapter:keystones 检查。
# 缺少已准备数据时失败：先运行 mise run data:prepare。
#
# 用法：
#   devs/scripts/regen-check.sh                      # 版本取 data/CURRENT
#   POBR_PATCH=0.5.5.5 devs/scripts/regen-check.sh   # 显式指定版本

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

PATCH="${POBR_PATCH:-}"
if [[ -z "$PATCH" && -f "$ROOT/data/CURRENT" ]]; then
    PATCH="$(head -1 "$ROOT/data/CURRENT" | tr -d '[:space:]')"
fi
if [[ -z "$PATCH" ]]; then
    echo "regen-check: 无法确定版本（缺 data/CURRENT，也未设 POBR_PATCH）" >&2
    exit 1
fi

COMMITTED_FLAT="$ROOT/data/$PATCH"
if [[ ! -d "$COMMITTED_FLAT" ]]; then
    echo "regen-check: data/$PATCH 不存在；先运行 mise run data:prepare" >&2
    exit 1
fi

TMP_OUT="$(mktemp -d "${TMPDIR:-/tmp}/pobr-regen-check.XXXXXX")"
trap 'rm -rf "$TMP_OUT"' EXIT
echo "regen-check: patch=${PATCH}，输出临时目录 ${TMP_OUT}"

STATUS=0

# ============================================================================
# 词条规则预编译与覆盖率棘轮
# ----------------------------------------------------------------------------
# precompile-mods 写 <data>/generated/{parsed_mods.json,parse-coverage.json}。
# 复制已准备数据到隔离目录后运行，避免改动工作区；dereference symlink，
# 输出不能指回源目录。语料来自 examples/。
# ============================================================================
BASELINE="$ROOT/devs/ci/parse-coverage-baseline.json"
echo "regen-check: 重跑 precompile-mods（词条规则预编译）……"
PC_ROOT="$TMP_OUT/precompile"
PC_DATA="$PC_ROOT/data/$PATCH"
mkdir -p "$PC_DATA"
cp -RL "$COMMITTED_FLAT/." "$PC_DATA/"
if [[ -d "$ROOT/data/overlay-common" ]]; then
    cp -RL "$ROOT/data/overlay-common" "$PC_ROOT/data/overlay-common"
fi
[[ ! -d "$ROOT/examples" ]] || ln -s "$ROOT/examples" "$PC_ROOT/examples"

cargo run --quiet -p precompile-mods --manifest-path "$ROOT/Cargo.toml" -- \
    --data "$PC_DATA" --report >/dev/null

COVERAGE="$PC_DATA/generated/parse-coverage.json"
if [[ ! -f "$COVERAGE" || ! -f "$BASELINE" ]]; then
    echo "regen-check: 缺 parse-coverage.json 或覆盖率基线，无法检查棘轮" >&2
    exit 1
fi
CUR_RATIO="$(sed -n 's/.*"coverage_ratio"[[:space:]]*:[[:space:]]*\([0-9.]*\).*/\1/p' "$COVERAGE" | head -1)"
BASE_RATIO="$(sed -n 's/.*"coverage_ratio"[[:space:]]*:[[:space:]]*\([0-9.]*\).*/\1/p' "$BASELINE" | head -1)"
if [[ -z "$CUR_RATIO" || -z "$BASE_RATIO" ]]; then
    echo "regen-check: 无法读取 coverage_ratio" >&2
    exit 1
fi
# 浮点比较走 awk。
if awk "BEGIN { exit !(${CUR_RATIO} + 0.0000005 < ${BASE_RATIO}) }"; then
    STATUS=1
    echo ""
    echo "regen-check: 覆盖率棘轮失败 —— 当前 ${CUR_RATIO} < 基线 ${BASE_RATIO}"
    echo "解析覆盖率不得降低；若属预期（语料扩面）请同 PR 更新 devs/ci/parse-coverage-baseline.json"
elif awk "BEGIN { exit !(${CUR_RATIO} > ${BASE_RATIO} + 0.0000005) }"; then
    echo "regen-check: 提示 —— 覆盖率 ${CUR_RATIO} 高于基线 ${BASE_RATIO}，可同 PR 抬高 baseline"
else
    echo "regen-check: OK — 覆盖率棘轮达标（当前 ${CUR_RATIO}，基线 ${BASE_RATIO}）"
fi

exit "$STATUS"
