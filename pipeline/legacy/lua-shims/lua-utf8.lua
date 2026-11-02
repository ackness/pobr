-- lua-utf8.lua — 纯 Lua 的 luautf8（starwing/luautf8）兼容垫片。
--
-- 背景：PoB2 0.21.0 起 Modules/Common.lua:29 硬 `require('lua-utf8')`（C 模块）。
-- vendor 只随包附带 Windows 的 runtime/lua-utf8.dll，macOS/Linux 下缺该 C 模块，
-- 导致 sync-pob-catalog 的完整 headless 抽取（config-options / parser-rules /
-- skill-overrides）在引导期整体失败（attempt to index global 'data' = nil，
-- 实为 Common.lua require 失败连锁）。
--
-- 本垫片仅供 **headless 数据抽取** 使用，覆盖 PoB 实际调用到的函数：
--   reverse / gsub / find / match / sub / next / len / char / byte /
--   codepoint / offset / charpattern。
-- PoB 在这些点的字符串都以 ASCII 为主、Lua pattern 全是 %d/%w/%s/%p/%. 等
-- ASCII 字符类（UTF-8 续字节 ≥0x80 不会落入这些类），故 find/match/gmatch/gsub
-- 直接委托 string.* 对这些用法等价且安全；按码点的 len/sub/reverse/next/offset/
-- codepoint 则做真正的 UTF-8 解码。
--
-- 不是完整 luautf8 实现（不覆盖 escape/insert/remove/width/title/fold/ncasecmp
-- 等 UI/排序专用 API）——这些在 headless 抽取路径上不会被调用。

local string = string
local s_byte, s_char, s_sub = string.byte, string.char, string.sub
local s_find, s_match, s_gmatch, s_gsub = string.find, string.match, string.gmatch, string.gsub
local floor = math.floor

local utf8 = {}

-- 单个 UTF-8 字符的 Lua pattern（与 luautf8.charpattern 等价）。
utf8.charpattern = "[\0-\127\194-\253][\128-\191]*"

-- 给定字节位置 i（1-based），返回该位置起首字节对应字符的字节长度。
local function char_len(b)
	if b < 0x80 then return 1
	elseif b < 0xE0 then return 2
	elseif b < 0xF0 then return 3
	else return 4 end
end

-- 解码 s 在字节位置 i 处的码点，返回 codepoint, 字节长度。
local function decode(s, i)
	local b = s_byte(s, i)
	if not b then return nil end
	local n = char_len(b)
	if n == 1 then return b, 1 end
	local cp
	if n == 2 then cp = b - 0xC0
	elseif n == 3 then cp = b - 0xE0
	else cp = b - 0xF0 end
	for k = 1, n - 1 do
		local cb = s_byte(s, i + k)
		if not cb then return b, 1 end -- 截断 → 按单字节兜底
		cp = cp * 64 + (cb - 0x80)
	end
	return cp, n
end

-- 码点 → UTF-8 字节串。
local function encode(cp)
	if cp < 0x80 then
		return s_char(cp)
	elseif cp < 0x800 then
		return s_char(0xC0 + floor(cp / 0x40), 0x80 + cp % 0x40)
	elseif cp < 0x10000 then
		return s_char(0xE0 + floor(cp / 0x1000), 0x80 + floor(cp / 0x40) % 0x40, 0x80 + cp % 0x40)
	else
		return s_char(0xF0 + floor(cp / 0x40000), 0x80 + floor(cp / 0x1000) % 0x40,
			0x80 + floor(cp / 0x40) % 0x40, 0x80 + cp % 0x40)
	end
end

