# Independent language generation

PoBR acquires immutable language inputs and invokes the producer in
`ackness/poe2-zh-cn-stats`. The translation owner performs dictionary projection,
word/template merging and explicit selected-pair composition. PoBR no longer
contains the legacy `gen-zh-cn.mjs`, dictionary downloader or translation merge
implementation. Complete inputs and outputs stay in disposable directories.

`locale-generation.lock.json` pins the target, full producer commit, the three
Node module byte/SHA-256 pins and the input manifest. Each manifest input has its
own byte/SHA-256 pin and immutable source. Logical input paths are independent
of their repository paths: raw GitHub URLs name a full commit; `producer-commit`
evidence resolves at the locked translation commit. No HEAD lookup or implicit
refresh occurs. The adapter verifies every generated file and receipt binding,
then rechecks all materialized inputs and the lock before exposing output.

```sh
mise exec -- python3 pipeline/locale_inputs.py --out .cache/locale-generated
# Reuse the authenticated download cache, refusing any miss or corruption:
mise exec -- python3 pipeline/locale_inputs.py --offline --out .cache/locale-replay
# Run inside the existing snapshot generation/validation transaction:
mise exec -- python3 pipeline/data_snapshot.py dictionary -- --offline
# Another target needs a reviewed, matching independent generation lock:
mise run data:legacy:regen --locale-lock /path/to/locale-generation.lock.json
```

The current `retained-selected` profile generates nine `i18n/zh-CN` files and
`generated/translation-audit.json` from fixed historical inputs. Its 27,896 old
pairs plus 140 explicitly selected native pairs and two selected literal pairs
reproduce the accepted 28,038-pair baseline in the original order. The old
metadata remains a separate pinned input; its historical regeneration command
is retained provenance. This is historical reproduction, not new source or
client-semantic acceptance. A new source bundle can use the independent
producer's `legacy` profile, but PoBR still requires exact expected output pins.

The three Traditional Chinese members and compact Simplified Chinese
`display_texts.json` have separate independent replay/projection contracts.
This generator does not produce them. Installation preserves those files and
writes only the ten declared outputs plus a maintenance receipt at
`generated/locale-generation-receipt.json`. Audit and receipt are excluded from
the runtime manifest. The final snapshot identity is still checked separately.

The explicit historical `data:legacy:regen` and base-adaptation transactions
can invoke this consumer with a matching `--locale-lock`. Current complete build
candidates instead use `data:update --lock <build-data.lock> --out <fresh>`: that
lock pins the independent owner's published language files. It does not run a
PoBR downloader or silently regenerate language from HEAD. A new language output
must be published by its owner and bound into the complete snapshot lock.
`--candidate` is restricted to the snapshot transaction and refuses the source
`data/` tree. Failure leaves the published snapshot and `CURRENT` unchanged;
the transaction seals and audits the candidate before promotion. Complete
outputs must be published by their independent owners, never committed to PoBR.

The adapter's synthetic checks cover acquisition, corrupt caches, immutable
addresses, receipt/output tampering and candidate boundaries. The independent
repository owns projection/merge/selection tests. Real fixed-input byte
reproduction and source semantic acceptance remain distinct evidence.
