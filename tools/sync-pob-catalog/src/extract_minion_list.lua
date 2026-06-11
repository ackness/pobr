-- extract_minion_list.lua — `extract-lua --what minion-list` 的引导脚本
--
-- 在最小 stub 环境下加载 vendor Data/Skills/<file>.lua（与
-- extract_skill_overrides.lua 同一注入约定），抽取宝石授予效果 → 召唤物的
-- 外键边车字段（00-index 裁决 §4-10 归 M5a；`minionList` 不在 .dat，来自
-- Export 模板手工指令 `#minionList`，见 Export/Scripts/skills.lua:771-776）：
--
--   minion_list          ← skill.minionList（召唤的 minion id 列表）
--   minion_uses          ← skill.minionUses（借用玩家装备槽，值为 true 的键升序）
--   minion_has_item_set  ← skill.minionHasItemSet
--
-- 注：M5a 蓝图 A3 还列了 addMinionList / minionLevel 族键——当前 vendor
-- Data/Skills/*.lua 中**零出现**（2026-06 核查），schema 字段已预留，
-- 此处不抽（出现后随 vendor bump 自然进入）。
--
-- 输出 JSONL（serde 形状 = pobr_data::catalog::actors::GrantedEffectMinionDef）；
-- 排序由 Rust 侧统一完成。
--
-- 用法：luajit - <vendor_src_dir> <逗号分隔的技能文件名（不含 .lua 后缀）>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <file1,file2,...>\n")
	os.exit(2)
end

-- 最小 stub 环境（同 extract_skill_overrides.lua）
SkillType = setmetatable({}, { __index = function(_, k) return k end })
ModFlag = setmetatable({}, { __index = function(_, k) return k end })
KeywordFlag = setmetatable({}, { __index = function(_, k) return k end })

local function makeSkillMod(modName, modType, modVal, flags, keywordFlags, ...)
	return {
		name = modName,
		type = modType,
		value = modVal,
		flags = flags or 0,
		keywordFlags = keywordFlags or 0,
		...,
	}
end
local function makeFlagMod(modName, ...)
	return makeSkillMod(modName, "FLAG", true, 0, 0, ...)
end
local function makeSkillDataMod(dataKey, dataValue, ...)
	return makeSkillMod("SkillData", "LIST", { key = dataKey, value = dataValue }, 0, 0, ...)
end

local skills = {}
for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/Skills/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local ok, runErr = pcall(chunk, skills, makeSkillMod, makeFlagMod, makeSkillDataMod)
	if not ok then
		io.stderr:write("error executing " .. path .. ": " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
end

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local function jsonStrArray(list)
	local parts = {}
	for i, v in ipairs(list) do
		parts[i] = '"' .. jsonEscape(v) .. '"'
	end
	return "[" .. table.concat(parts, ",") .. "]"
end

for effectId, skill in pairs(skills) do
	if type(skill) == "table" then
		local parts = {}
		if type(skill.minionList) == "table" and #skill.minionList > 0 then
			parts[#parts + 1] = '"minion_list":' .. jsonStrArray(skill.minionList)
		end
		if type(skill.minionUses) == "table" then
			local uses = {}
			for k, v in pairs(skill.minionUses) do
				if v == true then
					uses[#uses + 1] = k
				end
			end
			table.sort(uses)
			if #uses > 0 then
				parts[#parts + 1] = '"minion_uses":' .. jsonStrArray(uses)
			end
		end
		if skill.minionHasItemSet then
			parts[#parts + 1] = '"minion_has_item_set":true'
		end
		if #parts > 0 then
			print('{"effect_id":"' .. jsonEscape(effectId) .. '",' .. table.concat(parts, ",") .. "}")
		end
	end
end