-- 字节位置数组：codePositions[k] = 第 k 个字符的起始字节位置（1-based）。
local function code_positions(s)
	local pos, i, len = {}, 1, #s
	while i <= len do
		pos[#pos + 1] = i
		local _, n = decode(s, i)
		i = i + (n or 1)
	end
	return pos
end

function utf8.len(s, i, j)
	i = i or 1
	j = j or -1
	if i < 0 then i = #s + i + 1 end
	if j < 0 then j = #s + j + 1 end
	local count, p = 0, i
	while p <= j do
		local _, n = decode(s, p)
		if not n then break end
		count = count + 1
		p = p + n
	end
	return count
end

-- luautf8.byte / codepoint：返回区间内各字符码点（按字符索引）。
function utf8.codepoint(s, i, j)
	local pos = code_positions(s)
	local total = #pos
	i = i or 1
	j = j or i
	if i < 0 then i = total + i + 1 end
	if j < 0 then j = total + j + 1 end
	local out = {}
	for k = i, j do
		if pos[k] then
			local cp = decode(s, pos[k])
			out[#out + 1] = cp
		end
	end
	return table.unpack and table.unpack(out) or unpack(out)
end
utf8.byte = utf8.codepoint

function utf8.char(...)
	local parts = {}
	for _, cp in ipairs({ ... }) do
		parts[#parts + 1] = encode(cp)
	end
	return table.concat(parts)
end

-- 按字符索引取子串（i/j 为字符序号，支持负数）。
function utf8.sub(s, i, j)
	local pos = code_positions(s)
	local total = #pos
	i = i or 1
	j = j or -1
	if i < 0 then i = total + i + 1 end
	if j < 0 then j = total + j + 1 end
	if i < 1 then i = 1 end
	if j > total then j = total end
	if i > j then return "" end
	local startByte = pos[i]
	local endByte
	if j + 1 <= total then
		endByte = pos[j + 1] - 1
	else
		endByte = #s
	end
	return s_sub(s, startByte, endByte)
end

function utf8.reverse(s)
	local pos = code_positions(s)
	local out = {}
	for k = #pos, 1, -1 do
		local startByte = pos[k]
		local endByte = (pos[k + 1] or (#s + 1)) - 1
		out[#out + 1] = s_sub(s, startByte, endByte)
	end
	return table.concat(out)
end

-- luautf8.offset(s, n, i)：从字节位置 i 起第 n 个字符的字节位置。
function utf8.offset(s, n, i)
	local len = #s
	if i == nil then
		i = (n >= 0) and 1 or (len + 1)
	end
	local p = i
	if n > 0 then
		n = n - 1
		while n > 0 and p <= len do
			local _, sz = decode(s, p)
			p = p + (sz or 1)
			n = n - 1
		end
		return p <= len + 1 and p or nil
	elseif n < 0 then
		while n < 0 and p > 1 do
			p = p - 1
			while p > 1 and s_byte(s, p) >= 0x80 and s_byte(s, p) < 0xC0 do
				p = p - 1
			end
			n = n + 1
		end
		return p
	else
		while p > 1 and s_byte(s, p) >= 0x80 and s_byte(s, p) < 0xC0 do
			p = p - 1
		end
		return p
	end
end

-- PoB 用法：utf8.next(s, pos, dir) —— 返回 pos 相邻（dir=±1）字符的字节位置。
-- 兼容 luautf8 的迭代器签名 utf8.next(s, [bytepos]) 同样可用（dir 缺省 +1）。
function utf8.next(s, pos, dir)
	pos = pos or 0
	dir = dir or 1
	if dir >= 0 then
		if pos < 1 then return 1 end
		if pos > #s then return nil end
		local _, n = decode(s, pos)
		local np = pos + (n or 1)
		return np
	else
		if pos <= 1 then return nil end
		local p = pos - 1
		while p > 1 and s_byte(s, p) >= 0x80 and s_byte(s, p) < 0xC0 do
			p = p - 1
		end
		return p
	end
end

-- find/match/gmatch/gsub/lower/upper：PoB 在抽取路径上的 pattern 均为 ASCII 字符类，
-- 委托 string.* 等价且安全（见文件头说明）。
utf8.find = s_find
utf8.match = s_match
utf8.gmatch = s_gmatch
utf8.gsub = s_gsub
utf8.lower = string.lower
utf8.upper = string.upper

return utf8
