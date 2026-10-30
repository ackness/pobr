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
