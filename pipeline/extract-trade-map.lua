-- 词条 → 官方 trade2 stat id 映射抽取（Trade 市集加权搜索用）。
--
-- 数据源都是 vendor 静态文件，不需要 PoB2 运行时：
--   Data/ModItem.lua       每条词缀自带 tradeHashes = { [数字hash] = {模板行...} }
--   Data/TradeSiteStats.lua 官方 trade2 可搜 stat 清单（校验 hash + 取官方文案）
-- 产物：{ templates: { "<数字骨架化模板行>": { id, text } } }
--
-- 用法：luajit pipeline/extract-trade-map.lua <vendor>/src <out.json>

local vendor_src, out_path = arg[1], arg[2]
assert(vendor_src and out_path, "usage: extract-trade-map.lua <vendor_src> <out.json>")

local trade = dofile((arg[0]:match("^(.*[/\\])") or "") .. "trade-stats.lua")
local valid = trade.load(vendor_src)

-- 数字骨架化：范围 (a-b) 与裸数字都归一成 #（与 web/src/lib/trade.ts 同规则）。
local function normalize(line)
    line = line:gsub("%(%-?%d+%.?%d*%-%-?%d+%.?%d*%)", "#")
    line = line:gsub("%-?%d+%.?%d*", "#")
    return line
end

local function json_escape(s)
    s = s:gsub("\\", "\\\\"):gsub('"', '\\"'):gsub("\n", "\\n"):gsub("\r", "")
    return s
end

local map, ambiguous = {}, {}
for _, source in ipairs({ "ModItem", "ModJewel", "ModFlask", "ModCharm", "ModVeiled" }) do
    for _, mod in pairs(dofile(vendor_src .. "/Data/" .. source .. ".lua")) do
        for _, stat in ipairs(trade.extract(mod, "explicit", valid)) do
            local template = normalize(stat.line)
            if map[template] and map[template].id ~= stat.id then ambiguous[template] = true end
            map[template] = { id = stat.id, text = valid[stat.id].text }
        end
    end
end
for template in pairs(ambiguous) do map[template] = nil end

local keys = {}
for k in pairs(map) do
    keys[#keys + 1] = k
end
table.sort(keys)

local parts = {}
parts[#parts + 1] = '{\n  "_meta": {\n    "source": "PoB2 item/jewel/flask/charm/desecrated tradeHashes x TradeSiteStats.lua (explicit)",\n    "regen_command": "luajit pipeline/extract-trade-map.lua vendor/PathOfBuilding-PoE2/src <out>"\n  },\n  "templates": {'
for i, k in ipairs(keys) do
    local e = map[k]
    parts[#parts + 1] = string.format(
        '    "%s": { "id": "%s", "text": "%s" }%s',
        json_escape(k),
        e.id,
        json_escape(e.text),
        i < #keys and "," or ""
    )
end
parts[#parts + 1] = "  }\n}"

local f = assert(io.open(out_path, "w"))
f:write(table.concat(parts, "\n"))
f:close()
print(string.format("trade map: %d unambiguous explicit templates", #keys))
