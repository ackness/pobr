-- extract_stat_set_labels.lua — sync-pob-catalog `extract-lua --what stat-set-labels` 的引导脚本
--
-- 在与 extract_skill_overrides.lua 相同的**最小 stub 环境**下加载 vendor
-- Data/Skills/<file>.lua，抽取每个技能 `statSets[i].label`（statSet 形态 label 文本，
-- 原始 `.dat` `GrantedEffectStatSets.Label` 的 FK 目标表 `GrantedEffectLabels` 不可
-- 下载——vendor 导出时已解析为文本，`Export/Scripts/skills.lua:478`）并以 JSONL
-- 写到 stdout：
--
--   {"skill":"<效果 id>","set_index":<1-based 导出序号>,"label":"<文本>"}
--
-- statSets 数字键 = vendor 模板 `#set` 行的导出顺序（= `<Gem statSetIndex>` 的索引
-- 语义）；set 的稳定 id 不在数据文件中，由 Rust 侧 join `Export/Skills/<file>.txt`
-- 模板（`#skill`/`#set` 行序）补齐。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <逗号分隔的技能文件名（不含 .lua 后缀）>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <file1,file2,...>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- 最小 stub 环境（与 extract_skill_overrides.lua 同形）
----------------------------------------------------------------------
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

----------------------------------------------------------------------
-- 逐文件加载并输出（按 --files 给定顺序；文件内按 statSets 数字键升序，
-- 整体确定性由 Rust 侧排序兜底）
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/Skills/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local skills = {}
	local ok, runErr = pcall(chunk, skills, makeSkillMod, makeFlagMod, makeSkillDataMod)
	if not ok then
		io.stderr:write("error executing " .. path .. ": " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
	for skillId, skillData in pairs(skills) do
		if type(skillData) == "table" and type(skillData.statSets) == "table" then
			for setIndex, set in ipairs(skillData.statSets) do
				if type(set) == "table" and type(set.label) == "string" then
					print(
						'{"skill":"'
							.. jsonEscape(skillId)
							.. '","set_index":'
							.. tostring(setIndex)
							.. ',"label":"'
							.. jsonEscape(set.label)
							.. '"}'
					)
				end
			end
		end
	end
end
