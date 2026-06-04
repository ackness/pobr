# PoB 兼容、自定义物品与多语言架构

---

## 1. 目标

PoBR 是新项目，架构按 Rust 原生方式设计；兼容目标是 PoB 的用户数据格式、用户工作流和关键计算结果。Rust 实现以稳定 ID、显式计算阶段和可验证输出为核心。

产品能力必须覆盖：

- 快捷复制 BD：从当前 Build 生成 PoB 兼容 Build Code。
- 快捷导入：识别剪贴板/输入框里的 Build Code、XML、pobb.in URL、raw item text。
- PoB 兼容 Build Code：`XML -> deflate -> URL-safe Base64`。
- 自定义物品创建：选择 base、rarity、quality、socket、modifier 区块和 roll。
- 多语言显示：`en-US` 和 `zh-TW`。

开发顺序以计算核心为主线。Build Code、raw item、custom item 和多语言提供计算输入、回归样本和展示能力。

后续可扩展：

- 更多语言包。
- 中文繁体 raw item text 导入。
- PoE2 数据集和 PoE2 专属文本。
- 更多外部构筑站点。

---

## 2. 兼容边界

| 能力 | 第一阶段 | 后续扩展 |
|------|----------|----------|
| PoB Build Code 解码 | Build 兼容阶段支持 | 保持兼容新版字段 |
| PoB Build Code 编码 | Build 兼容阶段支持 | 支持更多 section |
| XML load/save | Build 兼容阶段支持核心字段 | 覆盖完整 PoB schema |
| pobb.in URL | 识别 + 下载 code | 上传分享 |
| raw item text | 英文输入 | `zh-TW` 输入 |
| custom item | 手工输入 mod + roll | affix pool/search/crafting 流程 |
| UI 多语言 | `en-US`, `zh-TW` | 任意新增语言 |
| Modifier 解析 | 英文 PoB 兼容 | 多语言反向解析 |

兼容失败时保留原始输入并返回结构化诊断，UI 可以继续保存草稿或提示用户补全。

---

## 3. 快捷导入/复制

### 3.1 输入识别

`pobr-build::detect_import(input)` 只做字符串识别，不做 I/O。

```rust
pub enum ImportKind {
    PobCode,
    Xml,
    PobbinUrl,
    RawItemText,
    Unknown,
}
```

识别顺序：

1. URL：`https://pobb.in/...`、后续可加 pastebin/maxroll。
2. XML：以 `<PathOfBuilding>` 或兼容根节点开头。
3. Raw item text：包含 `Rarity:`、`Item Class:`、分隔线等物品复制格式特征。
4. Build Code：Base64/URL-safe Base64 字符串，解码后能 inflate。

### 3.2 Build Code 编码

```rust
pub fn decode_pob_code(code: &str) -> Result<String, BuildCodeError>;
pub fn encode_pob_code(xml: &str) -> Result<String, BuildCodeError>;
```

格式要求：

- Base64 解码前恢复 PoB 替换：`-` -> `+`, `_` -> `/`。
- inflate 后输出 XML 字符串。
- encode 时先 XML，再 deflate，再 Base64，最后替换为 URL-safe 字符。
- 语言、窗口状态、UI 偏好不写入 PoB 兼容 code。

### 3.3 剪贴板职责

`apps/pobr-desktop` 和 `apps/pobr-cli` 负责读取剪贴板、文件、URL。核心 crates 只处理传入字符串，便于测试和 WASM 复用。

---

## 4. 自定义物品

### 4.1 数据模型

```rust
pub struct CustomItemDraft {
    pub base: ItemBaseId,
    pub rarity: ItemRarity,
    pub quality: u8,
    pub sockets: Vec<Socket>,
    pub implicits: Vec<EditableMod>,
    pub explicits: Vec<EditableMod>,
    pub crafted: Vec<EditableMod>,
    pub unsupported_lines: Vec<String>,
}
```

`CustomItemDraft` 是编辑态；参与计算前转换为 `pobr-data::Item`。

### 4.2 Modifier 区块

```rust
pub enum ItemTextSection {
    Enchant,
    Implicit,
    Explicit,
    Crafted,
    Fractured,
    Veiled,
    Eldritch,
    Crucible,
    Flavour,
    Unknown(String),
}
```

设计原则：

- 区块枚举允许保留 PoB/PoE 不同时代的 modifier 来源。
- 第一阶段只要求英文 mod text 能进入 `pobr-core::mod_parser`。
- 不支持的 modifier 不丢弃，保留到 draft，UI 标红提示。
- roll 值和原始文本分开存储，方便后续做 slider/stepper 编辑。

---

## 5. 多语言

### 5.1 语言包结构

```
crates/pobr-i18n/locales/
├── en-US/
│   ├── ui.toml
│   ├── stats.toml
│   └── errors.toml
└── zh-TW/
    ├── ui.toml
    ├── stats.toml
    └── errors.toml
```

`en-US` 是 canonical source 和 fallback。`zh-TW` 初始覆盖 UI、常见错误、核心 stat 展示名。

### 5.2 Key 约定

```toml
[build]
copy_code = "Copy BD"
import_code = "Import BD"

[items]
create_custom = "Create custom item"
unsupported_mod = "Unsupported modifier"
```

繁体中文示例：

```toml
[build]
copy_code = "複製 BD"
import_code = "匯入 BD"

[items]
create_custom = "建立自訂物品"
unsupported_mod = "不支援的詞綴"
```

### 5.3 本地化边界

- 计算核心不接收显示语言。
- Build Code 不依赖语言。
- `StatId`、`SkillId`、`ItemBaseId` 是跨语言稳定键。
- UI 展示通过 `Translator` 做最后一步格式化。
- 多语言 raw item import 是反向解析问题，放在 `pobr-i18n::stat_text` 和 `pobr-item` 的后续集成中。

---

## 6. 测试矩阵

| 测试 | fixture | 断言 |
|------|---------|------|
| Build Code roundtrip | `fixtures/pob-codes/*.txt` | decode -> encode 后等价 |
| XML roundtrip | `fixtures/builds/*.xml` | load/save 后关键字段一致 |
| Raw item import | `fixtures/raw-items/en-US/*.txt` | Item 字段和 mod 区块正确 |
| Custom item draft | Rust test fixtures | draft -> item -> draft 关键字段不丢失 |
| i18n fallback | `locales/zh-TW` | 缺失 key 回落到 `en-US` |
| i18n completeness | `tools/lint-i18n` | key 集合完整、格式参数一致 |

---

## 7. 推荐开发顺序

1. 建立 workspace、基础类型和 fixture 目录。
2. 实现 Modifier 语义、ModDB、ModList 和英文 parser/cache。
3. 实现最小计算闭环，输出可验证 breakdown。
4. 接入技能、物品、天赋等 modifier 来源。
5. 实现 PoB Build Code decode/encode，并加入 roundtrip 测试。
6. 实现 raw item text 英文解析和 custom item draft，作为计算输入来源。
7. 建立 `pobr-i18n`，完成 `en-US`/`zh-TW` 的 UI key 和 fallback。
8. 最后做 GUI、WASM、交易 API 和性能优化。

这个顺序让底层计算先形成可信闭环，上层兼容与显示能力围绕计算验证逐步接入。
