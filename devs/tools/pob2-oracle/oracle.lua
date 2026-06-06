-- PoB2 headless oracle: load a build XML, dump golden calc breakdown.
-- Usage (from vendor/PathOfBuilding-PoE2/src):
--   luajit -e "package.path=package.path..';../runtime/lua/?.lua;../runtime/lua/?/init.lua'; \
--     BUILD='/abs/path/build.xml'; dofile('/abs/path/devs/tools/pob2-oracle/oracle.lua')"
-- Emits @@ lines to stdout (build loading noise goes to the same stream — grep '^@@').
dofile("HeadlessWrapper.lua")
local path = os.getenv("BUILD") or BUILD or error("set BUILD=<xml path>")
local fh = assert(io.open(path, "r")); local xml = fh:read("*a"); fh:close()
loadBuildFromXML(xml, "oracle"); runCallback("OnFrame")
local env = build.calcsTab.mainEnv
local out = env.player.output
local sk = env.player.mainSkill
local function p(...) print("@@", ...) end
p("SKILL", sk.activeEffect.grantedEffect.name)
for _, k in ipairs({ "AverageDamage", "TotalDPS", "Speed", "HitChance",
  "CritChance", "CritMultiplier", "ProjectileCount", "ManaCost" }) do
  p(k, out[k])
end
-- per-type final hit (StoredCombinedAvg) — the apples-to-apples per-component number
for _, t in ipairs({ "Physical", "Fire", "Cold", "Lightning", "Chaos" }) do
  p("HIT", t, out[t .. "StoredCombinedAvg"])
end
-- skill modDB increase/more (folds flag/keyword-scoped damage into generic "Damage")
local sml, cfg = sk.skillModList, sk.skillCfg
for _, n in ipairs({ "Damage", "PhysicalDamage", "FireDamage", "ColdDamage",
  "LightningDamage", "ElementalDamage", "ChaosDamage" }) do
  p("INC", n, sml:Sum("INC", cfg, n), "MORE", sml:More(cfg, n))
end
