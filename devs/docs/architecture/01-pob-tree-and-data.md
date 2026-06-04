# PoB 天赋树与数据层深度分析

> 原始文件：`Classes/PassiveTree.lua`, `TreeData/`, `Modules/Data.lua`

---

## 1. 天赋树（PassiveTree.lua）

### 1.1 数据结构

| 字段 | 说明 |
|------|------|
| `nodes` | 所有天赋节点的字典（`[nodeId] = nodeData`） |
| `groups` | 节点组（视觉上靠近的节点簇） |
| `root` | 根节点 ID（职业起点） |
| `classes` | 职业起始节点 |
| `ascendancyClasses` | 升华职业 |
| `jewelSlots` | 珠宝插槽节点 |
| `masteryNodes` | 专精节点 |

### 1.2 节点数据结构

```lua
nodeData = {
    id = nodeId,
    group = groupId,
    orbit = orbitIndex,
    orbitIndex = positionInOrbit,
    name = nodeName,
    stats = { [statId] = value },
    mods = { "modifier text 1", "modifier text 2" },
    isNotable = boolean,
    isMastery = boolean,
    isJewelSocket = boolean,
    allocated = boolean,
}
```

### 1.3 关键算法

- **最短路径**：从根节点到目标节点的最短路径（用于自动分配）
- **范围珠宝**：计算珠宝 socket 周围半径内的节点，应用珠宝修饰符
- **永恒珠宝**：用种子生成固定但伪随机的节点替换规则

### 1.4 Rust 映射

```rust
// pobr-tree/src/tree.rs
pub struct PassiveTree {
    pub nodes: HashMap<NodeId, NodeData>,
    pub groups: HashMap<GroupId, NodeGroup>,
    pub root: NodeId,
    pub classes: Vec<ClassStart>,
    pub ascendancy_classes: Vec<AscendancyClass>,
    pub jewel_slots: Vec<NodeId>,
    pub mastery_nodes: Vec<NodeId>,
}

pub struct NodeData {
    pub id: NodeId,
    pub group: GroupId,
    pub orbit: u8,
    pub orbit_index: u8,
    pub name: String,
    pub stats: HashMap<StatId, i32>,
    pub mods: Vec<String>,
    pub is_notable: bool,
    pub is_mastery: bool,
    pub is_jewel_socket: bool,
    pub allocated: bool,
}

impl PassiveTree {
    pub fn shortest_path(&self, from: NodeId, to: NodeId) -> Option<Vec<NodeId>>;
    pub fn apply_radius_jewel(&mut self, socket: NodeId, jewel: &Jewel);
    pub fn apply_timeless_jewel(&mut self, socket: NodeId, seed: u32, jewel_type: TimelessType);
}
```

---

## 2. 数据层（Data.lua）

`Data.lua` 是全局游戏数据的入口，包含：

| 数据类别 | 内容 |
|----------|------|
| `skills` | 所有技能定义（主动 + 支持），按 `grantedEffectId` 索引 |
| `items` | 所有物品定义（普通/稀有/传奇基底） |
| `uniques` | 传奇物品数据 |
| `statDescriptions` | 统计描述文本 |
| `modSyntax` | Modifier 语法规则 |
| `version` | 数据版本 |

在 Rust 中，这些数据将被编译为静态常量或加载为序列化二进制文件。

```rust
// pobr-data/src/game_data.rs
pub struct GameData {
    pub skills: HashMap<GrantedEffectId, SkillDefinition>,
    pub items: HashMap<ItemBaseId, ItemBase>,
    pub uniques: HashMap<String, UniqueItem>,
    pub stat_descriptions: HashMap<StatId, StatDescription>,
    pub version: DataVersion,
}

// 运行时通过 include_bytes! 或 lazy_static 加载
pub static GAME_DATA: Lazy<GameData> = Lazy::new(|| {
    // 从编译时嵌入的二进制数据反序列化
});
```

---

## 3. 导入/导出系统（ImportTab.lua）

### 3.1 Build Code 格式

PoB 的 Build Code 是一个 Base64 压缩字符串，内部是 XML 或 Lua 表序列化格式。

编码流程：
1. 将 Build 状态序列化为 Lua 表/XML
2. 使用 zlib/deflate 压缩
3. Base64 编码
4. 添加版本前缀

### 3.2 Rust 映射

```rust
// pobr-build/src/import_export.rs
pub fn encode_build_code(build: &Build) -> Result<String, EncodeError>;
pub fn decode_build_code(code: &str) -> Result<Build, DecodeError>;

pub enum BuildCodeVersion {
    V1,
    V2,
    // ...
}
```
