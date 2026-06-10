#!/usr/bin/env bash
# 可再生性检查（CI 数据防线 ①，见 devs/docs/architecture/06-development-workflow.md §5.1）
#
# 用 pipeline/ 已有的本地输入重跑 tools/pobr-data-adapter，产物落到临时目录，
# 与仓库已提交的 data/<patch>/ 入库 JSON 逐文件 byte-diff：
#   - 任一文件不一致（或重生成产物在仓库中找不到对应文件）→ 退出码 1，并列出差异文件；
#   - 本地缺少 pipeline 输入（tables/ 与 tree/ 都没有）→ 明确报 SKIP，退出码 0（不是失败）；
#   - 全部一致 → 退出码 0。
#
# 布局兼容：已提交产物按存在性探测，优先 data/<patch>/base/<相对路径>（W1 之后的三层
# 目录新布局），否则回退 data/<patch>/<相对路径>（旧平铺布局）。
#
# 已知排除：manifest.json 暂不参与 byte-diff——当前 manifest 是多个域步骤（--raw / --tree /
# 手工合并）的合并产物，单独重跑 --raw 得到的 manifest 域列表不完整；待主线 W1 的
# manifest v2（三段 domains）落地、由单一步骤幂等生成后，再纳入 diff（见 §5.1 收紧计划）。
#
# 用法：
#   devs/scripts/regen-check.sh            # 自动从 pipeline/config.json 读 patch 版本
#   POBR_PATCH=4.5.0.3.4 devs/scripts/regen-check.sh   # 显式指定版本

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# ---- 版本号：环境变量优先，否则从 pipeline/config.json 的 "patch" 字段读取 ----
PATCH="${POBR_PATCH:-}"
if [[ -z "$PATCH" ]]; then
    PATCH="$(sed -n 's/.*"patch"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
        "$ROOT/pipeline/config.json" | head -1)"
fi
if [[ -z "$PATCH" ]]; then
    echo "regen-check: 无法确定 patch 版本（pipeline/config.json 缺 \"patch\"，也未设 POBR_PATCH）" >&2
    exit 1
fi

# ---- 输入探测：两个域各自独立，缺哪个就跳过哪个 ----
TABLES_DIR="$ROOT/pipeline/tables"
TREE_JSON="$ROOT/pipeline/tree/data.json"

HAVE_TABLES=0
if [[ -f "$TABLES_DIR/English/BaseItemTypes.json" \
   && -f "$TABLES_DIR/Traditional Chinese/BaseItemTypes.json" ]]; then
    HAVE_TABLES=1
fi
HAVE_TREE=0
if [[ -f "$TREE_JSON" ]]; then
    HAVE_TREE=1
fi

if [[ "$HAVE_TABLES" -eq 0 && "$HAVE_TREE" -eq 0 ]]; then
    echo "regen-check: SKIP — 本地没有 pipeline 输入（既无 pipeline/tables/ 的 .dat 导出，"
    echo "也无 pipeline/tree/data.json）。重生成需要先按 pipeline/README.md 下载输入。"
    exit 0
fi

# ---- 已提交产物根目录（兼容新旧布局）----
COMMITTED_FLAT="$ROOT/data/$PATCH"
COMMITTED_BASE="$ROOT/data/$PATCH/base"
if [[ ! -d "$COMMITTED_FLAT" ]]; then
    echo "regen-check: 仓库中不存在 data/$PATCH/ —— patch 版本与已提交数据不匹配？" >&2
    exit 1
fi

# 按存在性解析某个重生成文件对应的已提交文件路径；找不到则输出空串。
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

# ---- 重生成到临时目录 ----
TMP_OUT="$(mktemp -d "${TMPDIR:-/tmp}/pobr-regen-check.XXXXXX")"
trap 'rm -rf "$TMP_OUT"' EXIT

echo "regen-check: patch=${PATCH}，输出临时目录 ${TMP_OUT}"

if [[ "$HAVE_TABLES" -eq 1 ]]; then
    echo "regen-check: 重跑物品/词缀/技能域（--raw pipeline/tables）……"
    cargo run --quiet -p pobr-data-adapter --manifest-path "$ROOT/Cargo.toml" -- \
        --raw "$TABLES_DIR" --out "$TMP_OUT" --patch "$PATCH"
else
    echo "regen-check: SKIP 物品/词缀/技能域 —— 缺 pipeline/tables/{English,Traditional Chinese}/"
fi

if [[ "$HAVE_TREE" -eq 1 ]]; then
    echo "regen-check: 重跑被动天赋树域（--tree pipeline/tree/data.json）……"
    cargo run --quiet -p pobr-data-adapter --manifest-path "$ROOT/Cargo.toml" -- \
        --tree "$TREE_JSON" --out "$TMP_OUT" --patch "$PATCH"
else
    echo "regen-check: SKIP 被动天赋树域 —— 缺 pipeline/tree/data.json"
fi

# ---- byte-diff：只比对本次实际重生成出来的文件 ----
declare -a DIFF_FILES=()
declare -a MISSING_FILES=()
CHECKED=0

while IFS= read -r regen; do
    rel="${regen#"$TMP_OUT/$PATCH/"}"
    # manifest.json 暂排除（理由见文件头注释）
    if [[ "$rel" == "manifest.json" ]]; then
        continue
    fi
    committed="$(resolve_committed "$rel")"
    if [[ -z "$committed" ]]; then
        MISSING_FILES+=("$rel")
        continue
    fi
    CHECKED=$((CHECKED + 1))
    if ! cmp -s "$regen" "$committed"; then
        DIFF_FILES+=("$rel")
    fi
done < <(find "$TMP_OUT/$PATCH" -type f -name '*.json' | sort)

if [[ "$CHECKED" -eq 0 && "${#MISSING_FILES[@]}" -eq 0 ]]; then
    echo "regen-check: 重生成没有产出任何文件 —— adapter 行为异常" >&2
    exit 1
fi

STATUS=0
if [[ "${#MISSING_FILES[@]}" -gt 0 ]]; then
    STATUS=1
    echo ""
    echo "regen-check: 以下重生成文件在仓库 data/$PATCH/{base/,}** 中找不到对应已提交文件："
    printf '  - %s\n' "${MISSING_FILES[@]}"
fi
if [[ "${#DIFF_FILES[@]}" -gt 0 ]]; then
    STATUS=1
    echo ""
    echo "regen-check: 以下文件 byte-diff 不为零（已提交数据与重生成产物漂移）："
    printf '  - %s\n' "${DIFF_FILES[@]}"
    echo ""
    echo "处理方式：若漂移来自 adapter 行为变更，重跑 pipeline/README.md 第 3 步并提交新产物；"
    echo "若来自手改 data/，撤销手改（data/ 只能由 adapter 生成）。"
fi

if [[ "$STATUS" -eq 0 ]]; then
    echo "regen-check: OK — $CHECKED 个文件 byte-diff 全零（manifest.json 暂排除）。"
fi
exit "$STATUS"
