-- extract_stat_descriptions.lua — sync-pob-catalog `extract-lua --what stat-descriptions`
--
-- 在**最小环境**（描述文件是纯数据 `return {...}`，无函数/flag，loadfile 即可）下
-- 加载 vendor/PathOfBuilding-PoE2 的 Data/StatDescriptions/<scope>.lua（+ 沿 `parent`
-- 链继承的 root），为每个 stat_id 渲染一段 canonical 文本（喂代表值 V=1），把
-- stat_id → 文本输出为 JSONL（每行一个 JSON 对象）到 stdout：
--
--   {"stat":"<id>","scope":"<scope>","text":"<rendered line>","line":N,"compound":false}
--   {"stat":"<id>","scope":"<scope>","compound":true,"member_stats":[...],"template":"<raw text>"}
--
-- 渲染逻辑忠实移植自 vendor Modules/StatDescriber.lua（matchLimit:41-61 选变体 +
-- gsub 链:281-315 替换占位符 + 按字面 `\n` 切行:316）。**省略 applySpecial 值变换**
-- （negate/divide_by_* 只改数值不改词形；模板提取只需词形——ModName/ModType 由 §B
-- 解析得出，运行期填真实掷值）。**多 stat（compound）不强解**，原样输出供 §B 处置。
--
-- 确定性约定：本脚本只忠实抽取 + 合法 JSON；排序与 byte-stable 序列化由 Rust 侧。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <scope_file>   -- scope_file 不含 .lua（如 passive_skill_stat_descriptions）

local vendorSrc = arg and arg[1]
local scopeFile = arg and arg[2]
if not vendorSrc or not scopeFile then
	io.stderr:write("usage: luajit - <vendor_src_dir> <scope_file_without_.lua>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- JSON 编码（最小，仅本脚本所需的 string/number/bool/array 形态）。
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		if c == '"' then return '\\"'
		elseif c == "\\" then return "\\\\"
		elseif c == "\n" then return "\\n"
		elseif c == "\r" then return "\\r"
		elseif c == "\t" then return "\\t"
		else return string.format("\\u%04x", string.byte(c)) end
	end))
