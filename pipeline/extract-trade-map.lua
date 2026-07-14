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

local stats = dofile(vendor_src .. "/Data/TradeSiteStats.lua")
-- explicit 域：hash → 官方展示文案。
local valid = {}
for _, group in ipairs(stats) do
    for _, e in ipairs(group.entries or {}) do
        if e.type == "explicit" then
            local h = e.id:match("^explicit%.stat_(%d+)$")
            if h then
                valid[tonumber(h)] = e.text
            end
        end
    end
end

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

local mods = dofile(vendor_src .. "/Data/ModItem.lua")
local map = {}
local skipped_multiline = 0
for _, mod in pairs(mods) do
    for hash, lines in pairs(mod.tradeHashes or {}) do
        if valid[hash] then
            if #lines == 1 then
                local tpl = normalize(lines[1])
                map[tpl] = map[tpl]
                    or {
                        id = string.format("explicit.stat_%.0f", hash),
                        text = valid[hash],
                    }
            else
                -- 混合词条（一 hash 多行）单行匹配语义不成立，v1 跳过。
                skipped_multiline = skipped_multiline + 1
            end
        end
    end
end

local keys = {}
for k in pairs(map) do
    keys[#keys + 1] = k
end
table.sort(keys)

local parts = {}
parts[#parts + 1] = '{\n  "_meta": {\n    "source": "vendor ModItem.lua tradeHashes x TradeSiteStats.lua (explicit)",\n    "regen_command": "luajit pipeline/extract-trade-map.lua vendor/PathOfBuilding-PoE2/src <out>"\n  },\n  "templates": {'
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
print(string.format("trade map: %d templates (skipped %d multi-line)", #keys, skipped_multiline))
