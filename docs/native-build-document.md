# Native build document

`pobr_build::native_document::NativeBuildRequest` stores the existing `Build`
state with an explicit `pobr-native-build/v1` JSON discriminator. Strict encoding,
decoding and dataset matching are separate from the bounded
[`native_skill`](../crates/pobr-build/src/native_skill.rs) admission/calculation API.
Saving a document does not make a native catalog calculable. Ordinary schema-4
BuildData/ruleset calculation remains guarded; application dispatch is separate.

The required envelope records:

- Game, numerical realm, exact patch, tree-resource hash, native assembly key and
  final `GameData::identity().sha256`.
- Actor stable ID/level, selected zero-based group/gem locations, gem base-item ID,
  granted-effect ID, stat-set stable ID, exact gem level and quality.
- Actor-policy name/version and exact policy artifact hash, plus the requested
  evaluation kind.
- The complete existing `Build`: equipment, raw modifier text, tree choices,
  skill groups, active/inactive weapon and utility state, config and party state.

The dataset binding takes an explicitly supplied `GameData`. Missing, legacy or
different data fails; no CURRENT lookup, same-patch substitution or branch ordinal
fallback occurs. The selected actor/gem levels and identity mirrors must agree.
`admit_native_skill` checks catalog ownership, model policy, profile restrictions
and calculation dependencies after every edit and reopen. It owns the validated
request and resolved inputs behind private fields. It has no deserializer,
caller-forgeable constructor, defaults or mutable getters. Unsupported state can
be retained by the document codec without being admitted as a calculation source.
The current numerical contract is [native conditional skill hit](native-skill-hit.md).

`to_json()` canonicalizes object ordering and retains array order. Every emitted
field, including nullable fields, is required on reopen. Duplicate keys (also
escaped aliases), unknown fields and nested compatibility defaults that would
lose state are rejected. Numerical fields must remain finite; serialization
cannot turn NaN or infinity into JSON null. Float round-trip parsing preserves
the existing model's binary values. The input limit is 16 MiB.

`input_sha256()` identifies the complete native request. A result cache also
needs the executing engine/model implementation identity, or must recompute.
This request digest alone does not certify engine equivalence or source approval.

The opt-in ordinary-damage-passive profile edits the existing
`build.tree.allocated_nodes` array of graph IDs; no patch API or new document
fields are introduced. Its separately assembled pack and new actor-policy hash
must be bound explicitly. Old saved dataset/policy pins keep their old scope and
cannot silently widen or migrate. Save/reopen preserves allocation order, so a
reordered array has a distinct request hash even when sorted calculation/evidence
is equivalent. Removing all selections restores that candidate's baseline.
Every selected packet must pass admission as a whole; mixed unsupported selections
fail instead of saving a partial numerical answer. Selections declare active
nodes for a conditional comparison and do not certify point budget, paths or
progression legality. Starting relations grant nothing automatically.

Existing PoB XML/build-code imports continue through their explicit compatibility
entry points. JSON documents are rejected there before embedded XML/item markers
can be interpreted. The application WASM handshake and active data are unchanged
by this storage contract.

Focused check:

```sh
mise run verify build lib native_document::tests::
```

The codec regressions exercise complete in-memory round trips, source/selection and
policy-sensitive fingerprints, retained inactive/Chinese state, malformed and
conflicting input, duplicate/ignored fields, exact finite numbers and compatibility
route rejection. The admission suite adds portable sealed-fixture save/reopen and
fresh disk/memory instances. Its explicit supplied-path test covers actual native
dataset reopen/calculation; codec tests alone do not prove that path.
