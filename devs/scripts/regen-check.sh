#!/usr/bin/env bash
# 已准备数据上 PoBR 自有派生步骤的可再生性检查（CI 数据防线）
#
# 在临时目录重跑 PoBR 自有的派生步骤，不写入或恢复工作区文件：
#   - keystone 派生 special 表：pobr-data-adapter --emit-special-derived base/passive_tree.json，
#     与已准备的 generated/special_derived.json 逐字节比较；
#   - 词条规则预编译：precompile-mods --report 必须成功，解析覆盖率不得低于
#     devs/ci/parse-coverage-baseline.json（棘轮）。
# 游戏事实与语言数据来自 owner 发布，由 data:prepare 按摘要校验，这里不重生成。
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
COMMITTED_BASE="$ROOT/data/$PATCH/base"
if [[ ! -d "$COMMITTED_FLAT" ]]; then
    echo "regen-check: data/$PATCH 不存在；先运行 mise run data:prepare" >&2
    exit 1
fi
COMMITTED_TREE="$COMMITTED_BASE/passive_tree.json"
if [[ ! -f "$COMMITTED_TREE" ]]; then
    echo "regen-check: 缺 data/$PATCH/base/passive_tree.json；先运行 mise run data:prepare" >&2
    exit 1
fi

# 按存在性解析某个重生成文件对应的已准备文件路径；找不到则输出空串。
resolve_committed() {
    local rel="$1"
    if [[ -f "$COMMITTED_BASE/$rel" ]]; then
        echo "$COMMITTED_BASE/$rel"
    elif [[ -f "$COMMITTED_FLAT/$rel" ]]; then
        echo "$COMMITTED_FLAT/$rel"
    else
        echo ""
    fi
}

TMP_OUT="$(mktemp -d "${TMPDIR:-/tmp}/pobr-regen-check.XXXXXX")"
trap 'rm -rf "$TMP_OUT"' EXIT
echo "regen-check: patch=${PATCH}，输出临时目录 ${TMP_OUT}"

echo "regen-check: 重跑 keystone 派生 special 表（--emit-special-derived passive_tree.json）……"
cargo run --quiet -p pobr-data-adapter --manifest-path "$ROOT/Cargo.toml" -- \
    --emit-special-derived "$COMMITTED_TREE" --out "$TMP_OUT" --patch "$PATCH"

declare -a DIFF_FILES=()
declare -a MISSING_FILES=()
CHECKED=0
while IFS= read -r regen; do
    rel="${regen#"$TMP_OUT/$PATCH/"}"
    [[ "$rel" == "manifest.json" ]] && continue
    committed="$(resolve_committed "$rel")"
    if [[ -z "$committed" ]]; then
        MISSING_FILES+=("$rel")
        continue
    fi
    CHECKED=$((CHECKED + 1))
    cmp -s "$regen" "$committed" || DIFF_FILES+=("$rel")
done < <(find "$TMP_OUT/$PATCH" -type f -name '*.json' 2>/dev/null | sort)

if [[ "$CHECKED" -eq 0 && "${#MISSING_FILES[@]}" -eq 0 ]]; then
    echo "regen-check: 重生成没有产出任何文件 —— adapter 行为异常" >&2
    exit 1
fi

STATUS=0
if [[ "${#MISSING_FILES[@]}" -gt 0 ]]; then
    STATUS=1
    echo ""
    echo "regen-check: 以下重生成文件在已准备的 data/$PATCH/{base/,}** 中找不到："
    printf '  - %s\n' "${MISSING_FILES[@]}"
fi
if [[ "${#DIFF_FILES[@]}" -gt 0 ]]; then
    STATUS=1
    echo ""
    echo "regen-check: 以下文件与已准备数据不一致："
    printf '  - %s\n' "${DIFF_FILES[@]}"
    echo "处理方式：派生规则变更时同步更新 owner 发布的数据；不要手改已准备的 data/。"
fi

# ============================================================================
# 词条规则预编译与覆盖率棘轮
# ----------------------------------------------------------------------------
# precompile-mods 写 <data>/generated/{parsed_mods.json,parse-coverage.json}。
# 复制已准备数据到隔离目录后运行，避免改动工作区；dereference symlink，
# 输出不能指回源目录。语料来自 examples/。
# ============================================================================
BASELINE="$ROOT/devs/ci/parse-coverage-baseline.json"
echo ""
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

if [[ "$STATUS" -eq 0 ]]; then
    echo "regen-check: OK — keystone 派生 $CHECKED 个文件逐字节一致，预编译与覆盖率通过。"
fi
exit "$STATUS"
