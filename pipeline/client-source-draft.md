# Unreviewed first acquisition

`client_source_draft.py` fills the stage before [strict source-lock replay](client-sources.md):
discovery evidence → **unreviewed acquisition draft** → reviewed source-lock
promotion and strict replay → separate production adoption. It does not discover
versions, scan indexes, schedule updates, promote locks or adopt data. Existing
callers can invoke this command for each explicitly located selection.

[Fixed index discovery](client-index.md) can produce the location-only selection
from an explicit patch and path list. Its evidence remains unreviewed; this
collector preserves that claim without independently repeating discovery.

```sh
mise run data:client:draft -- \
  --selection .cache/new-source/selection.json \
  --cache .cache/new-source/cache --out .cache/new-source/raw \
  --decoder-python /absolute/repoe/.venv/bin/python \
  --decoder-lock /absolute/repoe/uv.lock --mode offline
```

Online mode requires `--mode online --network-budget-bytes <bytes>` (1–67,108,864 bytes).
Offline mode requires a zero budget and never constructs an HTTP opener. Both
modes reuse `RangeCache`, `Ranges`, `Bundle` and `ExternalDecoder`, including the
existing header/framing checks, immutable cache bounds, cumulative reservation
budget, request/timeout/decoder resource limits and no retries. No second
downloader, index reader, raw staging directory or implicit environment install
is involved. Use local directories without concurrent writers; this is not an
adversarial filesystem sandbox.

The UTF-8 selection schema is `pobr-client-source-selection/v1`. All fields shown
below are required; extra fields are rejected at every object level. The hashes
in this example fingerprint **evidence and the decoder lock**, never expected raw
game resources. Replace placeholders with the caller's actual fingerprints.

```json
{
  "schema": "pobr-client-source-selection/v1",
  "game": "poe2",
  "realm": "international",
  "game_patch": "0.5.5.5",
  "base_url": "https://patch-poe2.poecdn.com/0.5.5.5/",
  "decoder": {
    "repository": "https://github.com/repoe-fork/pypoe",
    "revision": "<full 40-character lowercase Git revision>",
    "pyooz_version": "<locked version>",
    "uv_lock_sha256": "<64-character lowercase SHA-256>"
  },
  "location_evidence": [
    {
      "id": "index-records",
      "reference": "research/index-selection.json#selected-records",
      "sha256": "<SHA-256 of referenced evidence file, without the fragment>"
    }
  ],
  "resources": {
    "Data/Balance/Example.datc64": {
      "bundle_url": "https://patch-poe2.poecdn.com/0.5.5.5/Bundles2/Example.bundle.bin",
      "uncompressed_offset": 123,
      "uncompressed_length": 456,
      "location_evidence": ["index-records"]
    }
  }
}
```

Only explicit numeric patches on the fixed official PoE2 HTTPS host and exact
`Bundles2/*.bundle.bin` URLs are accepted; queries, fragments and redirects fail.
Realm is a caller declaration, constrained to `international` for this host.
Locations are **caller-supplied, not independently checked**: evidence references
are opaque strings, never fetched/read, and their fingerprints are recorded
claims. Even if an earlier discovery process checked an index, this collector
does not claim that check. Preserve the original evidence separately.

Resources retain JSON insertion order. Each needs a safe portable game-relative
`Data/` or `Metadata/` path, exact uncompressed offset/length and one or more
unique evidence IDs. Symlinks, path traversal, case collisions and file/directory
collisions fail. Internal spaces in locale directory names such as
`Traditional Chinese` are preserved; trailing spaces/dots remain invalid.
Limits are 2 MiB selection JSON, 128 resources/evidence entries,
32 MiB per resource and 64 MiB selected raw total; the fixed resource cap permits
the 30,380,428-byte `stat_descriptions.csd` in patch 0.5.5.5. It has no caller
override; HTTP budgets and bundle/cache/decoder/worker limits remain those of
strict acquisition. Decoder repository/revision, pyooz version
and uv.lock hash are declared pins, checked using the explicit environment.

The output root must be absent and separate from production `data/`, cache and
source/decoder inputs. Raw bytes go directly to their standard game paths.
`source-selection.json` preserves the **exact input bytes**. Each decoded resource
must have the selected indexed length; its `observed_bytes` and `observed_sha256`
are then computed. These hashes are observations, **not verification against
pre-existing trusted raw pins**. Resource records must omit `sha256`, `bytes`,
`expected_sha256` and other undeclared fields. When converting an older research
draft, retain its path/hash as location evidence and copy only URL/offset/length
into resource records; do not turn its observed raw hashes into expected pins.

`draft-receipt.json` is written last with schema `pobr-client-source-draft/v1`,
status `unreviewed-acquisition-draft` and `production_ready: false`. It records
selection bytes/hash, ordered resource observations, original location claims,
actual compressed ranges used, network counters, declared and actual decoder
identity, collector/primitives code hashes and unresolved validation. Written
resources, selection input/copy, decoder lock/identity and tool code are rechecked
before completion. Existing or failed outputs are preserved and cannot be
replaced. Without the receipt the output is incomplete.

