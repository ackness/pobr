# Market copy and rune survival boundaries

## Complete-item copy contract

`web/public/userscripts/pobr-market-copy.user.js` adds one copy button per loaded
listing on the official CN and international PoE2 market search pages. A click
reads one listing from the current site's `/api/trade2/fetch` endpoint, using
its existing session. There are no background listing requests, automatic
retries, purchases or seller contacts. The copied payload contains only item
fields, never listing/account/whisper/price fields or credentials.

The serializer accepts string modifiers and localized API modifier objects.
It preserves their description, strips display-link markup, and ignores tier
labels and expanded roll alternatives. Normal/magic items have one canonical
base line; rare/unique items have a name and base. Requirements, item level,
rolled defences, implicit count, socket occupancy, rune/enchant origins,
crafted/fractured modifiers and corruption status survive the copy. Weapon
panel values and recovery/charge descriptions are reference metadata, not
extra modifiers. Utility property placeholders are interpolated. Unknown
modifier groups stay explicitly unmodeled, including conditional effects that
cannot safely be injected as ordinary permanent stats.

Search-only `pseudoMods` (including Sum and combined resistance) are excluded.
The paste normalizer converts the older userscript's exact
`Unmodeled market effect (pseudoMods):` prefix into reference metadata. It does
not remove real unknown effects or infer effects from aggregate scores.

Fetch failures never overwrite the clipboard. A failed clipboard write opens
a selected manual-copy dialog only if the listing is still current. The
script handles newly loaded rows and reused row IDs. It is served with the
web app; `/userscripts/index.html` documents installation and the Upgrade paste card
links to it. Active/lineage gems still use the skill-upgrade path.

`web/e2e/market-copy-userscript.spec.ts` executes the distributed script with
synthetic API fixtures. Coverage includes both regions, one-request scope,
metadata/origin preservation, changed listings, rate limits, clipboard denial
and copied text reaching real-WASM whole-item comparison. Manual QA also
exercised a logged-in CN listing with the browser clipboard. That manual test
does not establish compatibility with every userscript manager version.

## Utility import and editing

WeGame magic flasks and charms previously received an extra `Imported Item`
name line. Their canonical base was then misclassified as a modifier. The
importer now emits a single base and retains decorated names, recovery/charge
properties and extra requirements as metadata. Existing saved imports are
not rewritten; reimport the source to obtain corrected text.

`ItemDraft.metadata_lines` retains `Note`, attribute requirements and the
requirements marker through edit/build-raw round trips. Separators and these
metadata lines do not consume `Implicits`. Rune and enchant annotations retain
their origin even after the implicit section. Arbitrary unknown text and
actual unmodeled effects remain modifiers/diagnostics.

Empty socket placeholders previously emitted as `Rune: None` could increment
`sockets_filled`. Importers now emit rune names only for occupied sockets; the
core also ignores empty/None rune and soul-core headers in older text. This
does not remove actual `{rune}` modifiers, nor inject a second copy of a rune's
effects from its name.

Recovery descriptions are preserved for inspection. This is not a new flask
uptime/healing or charm-trigger simulation. In particular, `Also grants 102
Guard` remains unmodeled; it is not permanent life or unconditional EHP.

## Replacement augment planning

The complete-item comparison follows pinned PoB2 `ItemsTab:CopyAnointsAndAugments`:
an item with existing augments stays as listed by default; an empty candidate
inherits the current destination's identified augments, subject to its own
base, socket capacity, augment limits and character level. Each destination
gets its own prepared item text before a single batch comparison. The original
build is the baseline; nothing is applied until the player explicitly applies
the evaluated destination payload.

Players can compare the original listing or customize sockets and augments.
The UI shows assumed added sockets and omitted augments. This is a hypothetical
post-socketing comparison; market price does not include those changes. Raw
clipboard text is retained so returning to the listing does not stack effects.
Pasting again resets the setup, including when the text is identical.

