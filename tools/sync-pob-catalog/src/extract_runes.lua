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
	local firstSlot = true
	local limitId
	for _, slotName in ipairs(sortedKeys(slots)) do
		local slot = slots[slotName]
		if type(slot) ~= "table" or type(slot.type) ~= "string" then
			error("unexpected slot shape at " .. name .. "/" .. tostring(slotName))
		end
		-- The context-free catalog identifies a single whole-build limit group.
		if not firstSlot and limitId ~= slot.limitId then
			error("inconsistent limitId across slots at " .. name)
		end
		firstSlot = false
		limitId = slot.limitId
		local lines = {}
		for i, line in ipairs(slot) do
			if type(line) ~= "string" then
				error("non-string mod line at " .. name .. "/" .. slotName)
			end
			lines[i] = '"' .. jsonEscape(line) .. '"'
		end
		-- New exports separate bonded bonuses; preserve the conditional raw
		-- item syntax used by existing consumers and older exports.
		local statOrder = {}
		for _, stat in ipairs(slot.statOrder or {}) do
			statOrder[#statOrder + 1] = stat
		end
		if type(slot.bonded) == "table" then
			for _, line in ipairs(slot.bonded) do
				if type(line) ~= "string" then
					error("non-string bonded line at " .. name .. "/" .. slotName)
				end
				lines[#lines + 1] = '"' .. jsonEscape("Bonded: " .. line) .. '"'
			end
			for _, stat in ipairs(slot.bonded.statOrder or {}) do
				statOrder[#statOrder + 1] = stat
			end
		end
		local fields = {
			'"kind":"' .. jsonEscape(slot.type) .. '"',
			'"lines":[' .. table.concat(lines, ",") .. "]",
		}
		if type(slot.levelReq) == "number" then
			fields[#fields + 1] = '"required_level":' .. jsonNum(slot.levelReq)
		end
		if type(slot.limit) == "number" then
			fields[#fields + 1] = '"limit":' .. jsonNum(slot.limit)
		end
		if type(slot.limitId) == "string" then
			fields[#fields + 1] = '"limit_id":"' .. jsonEscape(slot.limitId) .. '"'
		end
		for key, source in pairs({ socket_bound = "isSocketBound", can_socket_in_unique_items = "canSocketInUniqueItems", can_socket_in_jewellery = "canSocketInJewellery", can_socket_in_corrupted_sanctified = "canSocketInCorruptedSanctified" }) do
			fields[#fields + 1] = '"' .. key .. '":' .. tostring(slot[source] == true)
		end
		if type(slot.rank) == "table" and #slot.rank > 0 then
			fields[#fields + 1] = '"rank":' .. jsonNumArray(slot.rank, name .. "/" .. slotName .. ".rank")
		end
		if #statOrder > 0 then
			fields[#fields + 1] = '"stat_order":'
				.. jsonNumArray(statOrder, name .. "/" .. slotName .. ".statOrder")
		end
		slotParts[#slotParts + 1] = '"' .. jsonEscape(slotName) .. '":{' .. table.concat(fields, ",") .. "}"
	end
	print('{"name":"' .. jsonEscape(name) .. '","slots":{' .. table.concat(slotParts, ",") .. "}}")
end
