# #5420: ESM-2026-10-08-D4-02: The SM decode leaves xEdit-defined fields marked "[open]/unverified": `QNAM` is Quest Count (100% of `SMQN`s), `XNAM` Max concurrent quests, `MNAM` Num quests to run, `HNAM` node Hours until reset; TES5 `FNAM` and F...

**Labels**: low,esm-plugin,scripting,quests,bug,game:skyrim,game:fo4,game:fo76,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5420

**Source**: `docs/audits/AUDIT_ESM_2026-10-08.md` — `ESM-2026-10-08-D4-02` (HEAD `00f580e09`)

- **Severity**: LOW. Nothing is misparsed. The contract and the docs say "unknown" where the reference says otherwise, and the runtime ignores decoded semantics because of it.
- **Dimension**: Record Schema Dispatch & Coverage
- **Record / Sub-record**: `SMQN`/`SMBN`/`SMEN` · `QNAM` `XNAM` `MNAM` `HNAM` `FNAM` `UNAM`, `DNAM` bits 1/18
- **Location**: `crates/plugin/src/esm/records/misc/story_manager.rs:27-35`, `:91-95`, `:152-166`; `docs/engine/story-manager.md:106-112` and §5.
- **Status**: NEW
- **Description**: The xEdit definitions (TES5.pas:7056-7111; FO4/FO76/SF1 `SMQN`) are:
  - `XNAM` 'Max concurrent quests'
  - `MNAM` 'Num quests to run'
  - `HNAM` 'Hours until reset' (FO4+)
  - `QNAM` 'Quest Count' (`SetCountPath` of the `NNAM` array)
  - `FNAM` '24 Hours Till Reset' (TES5, bool)
  - `UNAM` 'Priority' (FO76, per quest)
  - `DNAM` bit 1 'Warn if no child quest started' and bit 18 'Num quests to run'

  The module calls them "candidate repeat/priority, unverified" and "[open] … rather than being guessed at". `FNAM` falls to `extras`.
- **Evidence**: `QNAM == count(NNAM)` on 448/448 Skyrim, 36/43/8 Dawnguard/Dragonborn/HearthFires, 219/219 FO4 and 354/354 Starfield `SMQN`s. `FNAM` is present on 5 Skyrim `SMQN`s and `MNAM` on 17.
- **Impact**: The runtime starts one quest per node. It never honours Num-quests-to-run or Max-concurrent, which the CK wiki documents.
- **Related**: D4-01, D2-01.
- **Suggested Fix**: Decode the fields under their xEdit names with citations, keep raw values where semantics are unconsumed, and correct §5 of the design doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all SMBN/SMEN/SMQN arms + story-manager.md §5)
- [ ] **TESTS**: A regression test pins this specific fix