`itemAugmentInfo` and `reforgeRunes` share item eligibility and restrictions.
`base_item_overrides.socket_limit` and rune level/limit/type restrictions are
extracted from pinned PoB2 instead of a universal frontend socket constant.
Existing extra sockets are preserved. Corrupted/mirrored items cannot gain
sockets in this editor. Unsupported special socket mechanics and unidentified
existing augment effects remain read-only, preserving their original text.
Reforging rebuilds modifier sections through `ItemDraft`, retaining real
implicit/explicit effects and avoiding stale rolled-defence inputs.

Both the Equipment editor and replacement picker consume this contract.
PoB2 augment limits apply across equipped items, with shared groups keyed by
`limit_id` when present (otherwise by name). The editor and comparison subtract
usage on other currently active items, excluding the destination being replaced;
inactive weapon-set inventory never enters that count. Inheritance skips a
full group, while original/custom candidates that exceed it cannot be evaluated.
Unknown occupied augment identities reserve limited choices conservatively and
are explained in the UI. The backend also validates shared groups within each
reforged item; its item-only endpoint does not claim to validate an entire build.
Changing the build, weapon set or clipboard invalidates replacement results;
augment edits disable application until the new calculation completes.

## Rune survival audit

The reference is the repository's pinned PoB2 commit `ce566eac`, especially
`src/Classes/Item.lua` (Bonded activation before local-property calculation,
augment scaling around lines 2818 onward) and `src/Data/ModCache.lua`
(Guard wording has a nil parse result). Do not infer that a named parser
stat has a downstream consumer.

An actual imported build was recalculated with one source removed at a time.
Ordinary chest rune local evasion increases and a helmet rune's lightning
resistance changed evasion, energy shield, resistance and EHP as expected.
The energy-shield change followed Spectral Ward's chest-evasion contribution.
The tested build had neither the relevant augment-effect multipliers nor a
Bonded unlock, so this observation does not validate those separate paths.
Private builds and numeric reports remain outside committed fixtures.

Current gaps to address independently:

- Rune/augment effect stats can parse without a scaling consumer. Item data
  currently loses enough origin information that applying a generic effect
  multiplier risks scaling the wrong source or applying an API multiplier
  twice.
- Bonded local defence must activate before local armour/evasion/ES rolls.
  Treating it as a global modifier changes other equipment and interacts
  incorrectly with quality. Synthetic legacy-item checks reproduced this
  ordering gap; the audited character does not activate it.
- Guard activation, duration and absorption need an actual event/state model.
  The pinned PoB2 parser also leaves the charm-granted Guard wording
  unsupported. Existing generic Guard pool fields are not an implementation
  of that charm's behavior.
- Projectile speed, random Crescendo arrow effects and Ward behavior require
  distinct consumers or scenario metrics; they are not interchangeable with
  main-skill DPS or permanent survival.
- The existing weapon-base contribution models physical base damage, while
  some PoB2 bases also contain innate elemental/chaos damage. For example,
  Adherent Bow has `ChaosMin = 14, ChaosMax = 32` in PoB2's `Data/Bases/bow.lua`;
  these fields are absent from PoBR's current base schema. Copied weapon
  panel values remain reference metadata and do not close this calculation
  gap. Carry typed base damage through extraction and per-hand calculation
  before treating such weapon comparisons as complete; do not inject the
  rolled panel as extra flat damage on top of existing affixes.

The next implementation should first establish whether real market/WeGame
`runeMods` and rolled defences are already scaled, using an item that actually
has an augment-effect multiplier. Preserve rune/soul-core/idol origin, normal
versus Bonded state and raw versus already-scaled state across import, editing,
XML and clipboard round trips. Then activate eligible Bonded bonuses, scale
the correct item's augment modifiers and evaluate local defence.

PoB2 adds the original modifier and a separately scaled increment. Reuse the
equivalent `ModDb::scale_add_mod` semantics, including fractional values,
PercentStat, flags and lists; multiplying and truncating the final total is
not equivalent. Required regressions include mixed augment types, no
cross-item scaling, no double application of rolled defences, local quality,
Bonded gating, idol restrictions and fractional increments. Keep these gaps
visible until their source-to-output behavior has been verified.
