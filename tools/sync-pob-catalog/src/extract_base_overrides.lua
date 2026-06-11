-- extract_base_overrides.lua — sync-pob-catalog `extract-bases` 的引导脚本
--
-- 在**最小 stub 环境**下加载 vendor/PathOfBuilding-PoE2 的 Data/Bases/<file>.lua
-- （这些文件以 `local itemBases = ...` 接收注入，顶层为纯表字面量赋值，无其它
-- 全局依赖），抽取两个 GGG `.dat` 路线不可得的基底列并以 JSONL 写到 stdout：
--
--   block_chance ← itemBases[name].armour.BlockChance（`ShieldTypes.Block`）
--   spirit       ← itemBases[name].spirit            （`ItemSpirit.SpiritGranted`）
--
-- 确定性约定：本脚本只负责「忠实抽取 + 合法 JSON」；最终按 name 排序与整体
-- 文档（_meta + overrides）的 byte-stable 序列化由 Rust 侧统一完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <逗号分隔的基底文件名（不含 .lua 后缀）>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <file1,file2,...>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- 加载目标数据文件（vendor 只读，不做任何修改）
----------------------------------------------------------------------
local itemBases = {}
for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/Bases/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local ok, runErr = pcall(chunk, itemBases)
	if not ok then
		io.stderr:write("error executing " .. path .. ": " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
end

----------------------------------------------------------------------
-- JSON 工具（仅需字符串转义 + 数字；结构很浅，无需通用编码器）
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

-- %.17g 保证 f64 精度无损往返；Rust 侧 serde_json 会重排为最短表示
local function jsonNum(v)
	if v ~= v or v == math.huge or v == -math.huge then
		error("non-finite number in base item data")
	end
	return string.format("%.17g", v)
end

----------------------------------------------------------------------
-- 抽取并输出 JSONL（只输出至少携带一个目标字段的基底）
----------------------------------------------------------------------
for name, base in pairs(itemBases) do
	if type(base) == "table" then
		local blockChance = nil
		if type(base.armour) == "table" and type(base.armour.BlockChance) == "number" then
			blockChance = base.armour.BlockChance
		end
		local spirit = nil
		if type(base.spirit) == "number" then
			spirit = base.spirit
		end
		if blockChance or spirit then
			local parts = { '"name":"' .. jsonEscape(name) .. '"' }
			if blockChance then
				parts[#parts + 1] = '"block_chance":' .. jsonNum(blockChance)
			end
			if spirit then
				parts[#parts + 1] = '"spirit":' .. string.format("%d", spirit)
			end
			print("{" .. table.concat(parts, ",") .. "}")
		end
	end
end
