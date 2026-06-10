-- 测试夹具：模仿 vendor Data/Skills/*.lua 的 qualityStats 字段（extract-lua
-- --what gem-quality 抽取语义专用；与 mini.lua 同一注入约定）
local skills, mod, flag, skill = ...

-- 多条 qualityStats（保持 vendor 顺序：故意非字典序）
skills["MiniComet"] = {
	name = "Mini Comet",
	skillTypes = { [SkillType.Spell] = true, [SkillType.Cold] = true, },
	castTime = 1,
	qualityStats = {
		{ "base_spell_%_chance_to_echo", 0.5 },
		{ "aaa_extra_stat", 1.5 },
	},
	levels = {
		[1] = { critChance = 13, levelRequirement = 0, cost = { Mana = 17, }, },
	},
}

-- 空 qualityStats（导出时无对应行）：不应产生条目
skills["MiniEmpty"] = {
	name = "Mini Empty",
	skillTypes = { [SkillType.Spell] = true, },
	castTime = 0.8,
	qualityStats = {
	},
	levels = {
		[1] = { levelRequirement = 0, cost = { Mana = 5, }, },
	},
}

-- support 宝石效果：导出输出中本就无 qualityStats 字段（导出条件已生效），
-- 抽取忠实转录 = 不产生条目
skills["MiniSupport"] = {
	name = "Mini Support",
	support = true,
	requireSkillTypes = { SkillType.Spell, },
	addSkillTypes = { },
	excludeSkillTypes = { },
	levels = {
		[1] = { manaMultiplier = 20, levelRequirement = 0, },
	},
}
