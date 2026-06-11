-- extract_catalysts.lua — `extract-lua --what catalysts` 的引导脚本
--
-- catalystList / catalystDescriptorList / catalystTags 三个平行数组嵌在
-- vendor Classes/Item.lua:14-29 **逻辑文件内**（无法 dofile）。本脚本读源码
-- 文本，用 `%b{}` 平衡匹配截取三个 `local xxx = {...}` 表字面量后 load 执行
-- （表字面量自身是合法 Lua；比正则啃可靠且 drift 可检——截取失败 = 源码
-- 结构变化，显式报错）。按 index 合并为 12 条 JSONL（serde 形状 =
-- pobr_data::catalog::item_overlay::CatalystDef，id 1-based）。
--
-- 用法：luajit - <vendor_src_dir> <Item>（文件名固定 Classes/Item.lua）

local vendorSrc = arg and arg[1]
if not vendorSrc then
	io.stderr:write("usage: luajit - <vendor_src_dir> Item\n")
	os.exit(2)
end

local path = vendorSrc .. "/Classes/Item.lua"
local f, openErr = io.open(path, "r")
if not f then
	io.stderr:write("cannot open " .. path .. ": " .. tostring(openErr) .. "\n")
	os.exit(3)
end
local source = f:read("*a")
f:close()

local function extractTable(varName)
	local literal = source:match("local " .. varName .. " = (%b{})")
	if not literal then
		io.stderr:write("cannot locate `local " .. varName .. " = {...}` in " .. path .. "\n")
		os.exit(3)
	end
	local chunk, loadErr = loadstring("return " .. literal)
	if not chunk then
		io.stderr:write("cannot load " .. varName .. " literal: " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	return chunk()
end

local catalystList = extractTable("catalystList")
local catalystDescriptorList = extractTable("catalystDescriptorList")
local catalystTags = extractTable("catalystTags")

if #catalystList ~= #catalystDescriptorList or #catalystList ~= #catalystTags then
	io.stderr:write(
		string.format(
			"catalyst tables length mismatch: list=%d descriptors=%d tags=%d\n",
			#catalystList,
			#catalystDescriptorList,
			#catalystTags
		)
	)
	os.exit(3)
end

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

for i = 1, #catalystList do
	local tags = {}
	for j, tag in ipairs(catalystTags[i]) do
		tags[j] = '"' .. jsonEscape(tag) .. '"'
	end
	print(
		'{"id":'
			.. i
			.. ',"name":"'
			.. jsonEscape(catalystList[i])
			.. '","descriptor":"'
			.. jsonEscape(catalystDescriptorList[i])
			.. '","tags":['
			.. table.concat(tags, ",")
			.. "]}"
	)
end
