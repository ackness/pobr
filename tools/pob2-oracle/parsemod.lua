-- PoB2 headless parseMod differential oracle (M5b Track D-1).
--
-- Bootstraps headless PoB2 (for modLib + data tables), reads modifier-text
-- lines from stdin (or --lines-file), runs `modLib.parseMod(line)` (two passes,
-- matching ModParser.lua:7116-7119 cache-closure order=1/2 semantics), and
-- emits one JSON object per line as JSONL to stdout:
--   {"line": "...", "mods": [{name,type,value,flags,keywordFlags,tags}], "unsupported": bool}
-- where flags/keywordFlags are dumped as bit-name arrays (reverse-looked-up
-- against the Global.lua ModFlag / KeywordFlag tables).
--
-- This is a pure wrapper: it does NOT modify any vendor source. Run from the
-- vendor src/ dir with LUA_PATH pointing at runtime/lua (see run-parsemod.sh).
--
-- Usage:
--   cd vendor/PathOfBuilding-PoE2/src
--   LUA_PATH="../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;" CI=true \
--     luajit ../../../tools/pob2-oracle/parsemod.lua [--lines-file <path>]
--
-- Lines come from stdin by default (one modifier per line); blank lines skipped.

local linesFile = nil
do
	local i = 1
	while arg[i] do
		if arg[i] == "--lines-file" then
			linesFile = arg[i + 1]
			i = i + 2
		else
			i = i + 1
		end
	end
end

-- Bootstrap headless PoB2 silently (swallow the noisy load prints).
local realPrint = print
_G.print = function() end
dofile("HeadlessWrapper.lua")
_G.print = realPrint

assert(type(modLib) == "table" and type(modLib.parseMod) == "function",
	"headless did not expose modLib.parseMod")

----------------------------------------------------------------------
-- Minimal deterministic JSON encoder (same shape as oracle.lua).
----------------------------------------------------------------------
local function isArray(t)
	local n = 0
	for k in pairs(t) do
		if type(k) ~= "number" then return false end
		n = n + 1
	end
	return n == #t
end

local function escape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
		return map[c] or string.format('\\u%04x', c:byte())
	end))
end

local encode
encode = function(v, depth)
	depth = depth or 0
	local t = type(v)
	if t == "number" then
		if v ~= v then return '"NaN"' end
		if v == math.huge then return '"Infinity"' end
		if v == -math.huge then return '"-Infinity"' end
		if v == math.floor(v) and math.abs(v) < 1e15 then
			return string.format("%d", v)
		end
		return string.format("%.10g", v)
	elseif t == "boolean" then
		return v and "true" or "false"
	elseif t == "string" then
		return '"' .. escape(v) .. '"'
	elseif t == "nil" then
		return "null"
	elseif t == "table" then
		if depth > 8 then return '"<max-depth>"' end
		if isArray(v) and #v > 0 then
			local parts = {}
			for i = 1, #v do parts[i] = encode(v[i], depth + 1) end
			return "[" .. table.concat(parts, ",") .. "]"
		end
		if next(v) == nil then return "[]" end
		local keys = {}
		for k in pairs(v) do keys[#keys + 1] = k end
		table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
		local parts = {}
		for _, k in ipairs(keys) do
			local val = v[k]
			local tv = type(val)
			if tv ~= "function" and tv ~= "userdata" and tv ~= "thread" then
				parts[#parts + 1] = '"' .. escape(tostring(k)) .. '":' .. encode(val, depth + 1)
			end
		end
		return "{" .. table.concat(parts, ",") .. "}"
	end
	return '"<' .. t .. '>"'
end

----------------------------------------------------------------------
-- Bit-name reverse lookup for ModFlag / KeywordFlag (Global.lua tables).
----------------------------------------------------------------------
local function flagNames(value, flagTable)
	local names = {}
	if not value or value == 0 then return names end
	for name, bit in pairs(flagTable) do
		-- skip composite masks (SourceMask etc.) by exact single-bit test is
		-- not possible; include any bit fully covered by value.
		if type(bit) == "number" and bit ~= 0 and bit % 1 == 0 then
			if bit > 0 and (value % (bit * 2)) >= bit then
				names[#names + 1] = name
			end
		end
	end
	table.sort(names)
	return names
end

----------------------------------------------------------------------
-- Normalize one parsed mod (PoB2 mod table) to the differential shape.
----------------------------------------------------------------------
local function normMod(m)
	local out = {
		name = m.name,
		type = m.type,
	}
	-- value: number / boolean / table (LIST payload)
	local tv = type(m.value)
	if tv == "number" or tv == "boolean" or tv == "string" then
		out.value = m.value
	elseif tv == "table" then
		out.value = m.value -- nested LIST payload; encoder handles tables
	end
	if m.flags and m.flags ~= 0 then
		out.flags = flagNames(m.flags, ModFlag)
	end
	if m.keywordFlags and m.keywordFlags ~= 0 then
		out.keywordFlags = flagNames(m.keywordFlags, KeywordFlag)
	end
	-- tags: m[1..n] hold tag tables { type=..., ... }
	local tags = {}
	for i = 1, #m do
		if type(m[i]) == "table" and m[i].type then
			tags[#tags + 1] = m[i]
		end
	end
	if #tags > 0 then out.tags = tags end
	return out
end

----------------------------------------------------------------------
-- Process one line: two-pass parseMod (order 1 then 2), matching the cache
-- closure semantics at ModParser.lua:7116-7119.
----------------------------------------------------------------------
local function processLine(line)
	local modList = modLib.parseMod(line)
	local mods = {}
	if modList then
		for _, m in ipairs(modList) do
			mods[#mods + 1] = normMod(m)
		end
	end
	return {
		line = line,
		mods = mods,
		unsupported = (modList == nil) or (#mods == 0),
	}
end

----------------------------------------------------------------------
-- Read lines (stdin or --lines-file) and emit JSONL.
----------------------------------------------------------------------
local input
if linesFile then
	input = assert(io.open(linesFile, "r"), "cannot open lines file: " .. linesFile)
else
	input = io.stdin
end

for line in input:lines() do
	local trimmed = line:gsub("^%s+", ""):gsub("%s+$", "")
	if #trimmed > 0 then
		io.write(encode(processLine(trimmed)), "\n")
	end
end

if linesFile then input:close() end
