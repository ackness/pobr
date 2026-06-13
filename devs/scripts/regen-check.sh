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

# keystone 派生（M5b C-1）的输入是已提交的 passive_tree.json，无 pipeline 依赖——
# 仓库有 data/<patch>/ 即可重跑，故不计入「无 pipeline 输入」的整体 SKIP 判定。
HAVE_SPECIAL_DERIVED=0
if [[ -d "$ROOT/data/$PATCH" ]]; then
    HAVE_SPECIAL_DERIVED=1
fi

if [[ "$HAVE_TABLES" -eq 0 && "$HAVE_TREE" -eq 0 && "$HAVE_SPECIAL_DERIVED" -eq 0 ]]; then
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

# M5b C-1：keystone 派生 special 表（输入 = 已提交 passive_tree.json，无 pipeline 依赖）。
COMMITTED_TREE="$(resolve_committed "passive_tree.json")"
if [[ -n "$COMMITTED_TREE" ]]; then
    echo "regen-check: 重跑 keystone 派生 special 表（--emit-special-derived passive_tree.json）……"
    cargo run --quiet -p pobr-data-adapter --manifest-path "$ROOT/Cargo.toml" -- \
        --emit-special-derived "$COMMITTED_TREE" --out "$TMP_OUT" --patch "$PATCH"
else
    echo "regen-check: SKIP keystone 派生 —— 缺已提交 passive_tree.json"
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

# ============================================================================
# generated 段：precompile-mods 重生一致 + 覆盖率棘轮（M6-T7，架构 §4 防线 ③）
# ----------------------------------------------------------------------------
# precompile-mods 直接写 data/<patch>/generated/{parsed_mods.json,parse-coverage.json}。
# 校验方式：先把这两份已提交产物用 git 暂存到临时副本 → 原地重跑 precompile →
# byte-diff 重生产物 vs 已提交副本 → 用 git restore 还原工作区（CI 无副作用）。
# 输入 = 已提交 base/passive_tree.json + generated/special_derived.json + examples build XML，
# 无 pipeline 依赖；仓库有 data/<patch>/ 即可重跑。
# ============================================================================
GEN_DIR="$COMMITTED_FLAT/generated"
GEN_PARSED="$GEN_DIR/parsed_mods.json"
GEN_COVERAGE="$GEN_DIR/parse-coverage.json"
BASELINE="$ROOT/devs/ci/parse-coverage-baseline.json"

if [[ -f "$GEN_PARSED" ]]; then
    echo ""
    echo "regen-check: 重跑 precompile-mods（generated 段）……"

    # 已提交产物暂存到临时目录（byte 对照基准）。
    GEN_STASH="$(mktemp -d "${TMPDIR:-/tmp}/pobr-gen-stash.XXXXXX")"
    trap 'rm -rf "$TMP_OUT" "$GEN_STASH"' EXIT
    cp "$GEN_PARSED" "$GEN_STASH/parsed_mods.json"
    [[ -f "$GEN_COVERAGE" ]] && cp "$GEN_COVERAGE" "$GEN_STASH/parse-coverage.json"

    # 原地重跑（写回 generated/）。
    cargo run --quiet -p precompile-mods --manifest-path "$ROOT/Cargo.toml" -- \
        --data "$COMMITTED_FLAT" --report >/dev/null

    GEN_DIFF=0
    if ! cmp -s "$GEN_STASH/parsed_mods.json" "$GEN_PARSED"; then
        GEN_DIFF=1
        echo "regen-check: generated/parsed_mods.json byte-diff 不为零（precompile 漂移）。"
    fi
    if [[ -f "$GEN_STASH/parse-coverage.json" ]] \
       && ! cmp -s "$GEN_STASH/parse-coverage.json" "$GEN_COVERAGE"; then
        GEN_DIFF=1
        echo "regen-check: generated/parse-coverage.json byte-diff 不为零（precompile 漂移）。"
    fi

    # 还原工作区（不污染 git status）。重跑产物与已提交一致时本身就是 no-op。
    git -C "$ROOT" checkout -- "$GEN_PARSED" 2>/dev/null || true
    [[ -f "$GEN_COVERAGE" ]] && { git -C "$ROOT" checkout -- "$GEN_COVERAGE" 2>/dev/null || true; }
    rm -rf "$GEN_STASH"

    if [[ "$GEN_DIFF" -eq 1 ]]; then
        STATUS=1
        echo "处理方式：重跑 cargo run -p precompile-mods -- --data $COMMITTED_FLAT --report 并提交新产物；"
        echo "data/<patch>/generated/ 只能由工具生成，禁手改。"
    else
        echo "regen-check: OK — generated 段（parsed_mods/parse-coverage）byte-diff 全零。"
    fi

    # 覆盖率棘轮：已提交报表的 coverage_ratio 不得低于 baseline。
    if [[ -f "$BASELINE" && -f "$GEN_COVERAGE" ]]; then
        CUR_RATIO="$(sed -n 's/.*"coverage_ratio"[[:space:]]*:[[:space:]]*\([0-9.]*\).*/\1/p' \
            "$GEN_COVERAGE" | head -1)"
        BASE_RATIO="$(sed -n 's/.*"coverage_ratio"[[:space:]]*:[[:space:]]*\([0-9.]*\).*/\1/p' \
            "$BASELINE" | head -1)"
        if [[ -n "$CUR_RATIO" && -n "$BASE_RATIO" ]]; then
            # 浮点比较走 awk。
            if awk "BEGIN { exit !(${CUR_RATIO} + 0.0000005 < ${BASE_RATIO}) }"; then
                STATUS=1
                echo ""
                echo "regen-check: 覆盖率棘轮失败 —— 当前 ${CUR_RATIO} < 基线 ${BASE_RATIO}"
                echo "解析覆盖率不得降低（蓝图 §6.3）；若属预期（语料扩面）请同 PR 更新"
                echo "devs/ci/parse-coverage-baseline.json"
            elif awk "BEGIN { exit !(${CUR_RATIO} > ${BASE_RATIO} + 0.0000005) }"; then
                echo "regen-check: 提示 —— 覆盖率 ${CUR_RATIO} 高于基线 ${BASE_RATIO}，可同 PR 抬高 baseline"
            else
                echo "regen-check: OK — 覆盖率棘轮达标（当前 ${CUR_RATIO}，基线 ${BASE_RATIO}）"
            fi
        fi
    fi
else
    echo "regen-check: SKIP generated 段 —— 缺已提交 generated/parsed_mods.json（precompile 尚未产出）。"
fi

if [[ "$STATUS" -eq 0 ]]; then
    echo "regen-check: OK — $CHECKED 个文件 byte-diff 全零（manifest.json 暂排除）。"
fi
exit "$STATUS"
