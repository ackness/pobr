#!/usr/bin/env bash
# M4-T1 W-A1 commit-3：ModFlags 位表双跑 diff 报告。
#
# 蓝图（audits/rearchitecture-2026-06-10/blueprints/m4-offence-deep.md §2-T1
# W-A1 步骤 3）：`cargo test --workspace` 与 `--features modflags-pob2` 各跑一遍
# ninja_parity + golden，diff 报告应为零——此时（切换前）尚无人按新位消费，
# 双写通道（cfg 武器位 / mod 武器位）的 AND 语义蕴含于 condition 通道。
#
# 退出码：0 = diff 干净（允许后续切换）；非 0 = 测试失败或 parity 输出有 diff。
#
# 用法：devs/scripts/modflags-dualrun.sh [输出目录]
#   输出目录默认 target-dualrun/modflags-report（每跑覆盖）。
#   尊重外部 CARGO_TARGET_DIR；未设时用 ./target-dualrun 隔离（避免与并行
#   会话共享构建锁，见 CLAUDE.md 构建环境）。
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target-dualrun}"
OUT="${1:-$TARGET/modflags-report}"
mkdir -p "$OUT"

# 过滤构建/计时噪声行，只留测试与 parity 数值输出（per-build 对照 + 聚合摘要）。
strip_noise() {
    grep -vE '^\s*(Compiling|Finished|Running|Blocking|warning:|running [0-9]+ tests?|test result:)' \
        | grep -vE '^test .* \.\.\. ok$' \
        | grep -vE 'finished in [0-9.]+s' \
        | sed '/^\s*$/d'
}

run_config() { # $1 = 标签；$2 = 额外 cargo 参数（可空）
    local label="$1" feat="${2:-}"
    echo "==== [$label] cargo nextest run --workspace $feat ===="
    # shellcheck disable=SC2086
    CARGO_TARGET_DIR="$TARGET" cargo nextest run --workspace $feat \
        2>&1 | tail -2 | tee "$OUT/nextest-$label.txt"

    echo "==== [$label] ninja_parity 逐 build 报告 ===="
    # shellcheck disable=SC2086
    CARGO_TARGET_DIR="$TARGET" cargo test -p pobr-build $feat \
        --test ninja_parity parity_baseline_report -- --nocapture \
        2>&1 | strip_noise > "$OUT/parity-$label.txt"
    tail -8 "$OUT/parity-$label.txt"
}

run_config legacy ""
run_config pob2 "--features pobr-build/modflags-pob2"

echo "==== diff（应为空，逐 build 逐值） ===="
if diff -u "$OUT/parity-legacy.txt" "$OUT/parity-pob2.txt" > "$OUT/parity.diff"; then
    echo "diff=0：双跑 parity 输出逐值一致（$OUT/parity.diff 为空）"
else
    echo "双跑 diff 非空（见 $OUT/parity.diff）——禁止切换，先归因差异" >&2
    cat "$OUT/parity.diff" >&2
    exit 1
fi
