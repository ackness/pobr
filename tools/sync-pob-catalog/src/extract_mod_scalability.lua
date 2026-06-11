-- extract_mod_scalability.lua — `extract-lua --what mod-scalability` 的引导脚本
--
-- vendor Data/ModScalability.lua 是纯 `return {...}` 数据文件（自注
-- "automatically generated"）：key = `#`-模板化词条文本，value = 逐数值槽的
-- `{ isScalable = bool, formats = {...}? }` 数组。luajit dofile 直接执行后
-- 逐条 JSONL 输出（serde 形状 = pobr_data::catalog::item_overlay::
-- ModScalabilityEntryDef）；排序由 Rust 侧统一完成。
--
-- 用法：luajit - <vendor_src_dir> <ModScalability>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <ModScalability>\n")
	os.exit(2)
end

local path = vendorSrc .. "/Data/" .. fileListArg .. ".lua"
local chunk, loadErr = loadfile(path)
if not chunk then
	io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
	os.exit(3)
end
local ok, scalability = pcall(chunk)
if not ok or type(scalability) ~= "table" then
	io.stderr:write("error executing " .. path .. ": " .. tostring(scalability) .. "\n")
	os.exit(3)
end

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

for template, slots in pairs(scalability) do
	if type(template) ~= "string" or type(slots) ~= "table" then
		error("unexpected ModScalability entry shape at " .. tostring(template))
	end
	local slotParts = {}
	for i, slot in ipairs(slots) do
		if type(slot) ~= "table" then
			error("non-table slot on " .. template)
		end
		-- 已知键校验：isScalable / formats 之外的键 = vendor schema 漂移，报错
		for k in pairs(slot) do
			if k ~= "isScalable" and k ~= "formats" then
				error("unknown slot key '" .. tostring(k) .. "' on " .. template)
			end
		end
		local fields = {}
		if slot.isScalable then
			fields[#fields + 1] = '"is_scalable":true'
		else
			fields[#fields + 1] = '"is_scalable":false'
		end
		if type(slot.formats) == "table" and #slot.formats > 0 then
			local fmts = {}
			for j, fmt in ipairs(slot.formats) do
				if type(fmt) ~= "string" then
					error("non-string format on " .. template)
				end
				fmts[j] = '"' .. jsonEscape(fmt) .. '"'
			end
			fields[#fields + 1] = '"formats":[' .. table.concat(fmts, ",") .. "]"
		end
		slotParts[i] = "{" .. table.concat(fields, ",") .. "}"
	end
	print('{"template":"' .. jsonEscape(template) .. '","slots":[' .. table.concat(slotParts, ",") .. "]}")
end
