-- extract_runes.lua — `extract-lua --what runes` 的引导脚本
--
-- vendor Data/ModRunes.lua 是纯 `return {...}` 数据文件（自注 "automatically
-- generated"）：key = 符文名，value = `{ [槽类] = { type, 词条行…, statOrder,
-- tradeHashes, rank } }`。luajit dofile 后逐符文 JSONL 输出（serde 形状 =
-- pobr_data::catalog::item_overlay::RuneDef；tradeHashes 与计算/编辑态无关，
-- 不入库——M5c 蓝图 WI-B1 schema 仅 kind/lines/rank/stat_order）。
--
-- 用法：luajit - <vendor_src_dir> <ModRunes>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <ModRunes>\n")
	os.exit(2)
end

local path = vendorSrc .. "/Data/" .. fileListArg .. ".lua"
local chunk, loadErr = loadfile(path)
if not chunk then
	io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
	os.exit(3)
end
local ok, runes = pcall(chunk)
if not ok or type(runes) ~= "table" then
	io.stderr:write("error executing " .. path .. ": " .. tostring(runes) .. "\n")
	os.exit(3)
end

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local function jsonNum(v)
	if v ~= v or v == math.huge or v == -math.huge then
		error("non-finite number in rune data")
	end
	return string.format("%.17g", v)
end

local function jsonNumArray(list, where)
	local parts = {}
	for i, v in ipairs(list) do
		if type(v) ~= "number" then
			error("non-number in " .. where)
		end
		parts[i] = jsonNum(v)
	end
	return "[" .. table.concat(parts, ",") .. "]"
end

local function sortedKeys(t)
	local keys = {}
	for k in pairs(t) do
		keys[#keys + 1] = k
	end
	table.sort(keys)
	return keys
end

for _, name in ipairs(sortedKeys(runes)) do
	local slots = runes[name]
	if type(slots) ~= "table" then
		error("unexpected rune entry shape at " .. tostring(name))
	end
	local slotParts = {}
	for _, slotName in ipairs(sortedKeys(slots)) do
		local slot = slots[slotName]
		if type(slot) ~= "table" or type(slot.type) ~= "string" then
			error("unexpected slot shape at " .. name .. "/" .. tostring(slotName))
		end
		local lines = {}
		for i, line in ipairs(slot) do
			if type(line) ~= "string" then
				error("non-string mod line at " .. name .. "/" .. slotName)
			end
			lines[i] = '"' .. jsonEscape(line) .. '"'
		end
		local fields = {
			'"kind":"' .. jsonEscape(slot.type) .. '"',
			'"lines":[' .. table.concat(lines, ",") .. "]",
		}
		if type(slot.rank) == "table" and #slot.rank > 0 then
			fields[#fields + 1] = '"rank":' .. jsonNumArray(slot.rank, name .. "/" .. slotName .. ".rank")
		end
		if type(slot.statOrder) == "table" and #slot.statOrder > 0 then
			fields[#fields + 1] = '"stat_order":'
				.. jsonNumArray(slot.statOrder, name .. "/" .. slotName .. ".statOrder")
		end
		slotParts[#slotParts + 1] = '"' .. jsonEscape(slotName) .. '":{' .. table.concat(fields, ",") .. "}"
	end
	print('{"name":"' .. jsonEscape(name) .. '","slots":{' .. table.concat(slotParts, ",") .. "}}")
end
