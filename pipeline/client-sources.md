# Fixed official-client resource acquisition

`client_sources.py` acquires **every resource in one explicitly selected source**
from `sources.lock.json`. It produces a new raw root with direct game-relative
paths (`Data/Balance/*.datc64`, `Metadata/PassiveSkillGraph.psg`) suitable for
`RePoE.native_facts --raw`. It does not run the fact exporter or Rust adapter.
The current nine-resource source is a bounded candidate, not complete domain or
foreign-key coverage. Production `data/`, `CURRENT`, schema and defaults are untouched.

All paths below are explicit examples; there is no current/latest lookup or fallback.

```sh
# Caller selects an existing external RePoE environment. No install/sync occurs.
mise run data:client -- acquire \
  --source-lock pipeline/sources.lock.json --source official-client-0.5.5.5 \
  --cache .cache/client-sources/cache --out .cache/client-sources/raw \
  --decoder-python /Users/claw/codes/repoe/.venv/bin/python \
  --decoder-lock /Users/claw/codes/repoe/uv.lock --mode offline
```

Offline mode never constructs an HTTP opener. Missing or corrupt cached inputs
fail; they do not trigger downloads. For online acquisition, explicitly select
`--mode online --network-budget-bytes 67108864` (or a smaller budget). Only exact
locked bundle URLs on `https://patch-poe2.poecdn.com/<locked-patch>/Bundles2/` are
requested. All redirects are rejected, including redirects within that host.
There is no index, CDN enumeration, client download or version discovery.

The online budget is cumulative **per invocation**, including reservations for
failed requests, with a hard ceiling of 64 MiB. Each request reserves its requested
length plus one overflow-detection byte before network access. The default body
cap is 4 MiB; `--max-request-bytes` can lower it. Each response must be 206 with
exact Content-Range, Content-Length, identity encoding and a strong ETag. Total
size and ETag must remain identical across all cached and new ranges for that URL;
existing validators are sent using If-Match. Corrupt cache entries fail closed.
HTTP timeouts are 25 seconds; failed requests are not retried automatically.

The reader independently interprets only the 60-byte bundle header, size table and
selected chunks. Before table allocation or decompression it bounds counts, sizes,
offsets and total length, checks redundant sizes, supported encoders (9/12), flags,
reserved fields, chunk count and the compressed size sum. Limits are 4,096 chunks,
512 MiB declared bundle raw size, 256 MiB compressed total, 1 MiB decompressed and
2 MiB compressed per chunk, 16 MiB per resource and 64 MiB selected raw bytes.
Those declared bundle totals do not authorize downloading the full bundle.

## Compressed cache and archive import

The cache contains `blobs/<body-sha256>.bin` and
`ranges/<url-sha256>/<start>-<end>.json`. Immutable range records retain the exact
source URL, range, total, ETag, body hash and acquisition provenance. Reads verify
body sizes/hashes and reconstruct from covering ranges; overlapping ranges must
agree. A replay receipt lists the actual compressed ranges used. Cache inputs,
lock inputs, archive paths and output paths reject symlinks and path traversal.
Use local directories without concurrent writers; this is not an adversarial
filesystem sandbox. Decoder interpreter symlinks (normal for a venv) are allowed.

Previously archived HTTP bodies can be imported without executing archive code:

```sh
mise run data:client -- import-archive \
  --source-lock pipeline/sources.lock.json --source official-client-0.5.5.5 \
  --cache .cache/client-sources/cache \
  --archive-root /absolute/path/to/client-source-proof \
  --archive-ledger /absolute/path/to/client-source-proof/downloads.jsonl
```

The supported JSONL ledger has one object per response: `url`, `method: "GET"`,
`status: 206`, `final_url`, `headers` (Content-Range, Content-Length, ETag),
`path` relative to archive root, `downloaded_bytes`, and `sha256`.
Only exact selected URLs are imported; unrelated entries are ignored. Selected
failures, unsafe paths, bad hashes, overlapping conflicts or validator drift fail.
The importer preserves original evidence and writes a separate cache. It neither
imports raw outputs nor trusts archive scripts. `imports/*.json` is written last.

## External decoder and completion evidence

The caller must provide both an absolute decoder Python executable and its
`uv.lock`. The fixed subprocess protocol uses `-I -S -B`, excludes CWD/PYTHONPATH
and site startup hooks, and adds only that environment's installed library path.
It imports the native `ooz` extension after checking its distribution RECORD;
it never imports RePoE/PyPoE or candidate code. The installed pyooz version must
match uv.lock, whose PyPoE repository/revision must match the source declaration.
No GPL PyPoE or ooz implementation is copied into this script or Rust core.
External dependencies retain their own licenses and distribution requirements.

The receipt records the actual module SHA-256, distribution METADATA/WHEEL/RECORD
hashes, Python identity, uv.lock hash and locked package fingerprint. Module identity
is checked again before every decode. The native call receives a bounded expected
output size and its result length must match exactly. Each child has CPU (10 s),
wall time (25 s) and output file (2 MiB) limits. Linux also enforces a 256 MiB
address-space limit. Darwin rejects address-space/data/RSS limits; the receipt
explicitly reports `address_space_bytes: null` there. The output-size and other
limits still apply; no process-memory sandbox is claimed on Darwin.

Every raw file must match the selected lock's bytes/SHA-256 before it is written.
The output root must be absent, separate from cache, lock inputs and production
`data/`. Existing outputs are never replaced, including partial failed runs.
`client-sources.json` is written last, after rechecking written resources and lock
inputs. An output without this receipt is incomplete; retain it for diagnosis or
choose another new output root. There is no extra raw staging/copy directory.

The caller trusts the reviewed source/schema lock and explicitly supplied decoder
environment. Cache hashes, distribution RECORD and local fingerprints establish
integrity/identity, **not independent authenticity**. The source's schema declaration
is recorded, but acquisition does not parse tables or certify schema semantics,
game facts, geometry or gameplay. An installed wheel's upstream artifact hash is
not independently verified by comparing its installed RECORD.

For a future fixed source, separately discover/review its exact resource paths,
raw bytes/hashes, bundle URLs and uncompressed offsets/lengths, and pin its decoder
and schema. Add that reviewed source through the source-lock maintenance process,
then run this same command with its key and a new raw root. New index discovery,
scheduling, full-domain export and production adoption remain separate work.

```sh
mise run data:client:test
```

Tests use synthetic framing and fake HTTP, including offline replay, 200 responses,
redirects, validator drift, malformed frames, budgets, tampering and failed outputs.
Actual nine-resource ooz reconstruction is a separate offline acceptance check with
the external environment and archived ranges; unit tests need neither dependency.
