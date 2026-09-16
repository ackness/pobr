-- Export rollable affixes and base categories for trade/combination search.
-- Source: PoB2 TradeQueryGenerator.canModSpawnForItemCategory / ModItem / Bases.
local vendor, output, crafting_path = arg[1], arg[2], arg[3]
assert(vendor and output, "usage: extract-trade-catalog.lua <vendor_src> <out.json> [crafting_sources.json]")
package.path = vendor .. "/../runtime/lua/?.lua;" .. package.path
local json = require("dkjson")
local trade = dofile((arg[0]:match("^(.*[/\\])") or "") .. "trade-stats.lua")
local valid_stats = trade.load(vendor)

local function sorted_keys(t)
    local keys = {}
    for key in pairs(t) do keys[#keys + 1] = key end
    table.sort(keys)
    return keys
end

local maximum = trade.maximum

local categories = {
    ["Amulet"] = "accessory.amulet", ["Ring"] = "accessory.ring", ["Belt"] = "accessory.belt",
    ["Body Armour"] = "armour.chest", ["Helmet"] = "armour.helmet", ["Gloves"] = "armour.gloves",
    ["Boots"] = "armour.boots", ["Quiver"] = "armour.quiver", ["Shield"] = "armour.shield",
    ["Focus"] = "armour.focus", ["Buckler"] = "armour.buckler", ["Bow"] = "weapon.bow",
    ["Crossbow"] = "weapon.crossbow", ["Staff"] = "weapon.staff", ["Talisman"] = "weapon.talisman",
    ["One Hand Mace"] = "weapon.onemace", ["Two Hand Mace"] = "weapon.twomace",
    ["Wand"] = "weapon.wand", ["Sceptre"] = "weapon.sceptre", ["Spear"] = "weapon.spear",
    ["Flail"] = "weapon.flail", ["Jewel"] = "jewel", ["Charm"] = "flask.charm",
}
local raw_bases = {}
for _, file in ipairs({ "amulet", "belt", "body", "boots", "bow", "crossbow", "flail", "focus",
    "gloves", "helmet", "mace", "quiver", "ring", "sceptre", "shield", "spear", "staff", "talisman", "wand", "jewel", "flask" }) do
    dofile(vendor .. "/Data/Bases/" .. file .. ".lua")(raw_bases)
end
local bases = {}
for _, name in ipairs(sorted_keys(raw_bases)) do
    local base = raw_bases[name]
    local category = categories[base.type]
    if base.type == "Staff" and base.subType == "Warstaff" then category = "weapon.warstaff" end
    if base.type == "Flask" then category = base.subType == "Life" and "flask.life" or "flask.mana" end
    local domain = base.type == "Jewel" and "jewel" or base.type == "Charm" and "charm"
        or base.type == "Flask" and "flask" or "equipment"
    if category and not base.hidden and (not (base.tags or {}).not_for_sale or base.subType == "Radius") then
        local implicits = {}
        for line in (base.implicit or ""):gmatch("[^\n]+") do implicits[#implicits + 1] = maximum(line) end
        bases[#bases + 1] = { name = name, category = category, tags = sorted_keys(base.tags or {}),
            level = (base.req or {}).level or 1, implicits = implicits, domain = domain,
            affix_limit = domain == "equipment" and 3 or domain == "jewel" and 2 or 1,
            radius = base.subType == "Radius" and "Small" or nil }
    end
end
-- PoB2 Data.lua decorates radius grants after loading the raw ModJewel table.
local function radius_lines(mod)
    local kind = ({ [1] = "Small", [2] = "Notable" })[mod.nodeType]
    if kind and mod[1] and not mod[1]:find("Passive Skills in Radius", 1, true) then
        mod[1] = kind .. " Passive Skills in Radius also grant " .. mod[1]
    end
    return mod
end
local raw_mods = {}
for _, source in ipairs({ { "equipment", "ModItem" }, { "jewel", "ModJewel" }, { "flask", "ModFlask" }, { "charm", "ModCharm" } }) do
    for id, mod in pairs(dofile(vendor .. "/Data/" .. source[2] .. ".lua")) do
        mod.domain = source[1]
        raw_mods[source[1] .. ":" .. id] = radius_lines(mod)
    end
end
local mods = {}
for _, id in ipairs(sorted_keys(raw_mods)) do
    local mod = raw_mods[id]
    if (mod.type == "Prefix" or mod.type == "Suffix") and mod.group and #(mod.weightKey or {}) > 0 then
        local lines, roll_lines, weights, stats = {}, {}, {}, {}
        for _, line in ipairs(mod) do
            lines[#lines + 1] = maximum(line)
            roll_lines[#roll_lines + 1] = line
        end
        for i, tag in ipairs(mod.weightKey) do weights[#weights + 1] = {tag, mod.weightVal[i]} end
        stats = trade.extract(mod, "explicit", valid_stats)
        mods[#mods + 1] = { id = id, group = mod.group, kind = mod.type:lower(), level = mod.level or 1,
            lines = lines, roll_lines = roll_lines, weights = weights, stats = stats, domain = mod.domain }
    end
end
-- Searchable special sources do not imply an ordinary rollable affix combination.
-- Follow TradeQueryGenerator.InitMods category overrides for non-spawning crafts.
local search_mods, special = {}, {}
local function add_search(source, id, mod, allowed, namespace, recovered_stats)
    if not mod or #allowed == 0 then return end
    radius_lines(mod)
    local stats = recovered_stats or trade.extract(mod, namespace or "explicit", valid_stats)
    if #stats == 0 then return end
    local key = source .. ":" .. id
    if not special[key] then
        local lines = {}
        for _, line in ipairs(mod) do lines[#lines + 1] = maximum(line) end
        local jewel_types = {}
        for _, base in ipairs(bases) do
            if base.category == "jewel" then
                local tags = {}; for _, tag in ipairs(base.tags) do tags[tag] = true end
                for i, tag in ipairs(mod.weightKey or {}) do
                    if tags[tag] then
                        if mod.weightVal[i] > 0 then jewel_types[base.radius and "radius" or "base"] = true end
                        break
                    end
                end
            end
        end
        special[key] = { id = key, source = source, level = mod.level or 1, lines = lines, stats = stats, categories = {},
            jewel_types = next(jewel_types) and sorted_keys(jewel_types) or nil }
    end
    for _, category in ipairs(allowed) do special[key].categories[category] = true end
end
local function spawning_categories(mod)
    local allowed = {}
    for _, base in ipairs(bases) do
        local tags = {}; for _, tag in ipairs(base.tags) do tags[tag] = true end
        for i, tag in ipairs(mod.weightKey or {}) do
            if tags[tag] then
                if mod.weightVal[i] > 0 then allowed[base.category] = true end
                break
            end
        end
    end
    return sorted_keys(allowed)
end
for _, source in ipairs({ { "desecrated", "ModVeiled", "explicit" }, { "corrupted", "ModCorrupted", "enchant" } }) do
    for id, mod in pairs(dofile(vendor .. "/Data/" .. source[2] .. ".lua")) do
        if mod.type == "Prefix" or mod.type == "Suffix" or mod.type == "Corrupted" or mod.type == "SpecialCorrupted" then
            add_search(source[1], id, mod, spawning_categories(mod), source[3])
        end
    end
end
for name, essence in pairs(dofile(vendor .. "/Data/Essence.lua")) do
    if name:find("Perfect") and not name:find("PerfectEssenceAttribute$") then
        for item_type, id in pairs(essence.mods or {}) do
            local category = item_type == "Warstaff" and "weapon.warstaff" or categories[item_type]
            if category then add_search("essence", id, raw_mods["equipment:" .. id], { category }) end
        end
    end
end
for _, id in ipairs({ "EssencePercentStrength1", "EssencePercentDexterity1", "EssencePercentIntelligence1" }) do
    add_search("essence", id, raw_mods["equipment:" .. id], { "accessory.amulet" })
end
local influences = {
    chronomancy = { "armour.boots" }, marksman = { "armour.gloves" }, decay = { "armour.gloves" },
    berserking = { "armour.helmet" }, soul = { "armour.chest" },
    destruction = { "weapon.onemace", "weapon.twomace", "weapon.warstaff", "weapon.bow", "weapon.crossbow",
        "weapon.spear", "weapon.flail", "weapon.talisman" },
}
for key, mod in pairs(raw_mods) do
    local id = key:match("^equipment:(.+)$")
    if id then
        local breach, slots = false, { ring = true, belt = true }
        for i, tag in ipairs(mod.weightKey or {}) do
            if (tag == "genesis_tree_minion" or tag == "genesis_tree_caster") and mod.weightVal[i] > 0 then breach = true end
            if slots[tag] and mod.weightVal[i] == 0 then slots[tag] = nil end
            if influences[tag] and mod.weightVal[i] > 0 then add_search("influence", id, mod, influences[tag]) end
        end
        if breach then
            local allowed = {}; for slot in pairs(slots) do allowed[#allowed + 1] = "accessory." .. slot end
            add_search("breach", id, mod, allowed)
        else
            local slot = id:match("^GenesisTree(Ring)") or id:match("^GenesisTree(Belt)") or id:match("^GenesisTree(Amulet)")
            if slot then add_search("breach", id, mod, { "accessory." .. slot:lower() }) end
        end
    end
end
local unmapped_crafting_mods = setmetatable({}, { __jsontype = "array" })
if crafting_path then
    local file = assert(io.open(crafting_path, "r"))
    local crafting = assert(json.decode(file:read("*a"))); file:close()
    local veiled = dofile(vendor .. "/Data/ModVeiled.lua")
    local recover = dofile((arg[0]:match("^(.*[/\\])") or "") .. "trade-crafting-fallback.lua").load(vendor, valid_stats, trade)
    for _, recipe in ipairs(crafting.mods) do
        local allowed = {}
        for _, item_type in ipairs(recipe.item_types) do
            local category = item_type == "Warstaff" and "weapon.warstaff" or categories[item_type]
            if category then allowed[#allowed + 1] = category end
        end
        local mod = raw_mods["equipment:" .. recipe.id] or veiled[recipe.id]
        local recovered_stats = not mod and recover(recipe)
        if recovered_stats and #recovered_stats > 0 then
            mod = { level = recipe.level }
            for _, stat in ipairs(recovered_stats) do mod[#mod + 1] = stat.line end
        end
        if not mod and #allowed > 0 then
            unmapped_crafting_mods[#unmapped_crafting_mods + 1] = recipe.id
            io.stderr:write("trade catalog: no pinned PoB2 text/hash for crafting mod " .. recipe.id .. "\n")
        end
        special[recipe.source .. ":" .. recipe.id] = nil
        add_search(recipe.source, recipe.id, mod, allowed, "explicit", recovered_stats)
    end
end
for _, key in ipairs(sorted_keys(special)) do
    special[key].categories = sorted_keys(special[key].categories)
    search_mods[#search_mods + 1] = special[key]
end
-- Load data constructors for level requirements and support compatibility.
-- The same minimal enum stubs are used by sync-pob-catalog's skill extractors.
SkillType = setmetatable({}, { __index = function(_, key) return key end })
local function flag_enum()
    local next_bit = 1
    return setmetatable({}, { __index = function(t, key)
        local value = next_bit
        next_bit = next_bit * 2
        rawset(t, key, value)
        return value
    end })
end
ModFlag, KeywordFlag = flag_enum(), flag_enum()
local raw_skills = {}
local function unused_mod() return {} end
for _, file in ipairs({ "act_dex", "act_int", "act_str", "sup_dex", "sup_int", "sup_str" }) do
    local constructor = assert(loadfile(vendor .. "/Data/Skills/" .. file .. ".lua"))
    local result = constructor(raw_skills, unused_mod, unused_mod, unused_mod)
    if type(result) == "function" then result(raw_skills, unused_mod, unused_mod, unused_mod) end
end
local gems = {}
local raw_gems = dofile(vendor .. "/Data/Gems.lua")
for _, id in ipairs(sorted_keys(raw_gems)) do
    local gem = raw_gems[id]
    if gem.grantedEffectId and gem.name then
        local requirements = {}
        local effect = raw_skills[gem.grantedEffectId]
        local levels = (effect or {}).levels or {}
        local maximum_level = (gem.naturalMaxLevel or 20) + (gem.gemType == "Support" and 0 or 1)
        for level = 1, maximum_level do
            if not levels[level] or levels[level].levelRequirement == nil then break end
            requirements[level] = levels[level].levelRequirement
        end
        gems[#gems + 1] = { skill_id = gem.grantedEffectId, name = gem.name, family = gem.gemFamily or gem.name,
            is_support = gem.gemType == "Support", max_level = gem.naturalMaxLevel or 20,
            level_requirements = requirements,
            is_lineage = (gem.tags or {}).lineage == true,
            compatibility_known = effect ~= nil,
            skill_types = sorted_keys((effect or {}).skillTypes or {}),
            require_skill_types = (effect or {}).requireSkillTypes or {},
            exclude_skill_types = (effect or {}).excludeSkillTypes or {},
            add_skill_types = (effect or {}).addSkillTypes or {},
            support_gems_only = (effect or {}).supportGemsOnly == true,
            cannot_be_supported = (effect or {}).cannotBeSupported == true,
            families = (effect or {}).gemFamily or { gem.gemFamily or gem.name } }
    end
end
local result = { _meta = { source = "PoB2 ModItem/ModJewel/ModFlask/ModCharm/ModVeiled/ModCorrupted, Essence, Bases, Gems, Skills and TradeSiteStats"
        .. (crafting_path and "; GGG trade_crafting_sources" or ""),
    unmapped_crafting_mods = unmapped_crafting_mods,
    regen_command = "luajit pipeline/extract-trade-catalog.lua vendor/PathOfBuilding-PoE2/src <out> <crafting_sources>" },
    bases = bases, mods = mods, search_mods = search_mods, gems = gems }
-- This vendored dkjson tests truthiness when honoring keyorder. Wrap false only
-- for serialization so boolean fields keep deterministic order across processes.
local json_false = setmetatable({}, { __tojson = function() return "false" end })
for _, gem in ipairs(gems) do
    for key, value in pairs(gem) do
        if value == false then gem[key] = json_false end
    end
end
local f = assert(io.open(output, "w"))
f:write(json.encode(result, { indent = true, keyorder = {
    "_meta", "source", "regen_command", "unmapped_crafting_mods", "bases", "mods", "search_mods", "gems", "id", "name", "category", "categories", "jewel_types", "tags",
    "group", "kind", "level", "implicits", "domain", "affix_limit", "radius", "lines", "roll_lines", "weights", "stats", "line", "value", "trade_line", "source_lines", "value_indices",
    "skill_id", "family", "is_support", "max_level", "level_requirements", "is_lineage",
    "compatibility_known", "skill_types", "require_skill_types", "exclude_skill_types", "add_skill_types",
    "support_gems_only", "cannot_be_supported", "families",
} }), "\n")
f:close()
print(string.format("trade catalog: %d bases, %d affixes, %d special search mods", #bases, #mods, #search_mods))
