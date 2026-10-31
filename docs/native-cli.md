# Native CLI

`pobr calculate-native` evaluates a strict `pobr-native-build/v1` request against
one explicitly supplied native version root. Both `--data-dir` and `--file` are
required. The command always prints the complete native result as JSON.

Replace both placeholder paths with an exact native dataset and a saved request:

```sh
mise exec -- cargo run -p pobr-cli -- calculate-native \
  --data-dir /absolute/path/to/exact-native-version-root \
  --file /absolute/path/to/saved-native-build.json
```

This is an invocation example, not evidence of an executed real calculation.
The [native document codec](native-build-document.md) saves the complete existing
Build with its required dataset, actor, skill, evaluation and policy envelope.
Every invocation strictly decodes the saved request and admits it again against
the supplied immutable native catalogs before calling the shared calculation.

The dataset must match the exact saved binding, including game, numerical realm,
patch, tree-resource hash, assembly key and final runtime identity. The selected
actor, gem, effect, stat-set branch, level, quality and actor-policy pin must pass
native admission. Missing or different snapshots fail, including same-patch
replacements; no fallback dataset or branch is selected.

The opt-in `sorceress-spark-level1-q0-ordinary-damage-passives-v1` candidate uses
`selected-player-generic-ot-ordinary-damage-passives-v1`/version 1 and its exact
artifact hash. Edit only the saved `build.tree.allocated_nodes` graph-ID array
within the admitted scope, then run the same command to reopen/re-admit it.
Eligible native ordinary damage INC packets compose through the shared Rust hit
reader. Duplicate IDs, unknown or unsupported packets, and mixed supported/unsupported
selections fail whole; no text fallback or implicit starting grants occur. The
old profile still rejects allocations. Result evidence distinguishes equal stats
from different nodes, and retains the original request order/hash while sorting
numerical addition/evidence. Selected nodes are assumed active for comparison;
`allocation-legality` remains unavailable for points, reachability and progression.

The request file is limited to 16 MiB. File reads stop after the limit plus one
byte so oversized input can be rejected before decoding. Duplicate, unknown,
missing or conflicting JSON fields fail through the strict document codec.
There is no stdin, environment/default data selection, CURRENT lookup, source
discovery, XML/build-code autodetection, compatibility option or extra modifier
input. `--reference-basis`, enemy options, `--full-dps` and `--mods` are rejected.
Ordinary `calculate` and `calculate-build` keep their existing interfaces.

The conditional-skill-hit-v1 result reports only the admitted conditional skill
evaluation: limited hit ranges and means, supported intrinsic use-time/cost and
projectile facts, with exact dataset/request/selection/source/policy/acquisition
identity, assumptions, diagnostics and unavailable-output reasons retained.
It conditions on a successful damaging hit under the admitted profile. It does
not establish hit probability, repeated-hit count, effective cadence or that
the actor can equip, pay for or sustain the skill.

Full DPS, total pools, total accuracy, requirements, sustain, enemy mitigation,
defences/recovery and CN numerical support are outside this output scope; absent
outputs are not zero-valued legacy stats. Source acquisition status and model
assumptions remain visible in the result. This command does not certify source
review or production readiness, and ordinary native full-calculation guards
remain in force.

An explicit actual-profile regression exercises two fresh CLI processes per edit
and compares each complete parsed result with the shared API, then removes and
reopens the selection. It requires actual generated pins in the supplied baseline
document and never guesses them:

```sh
POBR_NATIVE_PASSIVE_PACK=<new-immutable-pack> \
POBR_NATIVE_PASSIVE_REQUEST=<new-bound-baseline-document> \
  mise exec -- cargo test --locked -p pobr-cli --test native_cmd \
  supplied_actual_passive_saved_edits_run_in_fresh_cli_processes -- --ignored
```

This command is a coordinator check, not evidence of an executed pass.
