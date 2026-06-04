# Content.ggpk 文件格式与解包

`Content.ggpk` 是 Path of Exile 2（以及 POE1）游戏客户端使用的主要资源打包文件，包含了游戏的所有数据资源[^ggpk-tool-repo][^visualggpk2-repo]。

## 文件概述

- **文件名**：`Content.ggpk`
- **位置**：游戏安装目录下（如 `C:\Program Files\Grinding Gear Games\Path of Exile 2\Content.ggpk`）
- **格式**：GGPK (Grinding Gear Games Package)
- **内容**：包含游戏的所有资源文件，包括：
  - 数据表 (`.dat` / `.dat64` 文件)
  - 纹理贴图 (`.dds` 文件)
  - 模型文件
  - 音效文件
  - 本地化文本
  - 技能数据
  - 物品数据

## 文件结构

GGPK 文件是一种打包格式，内部包含：
- 文件索引（路径和偏移信息）
- 压缩或不压缩的数据块
- 目录结构信息

## 解包工具

### 1. ggpk-tool (juddisjudd)

GitHub: https://github.com/juddisjudd/ggpk-tool [^ggpk-tool-repo]

一个现代化的工具，专门用于提取和解析 POE2 的打包 GGPK 格式。

功能：
- 从 GGPK 中提取文件
- 解析 `.dat` 文件格式
- 更新 DAT schema（从 GitHub）
- 转换 DDS 纹理为 PNG 或 WebP

使用示例：
```bash
# 提取文件
bun run src/index.ts extract -p "path/to/Content.ggpk" -o "output/dir"

# 转换 DDS 纹理
bun run src/index.ts convert-dds -i "input/dir" -o "output/dir" -f png
```

### 2. ggpk (ex-nihil)

GitHub: https://github.com/ex-nihil/ggpk [^ggpk-ex-nihil]

CLI 工具和库，用于读取 POE 的 GGPK 文件。

使用示例：
```bash
# 提取特定文件
$ ggpk --path "/games/Path of Exile" -q .+/_.index.bin --binary
```

### 3. VisualGGPK2 (aianlinb)

GitHub: https://github.com/aianlinb/VisualGGPK2 [^visualggpk2-repo]

带有图形界面的库和工具。

功能：
- 浏览 GGPK 内部目录结构
- 过滤文件路径
- 导出/替换文件
- 从补丁服务器恢复文件
- 支持目录替换

### 4. ggpkviewer (shadr)

GitHub: https://github.com/shadr/ggpkviewer [^ggpkviewer-repo]

Rust 语言实现的工具集。

包含：
- `ggpklib` - Rust 库，实现文件解析逻辑
- `ggpkcli` - CLI 工具，从 GGPK 或补丁服务器获取文件

使用示例（Rust）：
```rust
use ggpklib::poefs::{PoeFS, LocalSource, OnlineSource};

// 从本地 GGPK 创建来源
let source = LocalSource::new("path/to/Content.ggpk");
```

### 5. PoET (jcmoyer)

GitHub: https://github.com/jcmoyer/PoET [^poet-repo]

较老的 Python 命令行工具，用于从 POE 内容包提取数据。

使用示例：
```bash
$ poet.py extract "path/to/Content.ggpk" "directory/to/extract/to"
```

## 数据文件 (.dat / .dat64)

提取后的数据文件通常以 `.dat` 或 `.dat64` 格式存储，包含游戏的核心数据：

### 常见数据表类型

- **技能数据**：技能效果、伤害值、消耗等
- **物品数据**：物品基础属性、修饰词池、掉落率
- **怪物数据**：怪物属性、技能、AI 行为
- **任务数据**：任务目标、奖励
- **本地化数据**：多语言文本

### 解析 .dat 文件

.dat 文件是二进制格式，需要 schema 信息来正确解析。社区维护的 schema 可以在 GitHub 上找到，通常由以下项目维护：
- `PathOfBuildingCommunity/PathOfBuilding-PoE2`（POE2 数据解析的主要参考）
- `PathOfBuildingCommunity/PathOfBuilding`（POE1 历史参考）
- `poe-dat-viewer` 相关项目

## 纹理文件 (.dds)

游戏中的纹理以 DDS (DirectDraw Surface) 格式存储。可以使用以下工具转换：
- ggpk-tool 的内置转换功能
- 第三方图像转换工具（如 ImageMagick、Nvidia Texture Tools）

## 注意事项

1. **文件锁定**：某些工具在打开 GGPK 时会锁定文件，阻止其他程序修改
2. **补丁更新**：游戏更新会修改 GGPK 文件，解包工具可能需要更新
3. **版权**：提取的资源属于 Grinding Gear Games，仅供个人学习和研究使用
4. **在线模式**：使用在线模式从补丁服务器获取文件可能需要特殊配置

## 数据应用

解包后的数据可用于：
- 构建规划工具（如 Path of Building）[^pob-repo]
- 游戏数据库网站（如 PoE2DB、PoE Wiki）
- 数据分析
- 模组开发（仅限离线模式）

---

## 参考来源

[^ggpk-tool-repo]: juddisjudd — ggpk-tool (Modern POE2 GGPK extraction tool). https://github.com/juddisjudd/ggpk-tool
[^ggpk-ex-nihil]: ex-nihil — ggpk (CLI tool and library for reading POE GGPK files). https://github.com/ex-nihil/ggpk
[^visualggpk2-repo]: aianlinb — VisualGGPK2 (GUI tool for browsing and exporting GGPK contents). https://github.com/aianlinb/VisualGGPK2
[^ggpkviewer-repo]: shadr — ggpkviewer (Rust implementation for GGPK parsing). https://github.com/shadr/ggpkviewer
[^poet-repo]: jcmoyer — PoET (Python CLI tool for extracting POE content packages). https://github.com/jcmoyer/PoET
[^pob-repo]: PathOfBuildingCommunity — Path of Building (Offline build planning tool for Path of Exile). https://github.com/PathOfBuildingCommunity/PathOfBuilding