The command never emits `client-sources.json` or a reviewed official-source label.
Cache/ETag/fingerprint integrity does not authenticate source content; installed
decoder RECORD checks do not prove upstream wheel authenticity. DAT/schema
compatibility, semantic decoding, foreign keys, gameplay and domain readiness
remain unchecked. Separately review location evidence, source authenticity and
producer/schema results before proposing a source-lock entry; use the unchanged
strict replay command against reviewed pins in another new root. Production
adoption remains a separate decision.

`mise run data:client:draft:test` runs synthetic HTTP failure and cache-only replay
contracts without native decoder dependencies. It also runs in `check:workflows`.

For an already acquired facts package with separately accepted replacement
evidence, [draft rebinding and portable ordinary input](source-rebinding.md)
produces a fresh scope/evidence proposal without collecting again or admitting
the source. Historical manifests and receipts remain immutable.

The coordinator records bounded decisions separately in
[`sources.lock.json`](sources.lock.json)'s `acquisition_acceptances`; these are
exact evidence decisions, not a runtime release gate. The six-supplement DAT
entry covers five lookup tables and MonsterCategories (20,878 raw bytes / 321
physical rows), bound to manifests `b3927aab` and `c0f2a23f`. Its Git archive pin,
individual resource locations, complete-table checks and independent review are
retained in the entry. Four other lookup tables exactly reuse accepted F77.
The tree-and-inheritance entry separately accepts exactly Ascendancy and
AscendancyPassiveSkillOverrides (15,891 raw bytes / 50 physical rows), plus 89
item `.it` resources (38,664 raw bytes / 82 roots), bound to manifests
`54e72ff9` and `f8642243`. Six other tree tables reuse complete F77 objects.
The completed independent review covers 135 frozen closure artifacts; root
reused its recorded strict offline reconstruction and physical decode evidence.
The `.it` path proof establishes fixed-index hash/location resolution, not a
complete directory-name inventory or independent authoritative spelling.
Literal fields and finite inheritance chains do not certify official merge law;
`Base.tag` remains the named PyPoE66d1 community interpretation. Both initial
audit-driver failures and the unused Windows publisher difference remain frozen.

The ten-CSD entry separately accepts gem, passive, skill, global, active-gem,
four snipe statsets and aura (42,841,910 raw bytes), bound to manifests
`c8a475ba` and `922aece0`. Four OS-denied-network strict replays match all ten
raw pins; complete tree9/aura physical payloads match, preserving their six/two
syntax diagnostics. The initial failed gem download remains a failure; retained
HTTP bodies, later cached receipt and exact import ledgers close its byte chain.
The four snipe CSDs have their own evidence, independent of F77. Original receipt
modes, whole-batch network counts, historical and audit-driver failures remain
distinct. Full index reconstruction reuses fixed committed discovery evidence;
the new audit checks ten physical records rather than decoding the index again.

The locale-native-tables entry separately accepts nine explicitly named
Traditional Chinese DATs, default Words, default AlternateQualityTypes and
ModEffectStats (12 resources / 24,088,336 raw bytes / 39,907 physical rows).
It binds manifests `94dcfdc5`, `a9597d71` and `edad16f5`, with three new strict
offline replays and reused complete physical-table evidence. The selected-table
reader requires exact localized resource paths and hashes before decoding; the
generic reader's locale fallback is not used to fill absent tables. This does
not approve all nine tables as UI inputs, SC client identity, language conversion
or gameplay. The ancestor's original traffic still measures four resources;
its two Ascendancy resources retain their earlier separate decision.

The original draft envelopes remain unchanged. Handler/category or tree gameplay semantics,
complete unique ownership/roll/variants, locale, compatibility, freshness and
CURRENT remain outside these decisions. New bytes do not inherit acceptance.

The sibling `retained_input_acceptances` records three separate bounded decisions
for the 0.5.5.5 international snapshot: locale text, shared PoBR rules/configuration,
and retained compatibility inputs. Each decision pins its files and source basis;
only explicitly accepted item statuses apply. These decisions preserve community
interpretations and compatibility policies rather than relabeling them as official
client facts. Blocked SC identity, deferred distribution rights and excluded inputs
keep their item-level status. Existing acquisition decisions are unchanged.

The locale decision retains the old dictionary pairs and matching order, adds the
enumerated 140 native-selected pairs plus two selected literal pairs, and pins the
v3 display projection and empty reviewed-supplement package. The shared decision
covers all 18 files, distinguishing embedded configuration, version-overlay
precedence, merged rules and the unread alias file. The compatibility decision
distinguishes vendor-derived inputs, three byte-identical extractor reproductions,
an identity-only crafting table whose original physical input is absent, and
target-produced outputs outside the six-domain release scope. A buff reference
check still exits 1 for 15 relocated windows; the parser drift check exits 0 with
closure/count warnings. Neither result certifies semantics. Local activation is a
separate user-authorized operation, not an effect of these registry entries.

The historical crafting-table decision keeps its original execution identity
unknown. The current [required crafting projection](crafting-inputs.md) instead
uses seven already pinned F77 tables and a separate neutral RePoE package; it
preserves unknown language, raw intervals and scope diagnostics. This new
consumer/source publication does not rewrite the old decision or expand raw
acquisition acceptance, complete-effect semantics or client-identity claims.
