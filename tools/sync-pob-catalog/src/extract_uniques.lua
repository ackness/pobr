-- extract_uniques.lua — `extract-lua --what uniques` 的引导脚本
--
-- vendor Data/Uniques/<type>.lua（27 个 itemTypes 文件，对应 Modules/Data.lua:26
-- itemTypes 列表 + :1054-1058 加载序）是纯 `return { [[raw 文本块]], ... }`
-- 数据文件；Special/race.lua 同形。逐块 JSONL 输出 `{item_type, raw}`
-- （raw 逐字节保留——P15 双层中的原始层；name/base/variants 等索引列由
-- Rust 侧预解析）。
--
-- 范围注：Special/Generated.lua（程序化生成，依赖运行时 data.itemMods）与
-- Special/New.lua（未定稿池，写入 data.uniques.new）不在抽取范围。
--
-- 用法：luajit - <vendor_src_dir> <逗号分隔的 Uniques 文件名（不含 .lua）>
--   文件名含 `/` 时按相对 Data/Uniques/ 的路径处理（如 Special/race）。

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <amulet,axe,...,Special/race>\n")
	os.exit(2)
end

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/Uniques/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local ok, blocks = pcall(chunk)
	if not ok or type(blocks) ~= "table" then
		io.stderr:write("error executing " .. path .. ": " .. tostring(blocks) .. "\n")
		os.exit(3)
	end
	-- item_type = 文件名去目录前缀（Special/race → race）
	local itemType = fileName:match("([^/]+)$")
	for _, raw in ipairs(blocks) do
		if type(raw) ~= "string" then
			error("non-string unique block in " .. fileName)
		end
		print('{"item_type":"' .. jsonEscape(itemType) .. '","raw":"' .. jsonEscape(raw) .. '"}')
	end
end
