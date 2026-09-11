# WeGame share import

Paste a `https://www.wegame.com.cn/helper/poe2/#/share/<code>` URL into
the Build import box. The resulting build uses the existing item editor,
skill groups, passive tree, item library and optimizers. It can be exported
as a PoBR session or a PoB2 build code.

The same-origin `POST /api/import/wegame` endpoint takes `{ "url": "..." }`.
It reads only the five public Profile endpoints used by WeGame's share page:
`GetRoleInfo`, `GetEquipments`, `GetTalentTree`, `GetJewels`, and `GetSkills`.
It requires no login and forwards no cookies or credentials. Account IDs,
character names and share tokens are excluded from the returned bundle.
Responses are not cached. An expired share, failed endpoint or malformed
payload fails the import instead of replacing the build with partial data.

`web/public/_worker.js` is a Cloudflare Pages advanced-mode worker, copied
into `dist/` by Vite and included in the existing deployment artifact.
Vite dev and preview run the same handler. Other static hosts must provide
this endpoint; the WASM calculator itself remains entirely local.
The returned `{ "format": "wegame", "version": 1, ... }` JSON can also be
saved locally and imported through the JSON/file input.

The Rust importer shares the existing `decode_build_file_json` entry point
with China-server `.build` files. It preserves character level/class,
equipment bases and modifiers, rolled defences, quality, rune sockets,
skill groups with gem levels/quality, passive attribute choices and socketed
tree jewels. Chinese text is canonicalized using the existing dictionary.
Actual quest rewards replace the default campaign rewards to avoid double
counting. Unknown gems, passives and unsupported slots appear in import notes.

Review the main skill, resistance penalty and combat configuration before
comparing upgrades. The share does not contain PoB combat configuration.
Only the first weapon set is used; inactive specialisations and weapon-swap
equipment are reported. Shaman-only rune bonuses and equipment-socketed
jewels require manual review. Chinese names absent from the dictionary are
reported when gem resolution fails; unsupported modifier lines retain the
existing calculation diagnostics. Imported data does not imply complete
support for every game mechanic.

For a quiver upgrade, select the intended bow skill, add candidate quivers
to the item library under the offhand (`weapon2`) slot, and run the item
optimizer with DPS as its objective. Every candidate is equipped and fully
recalculated. The trade page's weighted query is a preliminary search;
it does not fetch listings or prove a market-wide optimum within a budget.
Main-hand and offhand library candidates remain separate, so importing a
bow does not make it a candidate for the quiver slot.

Protocol reference: the public [WeGame PoE2 helper](https://www.wegame.com.cn/helper/poe2/)
and [PoB2 ImportTab.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/ImportTab.lua),
checked on 2026-09-11. Tests use synthetic data. The ignored local smoke
test accepts `POBR_WEGAME_FILE`; real player exports stay outside version control.
