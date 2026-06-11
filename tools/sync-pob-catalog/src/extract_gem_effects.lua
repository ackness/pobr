-- extract_gem_effects.lua — sync-pob-catalog `extract-lua --what gem-effects` 的引导脚本
--
-- 加载 vendor `Data/Gems.lua`（`return { ["<变体键>"] = { gameId=..., variantId=...,
-- grantedEffectId=..., additionalGrantedEffectId1..N, additionalStatSet1..N, ... } }`，
-- 由 PoB2 `Export/Scripts/skills.lua:898-925` 自 `.dat` `SkillGems.GemEffects` →
-- `GemEffects` 表导出），抽取宝石→授予效果连边并以 JSONL 写到 stdout：
--
--   {"gem_id":"<gameId>","variant_id":"<variantId>","granted_effect":"<grantedEffectId>",
--    "additional_granted_effects":[...],"additional_stat_sets":[...]}
--
-- 数据语义（对齐导出脚本）：
-- - `additionalGrantedEffectId1..N` / `additionalStatSet1..N` 是带序号的散列字段，
--   按 1..N 连续收集（导出端 ipairs 写出，序号必然连续）；
-- - 无 grantedEffectId 的条目（不应存在）跳过并经 stderr 告警。
--
-- 确定性约定：本脚本只负责忠实抽取 + 合法 JSON；最终排序（gem_id 升序）与整体
-- 文档（_meta + gems）的 byte-stable 序列化由 Rust 侧（extract_gem_effects.rs）完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <ignored>
--   （第二参数为公共调用层的文件名约定位，本目标恒读 Data/Gems.lua，忽略之）

local vendorSrc = arg and arg[1]
if not vendorSrc then
	io.stderr:write("usage: luajit - <vendor_src_dir> <ignored>\n")
	os.exit(2)
end

-- Gems.lua 是纯数据文件（return { ... }，无 SkillType/mod 构造器引用），直接加载。
local path = vendorSrc .. "/Data/Gems.lua"
local chunk, loadErr = loadfile(path)
if not chunk then
	io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
	os.exit(3)
end
local ok, gems = pcall(chunk)
if not ok or type(gems) ~= "table" then
	io.stderr:write("error executing " .. path .. ": " .. tostring(gems) .. "\n")
	os.exit(3)
end

----------------------------------------------------------------------
-- JSON 工具（仅需字符串转义；本目标全部字段为字符串）
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local function jsonStringArray(list)
	local parts = {}
	for i, v in ipairs(list) do
		parts[i] = '"' .. jsonEscape(v) .. '"'
	end
	return "[" .. table.concat(parts, ",") .. "]"
end

-- 收集带序号的散列字段 `<prefix>1..N`（序号连续，遇 nil 即止）。
local function collectNumbered(entry, prefix)
	local list = {}
	local i = 1
	while type(entry[prefix .. i]) == "string" do
		list[i] = entry[prefix .. i]
		i = i + 1
	end
	return list
end

----------------------------------------------------------------------
-- 抽取并输出 JSONL（pairs 顺序不确定没关系：Rust 侧按 gem_id 排序）
----------------------------------------------------------------------
for variantKey, gem in pairs(gems) do
	if type(gem) == "table" then
		local gameId = gem.gameId
		local variantId = gem.variantId
		local granted = gem.grantedEffectId
		if type(gameId) == "string" and type(variantId) == "string" and type(granted) == "string" then
			print(
				'{"gem_id":"'
					.. jsonEscape(gameId)
					.. '","variant_id":"'
					.. jsonEscape(variantId)
					.. '","granted_effect":"'
					.. jsonEscape(granted)
					.. '","additional_granted_effects":'
					.. jsonStringArray(collectNumbered(gem, "additionalGrantedEffectId"))
					.. ',"additional_stat_sets":'
					.. jsonStringArray(collectNumbered(gem, "additionalStatSet"))
					.. "}"
			)
		else
			io.stderr:write("gem entry missing gameId/variantId/grantedEffectId: " .. tostring(variantKey) .. "\n")
		end
	end
end
