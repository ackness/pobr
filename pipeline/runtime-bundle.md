# 运行时数据组合封存

`data/<version>/manifest.json` 固定版本文件；运行时还读取同级 `overlay-common/` 的共享规则。
`data:bundle` 将两者连同已入库的翻译、兼容表封存在一个独立目录中，让历史组合可以被取回。
封存不重新导出数据，也不替代来源、游戏数值或消费者验证。

```sh
mise run data:bundle -- --out .cache/data-bundles/first
mise run data:bundle:verify -- --bundle .cache/data-bundles/first
```

默认选择 `data/CURRENT`，也可传 `--version`。来源必须有补丁匹配的 schema-3 manifest；
所有已登记文件逐一验证 SHA-256 和 JSON。域候选标记、列漂移、损坏输入及符号链接会使封存失败。
输出必须是正式 `data/` 之外的新目录，已有目录不会被覆盖。输入在生成期间变化时不完成输出。

产物结构为 `bundle.json`、`data/CURRENT`、`data/<version>/` 及 `data/overlay-common/`。
只复制 manifest 登记的版本文件及全部共享 JSON，不把临时报告和用户 `patch/` 当作发行内容。
`bundle.json` 是最后写入的完成标记；若写入失败遗留了部分目录，验证会拒绝该目录。

收据中的两个身份用途不同：

- `bundle_id` 固定整个封存内容及收据契约。传输或取回旧包时，将已核对交付记录中的 ID
  传给 `--expected-id`；仅检查包内自洽，不能确认拿到的是原先指定的包。
- `runtime_identity` 按 Rust `GameData::identity()` 的同一格式计算默认运行时输入身份，
  包括共享规则，不包括用户 patch。manifest 的排版可能改变 bundle ID，而不改变这个运行时身份。

调用示例（`POBR_BUNDLE_ID` 来自事先核对的封存记录）：

```sh
mise run data:bundle:verify -- --bundle .cache/data-bundles/first --expected-id "$POBR_BUNDLE_ID"
POBR_DATA_ROOT="$PWD/.cache/data-bundles/first/data" mise exec -- cargo run -p pobr-cli -- \
  calculate-build --file examples/demo-bd-test/builds/sorceress-disciple-of-varashta-comet/code.txt --json
```

选择另一个已验证组合的 `data/` 目录即可进行独立的读取/回滚演练，无需改动仓库活动包。
本地 patch 作为另外的身份维护，回滚后需要针对选中的数据组合重新验证，再由消费者显式应用。

本入口检验的是字节封存与可取回性：不会为旧数据补造来源收据，也不会将客户端解码、
游戏机制、国服数值或整库计算标成已认证。正式发布/采用仍须完成相应领域与消费者验收。
