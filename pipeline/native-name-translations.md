# Native display-name sidecars

`native-name-translations.mjs` consumes a pinned `poe2-native-names/v1` candidate
from the independent translation repository. RePoE owns the physical localized
tables; the translation repository owns pairing and ambiguity; PoBR only adapts
the result to its existing ordinary name DTOs.

The current output is the three existing **zh-TW ID-keyed** files:
`base_items.json`, `skills.json`, `mods.json`. The zh-CN affix sidecar uses a
different, English-keyed contract and is deliberately not routed through this
adapter. No locale conversion, numeric field, formula, import codec or runtime
translation service is added.

```sh
mise exec -- node pipeline/native-name-translations.mjs \
  --request .cache/name-request.json --out .cache/fresh-names
mise exec -- node --test pipeline/test-native-name-translations.mjs
```

The request has schema `pobr-native-name-request/v1`, `version`, `locale: "zh-TW"`,
`native: {path, sha256}`, and `legacy: {"base_items.json": {path, sha256},
"skills.json": {path, sha256}, "mods.json": {path, sha256}}`. Record the historical
source and version of the legacy input in a `compatibility` annotation. Inputs
must be bounded regular files and match their external fingerprints. The output
must be a fresh directory in this checkout's `.cache`; its parent must exist.

For current source IDs, translated text replaces old text; missing, empty or
unchanged source text removes the stale localized value so normal English
display fallback applies. IDs absent from the entire new inventory retain the
pinned historical value, explicitly listed in the receipt. No old text becomes
target-language evidence. Source row coverage and duplicates are checked; English
homonyms are safe here because the consumer retains exact table/ID identity.

`receipt.json` records every adopted/excluded/current and historical entry, input
identities, implementation digest and output fingerprints. Candidate assembly
pins these three files into a **new** snapshot and runtime identity; it does not
change production `CURRENT`. Other names in the translation candidate remain
available to future existing-contract consumers, not claimed as active UI
coverage. Simplified Chinese missing raw sources and complete unique modifier
definitions remain independent gaps.
