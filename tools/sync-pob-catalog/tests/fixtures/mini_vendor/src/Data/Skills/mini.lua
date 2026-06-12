-- 测试夹具：模仿 vendor Data/Skills/*.lua 的最小结构（与真实文件同一注入约定）
local skills, mod, flag, skill = ...

-- 全等级同值的 baseMultiplier（触发压缩为单 value）+ 含 statSets baseMods Speed MORE
-- （模仿 FlickerStrikePlayer；M1-T4.3 后 attackSpeedMultiplier/critChance 不再抽取，
-- 字段保留在夹具里用于验证「非目标字段被忽略」）
skills["MiniFlicker"] = {
	name = "Mini Flicker",
	skillTypes = { [SkillType.Attack] = true, [SkillType.Melee] = true, },
	castTime = 1,
	levels = {
		[1] = { attackSpeedMultiplier = -50, baseMultiplier = 1.2, levelRequirement = 0, cost = { Mana = 12, }, },
		[2] = { attackSpeedMultiplier = -50, baseMultiplier = 1.2, levelRequirement = 3, cost = { Mana = 14, }, },
		[3] = { attackSpeedMultiplier = -50, baseMultiplier = 1.2, levelRequirement = 6, cost = { Mana = 16, }, },
	},
	statSets = {
		[1] = {
			label = "Mini Flicker",
			baseFlags = {
				attack = true,
			},
			baseMods = {
				mod("Speed", "MORE", 285, ModFlag.Attack),
				skill("dotIsArea", true), -- M4-T4 W-D1：dotIs* 布尔（statSet 级）
				skill("radius", 18), -- 非布尔 skillData：不属抽取目标，应被忽略
			},
		},
	},
}

-- 等级间变化的 baseMultiplier（触发 per_level 表示）；critChance 为非目标字段（不抽）
skills["MiniArc"] = {
	name = "Mini Arc",
	skillTypes = { [SkillType.Spell] = true, [SkillType.Lightning] = true, },
	castTime = 1.1,
	levels = {
		[1] = { critChance = 9, baseMultiplier = 2.0, levelRequirement = 0, cost = { Mana = 8, }, },
		[2] = { critChance = 9.5, baseMultiplier = 2.65, levelRequirement = 3, cost = { Mana = 9, }, },
	},
	statSets = {
		[1] = {
			label = "Mini Arc",
			statMap = {
				["mini_arc_dummy_stat"] = {
					mod("Speed", "MORE", nil, ModFlag.Cast), -- value 为 nil：不应被抽取
				},
			},
		},
	},
}

-- 无目标字段的技能：不应产生任何条目
skills["MiniPlain"] = {
	name = "Mini Plain",
	skillTypes = { [SkillType.Spell] = true, },
	castTime = 0.8,
	levels = {
		[1] = { levelRequirement = 0, cost = { Mana = 5, }, },
	},
}
