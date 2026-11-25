# PoE2 Build Sources and Acquisition Formats

> **PoE2-Only Rule:** Ignore PoE1 completely. Never fetch from PoE1 `/poe1/`
> endpoints or PoE1 guide sections, and reject any decoded export whose XML
> root is `<PathOfBuilding>` instead of `<PathOfBuilding2>`.

When staging builds under `.cache/<batch>/builds/<build-id>/code.txt` for
`triage.py`, prefer PoE2 sources that provide **full PoB2 export codes**
(URL-safe Base64 + zlib `<PathOfBuilding2>` XML, often with `<PlayerStat>`
reference numbers inside).

## 1. Direct Raw PoB-Code Hosts (Already Supported by PoBR Web Importer)

These hosts are supported in [web/public/build-import.mjs](../../../../web/public/build-import.mjs)
with fixed, deterministic raw text endpoints (`HTTPS`, no credentials, `<= 1 MiB`):

| Host | Share URL Pattern | Raw Code Endpoint | Notes |
| --- | --- | --- | --- |
| `poe.ninja` / `poe2.ninja` | `https://poe.ninja/poe2/pob/<id>` | `https://poe.ninja/poe2/pob/raw/<id>` | Dedicated PoE2 PoB short links (distinct from character profile pages) |
| `maxroll.gg` | `https://maxroll.gg/poe2/pob/<id>` | `https://maxroll.gg/poe2/api/pob/<id>` | Dedicated PoE2 PoB hosting on Maxroll; gives full PoB2 XML code |
| `poe2db.tw` | `https://poe2db.tw/pob/<id>` | `https://poe2db.tw/pob/<id>/raw` | PoE2-only PoB paste endpoint (`id` `[\\w-]{1,128}`) |
| `pobb.in` | `https://pobb.in/<id>` | `https://pobb.in/pob/<id>` | Shared host (PoE1 + PoE2): decode and verify `<PathOfBuilding2>` root before keeping |
| `pastebin.com` / `pastebinp.com` | `https://pastebin.com/<id>` | `https://pastebin.com/raw/<id>` | Generic paste host: decode and verify `<PathOfBuilding2>` root before keeping |
| `rentry.co` | `https://rentry.co/<id>` | `https://rentry.co/paste/<id>/raw` | Generic paste host: decode and verify `<PathOfBuilding2>` root before keeping |

**Request hygiene:** Always send `Accept: text/plain` and a descriptive
`User-Agent: PoBR (+https://github.com/ackness/pobr)`. Validate both the
Base64 payload (`^[A-Za-z0-9+/_-]+={0,2}$`) and that the inflated XML root is
`<PathOfBuilding2>` before saving to `code.txt`.

## 2. Automated PoE2 Ladder / Index Acquisition (`poe.ninja` / `poe2.ninja`)

PoBR ships a PoE2-only automated collector ([devs/scripts/fetch-reference-builds.py](../../../../devs/scripts/fetch-reference-builds.py)
wrapping `web/scripts/fetch-ninja-benchmarks.py`):

```bash
mise run builds:fetch .cache/<batch> --pick first   # or --pick last
```

Underlying `/poe2/api/` endpoints (used by the script; deduplicates against
`examples/**/builds/*/meta.json`):

- **Index state**: `GET https://poe.ninja/poe2/api/data/build-index-state`
- **Search index** (protobuf): `GET https://poe.ninja/poe2/api/builds/<snapshot>/search?overview=<snapshotName>&class=<ascendancy>`
- **Character export** (JSON with `pathOfBuildingExport`): `GET https://poe.ninja/poe2/api/builds/<snapshot>/character?account=<account>&name=<character>&overview=<snapshotName>`

Character exports include exporter `<PlayerStat>` numbers alongside XML, making
them ideal for three-way (`pobr` vs `code` reference vs pinned `pob2`) triage.
Pace requests conservatively; if `poe.ninja` returns HTTP `429` or Cloudflare
HTML, stop scraping immediately.

## 3. PoE2 Build Guide Hubs & Aggregators (Extract Embedded PoB2 Links)

Use **PoE2-only** guide sections and filter out PoE1 entries; extract embedded
`pobb.in`, `maxroll.gg/poe2/pob/...`, `poe2db.tw/pob/...`, or raw PoB2 code
blocks (verifying `<PathOfBuilding2>` after decoding):

- **PoB Archives (PoE2 section)** (`https://pobarchives.com`): filter strictly
  to PoE2 builds and extract linked PoB2 URLs/codes.
- **PoE Vault (PoE2)** (`https://www.poe-vault.com/poe2`): written PoE2 guides
  with Path of Building (`pobb.in`) sections.
- **CN PoE2 Hubs** — **踩蘑菇 PoE2** (`https://poe2.caimogu.cc`), **暗黑核 PoE2**
  (`https://www.d2core.com/poe2`): PoE2 community guides embedding `pobb.in`,
  `poe2db.tw/pob/<id>`, raw PoB2 strings, or PoE2 WeGame share URLs.

## 4. Non-PoB Native Formats (Parity Triage Caveats)

- **WeGame 国服分享 (`https://www.wegame.com.cn/helper/poe2/#/share/<key>`)**:
  Resolved via `https://www.wegame.com.cn/api/v1/wegame.pallas.poe2.Profile/`
  in [web/public/_worker.js](../../../../web/public/_worker.js) and decoded by
  `crates/pobr-build`'s `wegame` codec. Useful for validating PoBR import and
  in-engine calculation directly, but it does **not** produce PoB XML or
  `<PlayerStat>` references for `triage.py` / `PoB2` oracle comparison directly.
- **Mobalytics (`https://mobalytics.gg/poe-2`) & Maxroll In-Game `.build` Files**:
  GGG's native `.build` JSON format exports passive trees, gem links, and notes
  without full rare item affixes or PoB `<PlayerStat>` totals. Prefer guides on
  Maxroll (`maxroll.gg/poe2/pob/...`) or `pobb.in` links when auditing numeric
  accuracy against PoB2.
