-- extract_gem_quality.lua — sync-pob-catalog `extract-lua --what gem-quality` 的引导脚本
--
-- 在与 extract_skill_overrides.lua 相同的**最小 stub 环境**下加载 vendor
-- Data/Skills/<file>.lua，抽取每个技能的 `qualityStats` 字段（宝石品质 stat 斜率）
-- 并以 JSONL（每行一个 JSON 对象）写到 stdout：
--
--   {"effect":"<效果 id>","stat":"<stat id>","rate":<每 1 品质斜率>}
--
-- 数据语义（对齐 PoB2 Export/Scripts/skills.lua:304-313）：
-- - vendor 数据文件里的 qualityStats 已是导出产物：rate = `StatValues[i]/1000`
--   已除好、辅助宝石（skillGem 且 IsSupport）已按导出条件跳过——本脚本**忠实转录**
--   导出输出，不重复施加任何条件（少数 support=true 但非宝石的效果在导出输出中
--   本就带 qualityStats，照实保留）。
-- - 空 qualityStats（导出时无对应 GrantedEffectQualityStats 行）自然产出 0 行。
--
-- 确定性约定：本脚本只负责"忠实抽取 + 合法 JSON"；最终排序（effect_id 升序，
-- 效果内保持 vendor 顺序）、数字格式与整体文档（_meta + effects）的 byte-stable
-- 序列化由 Rust 侧（extract_quality.rs）统一完成。
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
-- 最小 stub 环境（与 extract_skill_overrides.lua 同形）：值的具体语义无关紧要，
-- __index 自映射即可让数据文件顶层求值通过。
----------------------------------------------------------------------
SkillType = setmetatable({}, { __index = function(_, k) return k end })
-- 惰性数字位枚举：新 vendor 在技能数据顶层就调 bit.bor(ModFlag.X, ...)，
-- 字符串 stub 会炸（number expected）；值只需互异、不进产物。
local function flagEnum()
	local nextBit = 1
	return setmetatable({}, { __index = function(t, k)
		local v = nextBit
		nextBit = nextBit * 2
		rawset(t, k, v)
		return v
	end })
end
ModFlag = flagEnum()
KeywordFlag = flagEnum()

-- 与 vendor Modules/Data.lua 的 makeSkillMod/makeFlagMod/makeSkillDataMod 同形：
-- mod(...) 调用捕获为纯表（本脚本不消费 mods，仅保证文件可执行）。
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
-- 加载目标数据文件（vendor 只读，不做任何修改）
----------------------------------------------------------------------
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
		error("non-finite number in qualityStats")
	end
	return string.format("%.17g", v)
end

----------------------------------------------------------------------
-- 抽取并输出 JSONL（pairs 顺序不确定没关系：Rust 侧按 effect_id 排序，
-- 单个效果内的行经 ipairs 保持 vendor 顺序且在输出中天然连续成组）
----------------------------------------------------------------------
for skillId, skill in pairs(skills) do
	if type(skill) == "table" and type(skill.qualityStats) == "table" then
		for _, qs in ipairs(skill.qualityStats) do
			if type(qs) == "table" and type(qs[1]) == "string" and type(qs[2]) == "number" then
				print(
					'{"effect":"'
						.. jsonEscape(skillId)
						.. '","stat":"'
						.. jsonEscape(qs[1])
						.. '","rate":'
						.. jsonNum(qs[2])
						.. "}"
				)
			end
		end
	end
end