end
local function jsonStr(s) return '"' .. jsonEscape(s) .. '"' end
local function jsonArr(t)
	local parts = {}
	for _, v in ipairs(t) do parts[#parts + 1] = jsonStr(v) end
	return "[" .. table.concat(parts, ",") .. "]"
end

----------------------------------------------------------------------
-- 加载一个 scope 描述文件（纯数据）。沿 `parent` 链返回 scopeList（child 在前）。
----------------------------------------------------------------------
local loaded = {}
local function loadScope(name)
	if loaded[name] then return loaded[name] end
	local path = vendorSrc .. "/Data/StatDescriptions/" .. name .. ".lua"
	local chunk, err = loadfile(path)
	if not chunk then
		io.stderr:write("loadfile failed: " .. tostring(err) .. "\n")
		os.exit(3)
	end
	local scope = chunk()
	scope.__name = name
	if scope.parent then
		scope.__scopeList = { scope }
		for _, s in ipairs(loadScope(scope.parent)) do
			scope.__scopeList[#scope.__scopeList + 1] = s
		end
	else
		scope.__scopeList = { scope }
	end
	loaded[name] = scope.__scopeList
	return scope.__scopeList
end

----------------------------------------------------------------------
-- matchLimit（移植自 StatDescriber.lua:41-61，单值 quality=nil）：按 val 选首个命中
-- limit 的变体。val[i] = {min=V,max=V}。
----------------------------------------------------------------------
local function matchLimit(lang, val)
	for _, desc in ipairs(lang) do
		if not desc.gem_quality then
			local match = true
			for i, limit in ipairs(desc.limit) do
				local v = val[i] or { min = 0, max = 0 }
				if limit[1] == "!" then
					if v.min == limit[2] then match = false break end
				elseif (limit[2] ~= "#" and v.min > limit[2]) or (limit[1] ~= "#" and v.min < limit[1]) then
					match = false break
				end
			end
			if match then return desc end
		end
	end
end

----------------------------------------------------------------------
-- 渲染变体文本（移植自 StatDescriber.lua:281-315 gsub 链；省 applySpecial）。
-- 所有占位符代入 val[n].min（= 代表值），单值无区间。
----------------------------------------------------------------------
local function renderText(text, val)
	local function single(i)
		local v = val[i] or { min = 0 }
		return tostring(v.min)
	end
	return (text
		:gsub("{(%d)}", function(n) return single(tonumber(n) + 1) end)
		:gsub("{}", function() return single(1) end)
		:gsub("{:%+?d}", function() local v = val[1] or { min = 0 }; if v.min >= 0 then return "+" .. v.min else return tostring(v.min) end end)
		:gsub("{(%d):(%+?)d}", function(n, fmt)
			local v = val[tonumber(n) + 1] or { min = 0 }
			if fmt == "+" and v.min >= 0 then return "+" .. v.min else return tostring(v.min) end
		end)
		:gsub("%%%%", "%%"))
end

-- 按字面 `\n`（数据里是 `\\n`）切行（移植自 :316）。
local function splitLines(s)
	local out = {}
	for line in (s .. "\\n"):gmatch("([^\\]+)\\n") do out[#out + 1] = line end
	return out
end

----------------------------------------------------------------------
-- 为一个 descriptor + 代表值 V 渲染 canonical 文本行。返回 lines 或 nil。
----------------------------------------------------------------------
local function describe(descriptor, v)
	local lang = descriptor[1]
	if not lang then return nil end
	local val = {}
	for i = 1, #descriptor.stats do val[i] = { min = v, max = v } end
	local desc = matchLimit(lang, val)
	if not desc then return nil end
	return splitLines(renderText(desc.text, val))
end

----------------------------------------------------------------------
-- 主：对每个 scope（逗号分隔）遍历其 lookup 表（`["stat_id"]=idx`），逐 stat 渲染。
-- 多 scope 时各自只抽自己的 own keys（不抽继承的 parent，避免重复——root 单独抽）；
-- 跨 scope 同 stat_id 由 Rust 侧按 scope 精确度去重。
----------------------------------------------------------------------
local function processScope(scopeFile)
local scopeList = loadScope(scopeFile)
local scope = scopeList[1]
local emitted = {}

-- lookup 键 = scope 里的字符串键（排除 parent/name/__* + 整数键 descriptor）。
local statIds = {}
for k, v in pairs(scope) do
	if type(k) == "string" and type(v) == "number" and not k:match("^__") and k ~= "parent" and k ~= "name" then
		statIds[#statIds + 1] = k
	end
end
table.sort(statIds)

for _, statId in ipairs(statIds) do
	if not emitted[statId] then
		emitted[statId] = true
		local descriptor = scope[scope[statId]]
		if descriptor and descriptor.stats then
			if #descriptor.stats == 1 then
				-- 单 stat：喂 V=1 渲染；若无命中变体（仅负向）退 V=-1。
				local lines = describe(descriptor, 1) or describe(descriptor, -1)
				if lines then
					for i, line in ipairs(lines) do
						io.write(string.format(
							'{"stat":%s,"scope":%s,"text":%s,"line":%d,"compound":false}\n',
							jsonStr(statId), jsonStr(scopeFile), jsonStr(line), i))
					end
				else
					io.write(string.format('{"stat":%s,"scope":%s,"compound":false,"unrendered":true}\n',
						jsonStr(statId), jsonStr(scopeFile)))
				end
			else
				-- compound（多 stat）：原样输出 raw text + member_stats，不强解。
				-- 多个 member stat_id 共指同一 descriptor——标记全部 member，避免重复输出。
				local members = {}
				for _, s in ipairs(descriptor.stats) do
					members[#members + 1] = s
					emitted[s] = true
				end
				local lang = descriptor[1]
				local rawText = (lang and lang[1] and lang[1].text) or ""
				io.write(string.format(
					'{"stat":%s,"scope":%s,"compound":true,"member_stats":%s,"template":%s}\n',
					jsonStr(descriptor.stats[1]), jsonStr(scopeFile), jsonArr(members), jsonStr(rawText)))
			end
		end
	end
end
end

-- 分发：arg[2] 是逗号分隔的 scope 列表（对齐 sync-pob-catalog `--files` 约定）。
for scope in (scopeFile .. ","):gmatch("([^,]+),") do
	processScope((scope:gsub("%s+", "")))
end
