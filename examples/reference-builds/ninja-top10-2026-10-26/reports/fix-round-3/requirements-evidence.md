# Round 3 requirement evidence

Source: `/root/codes/pobr/.cache/accuracy-round2-2026-10-26/final-classified.json`; all23 engine/both requirement rows have one matching exported amulet source and one raw zero-quality, uncorrupted ordinary listing.

| Build | Stat | PoBR | PoB2 | Granted skill | Item / groups (1-based) | Nominal requirement |
|---|---|---:|---:|---|---|---:|
| abyssal-lich-06 | ReqStr | 68 | 137 | Eternal Rage 18 | item13; listed2, source19 | 137 |
| acolyte-of-chayula-09 | ReqStr | 65 | 137 | Eternal Rage 18 | item12; listed5, source18 | 137 |
| amazon-08 | ReqStr | 20 | 137 | Eternal Rage 18 | item8; listed2, source11 | 137 |
| blood-mage-05 | ReqDex | 72 | 86 | Trinity 20 | item10; listed4, source17 | 86 |
| deadeye-07 | ReqStr | 60 | 126 | Eternal Rage 17 | item21; listed2, source15 | 126 |
| deadeye-08 | ReqInt | 116 | 126 | Cast on Elemental Ailment 17 | item20; listed2, source15 | 126 |
| disciple-of-varashta-03 | ReqStr | 30 | 75 | Wolf Pack 18 | item17; listed4, source17 | 75 |
| disciple-of-varashta-10 | ReqStr | 20 | 86 | Wolf Pack 20 | item22; listed5, source19 | 86 |
| gemling-legionnaire-06 | ReqStr | 73 | 144 | Eternal Rage 20 | item12; listed5, source17 | 157 × 0.92 → 144 |
| invoker-04 | ReqStr | 70 | 137 | Eternal Rage 18 | item7; listed2, source12 | 137 |
| invoker-05 | ReqInt | 126 | 137 | Cast on Critical 18 | item17; listed1, source12 | 137 |
| invoker-07 | ReqInt | 137 | 157 | Cast on Critical 20 | item11; listed2, source13 | 157 |
| invoker-08 | ReqDex | 57 | 86 | Trinity 20 | item9; listed3, source18 | 86 |
| martial-artist-04 | ReqStr | 137 | 147 | Eternal Rage 19 | item7; listed5, source16 | 147 |
| martial-artist-08 | ReqStr | 103 | 137 | Eternal Rage 18 | item11; listed4, source16 | 137 |
| martial-artist-09 | ReqStr | 103 | 157 | Eternal Rage 20 | item14; listed4, source14 | 157 |
| martial-artist-10 | ReqStr | 63 | 137 | Eternal Rage 18 | item15; listed5, source14 | 137 |
| pathfinder-07 | ReqStr | 86 | 147 | Eternal Rage 19 | item29; listed3, source17 | 147 |
| pathfinder-10 | ReqStr | 86 | 157 | Eternal Rage 20 | item23; listed2, source12 | 157 |
| shaman-03 | ReqDex | 41 | 86 | Trinity 20 | item23; listed2, source16 | 86 |
| shaman-04 | ReqDex | 10 | 86 | Trinity 20 | item26; listed2, source13 | 86 |
| witchhunter-03 | ReqStr | 100 | 137 | Eternal Rage 18 | item13; listed4, source15 | 137 |
| witchhunter-09 | ReqStr | 98 | 137 | Eternal Rage 18 | item16; listed4, source16 | 137 |

Gemling06 has two allocated4% global requirement reductions (nodes18146 and3084): floor157×0.92=144. Its local boot reduction is not a global gem modifier.

Oracle04 is only partially explained: native377−93=284, oracle372−273=99. Oracle reserves180 for the Absent Amulet Archmage (100+Her Declaration30+Clarity II20+Mysticism II30); native correctly waives it. Both reserve Arctic Armour30 and Spell Totem63. The remaining5 pool difference is unresolved: imported sceptreSpirit227 versus oracle-implied222. Do not mark the whole185 unreserved row known.

No glove parser change: the four affected Martial Artists already equip zero-requirement Fists of Stone bases.

Exact raw item/source/listed-group fields, requirement shares and reductions are in `/tmp/pobr-round3-requirements-evidence.json`.
