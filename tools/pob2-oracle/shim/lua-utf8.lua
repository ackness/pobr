-- Pure-Lua `lua-utf8` shim for running headless PoB2 on systems without the
-- native `luautf8` module (e.g. Linux CI boxes; PoB only vendors a Windows
-- `lua-utf8.dll`). Injected via LUA_PATH by run.sh — does NOT modify any vendor
-- source.
--
-- PoB2 uses `lua-utf8` only for number formatting (Common.lua `formatNumSep`
-- thousands separator) and the GUI text edit control (EditControl.lua, never
-- exercised in headless calc). All build data, mod text, and numbers are ASCII,
-- for which the luautf8 API is byte-for-byte identical to the standard `string`
-- library. So this shim maps the common surface to `string.*` (exact for ASCII)
-- and implements the few codepoint-oriented helpers (next/offset/char) for the
-- single-byte case. Multi-byte UTF-8 correctness is intentionally out of scope —
-- the oracle only ever feeds it ASCII.

local s = string
local m = setmetatable({}, { __index = s })

-- Byte-oriented passthroughs (exact for ASCII; luautf8 keeps these names).
m.byte = s.byte
m.find = s.find
m.format = s.format
m.gmatch = s.gmatch
m.gsub = s.gsub
m.len = s.len
m.lower = s.lower
m.match = s.match
m.rep = s.rep
m.reverse = s.reverse
m.sub = s.sub
m.upper = s.upper
m.char = s.char

-- codepoint(str, i, j): for ASCII a codepoint == its byte value.
m.codepoint = s.byte

-- Display/measurement helpers (ASCII: one column per byte).
function m.width(str)
	return #tostring(str)
end
function m.len_safe(str)
	return #tostring(str)
end

-- offset(str, n [, i]): byte index of the n-th char from byte i (default 1).
-- ASCII => one byte per char.
function m.offset(str, n, i)
	i = i or 1
	local pos = i + (n - (n >= 1 and 1 or 0))
	if pos < 1 or pos > #str + 1 then
		return nil
	end
	return pos
end

-- next(str, bytepos [, charoffset]): luautf8 cursor move. Returns the byte
-- position offset by `charoffset` chars from `bytepos` (defaults: bytepos=0,
-- charoffset=1). ASCII => +charoffset bytes. nil when out of range.
function m.next(str, bytepos, charoffset)
	bytepos = bytepos or 0
	charoffset = charoffset or 1
	local pos = bytepos + charoffset
	if pos < 0 or pos > #str + 1 then
		return nil
	end
	if charoffset == 0 then
		return pos
	end
	-- Match luautf8: also return the codepoint at the new position when moving
	-- forward into the string (callers in PoB only use the first return).
	if pos >= 1 and pos <= #str then
		return pos, s.byte(str, pos)
	end
	return pos
end

-- Expose as the conventional global too (Main.lua:287 checks `_G.utf8`).
if not _G.utf8 then
	_G.utf8 = m
end

return m
