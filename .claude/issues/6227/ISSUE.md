# #6227: CHAR-2026-10-21-M-01: New Vegas never lifts its level cap past 30, because each add-on's +5 raise is applied by the executable and appears in no master

**Labels**: medium,character,bug,game:fnv

**Source**: docs/audits/AUDIT_CHARACTER_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Progression & Pools · **Location**: `crates/core/src/character/leveling.rs` (`LevelingModel::FNV`, `level_cap: 30`; `with_gmst` `XpCurve` arm; the `XpCurve` variant doc, which says "a DLC master that raises the cap (`BrokenSteel.esm`) is honoured through the merged load order")

**Audit status**: NEW

- **Game**: fnv
- **Source**: `charal-fnv-fo3-ruleset.md:459`: "FNV **30** (50 with the four add-ons, +5 each)". DeadMoney.esm `MESG` `NVDLC01LoadedMsg` (0x0100F41E) and HonestHearts.esm `MESG` `NVDLC02LoadMessage02` (0x01008A34) both say "…has been loaded. Your level cap has been raised by 5."
- **Description**: #5915 made the cap an overlay of the authored `iMaxCharacterLevel`. That covers FO3: BrokenSteel.esm authors 30. It does not cover New Vegas. A GMST scan of all ten FNV masters finds `iMaxCharacterLevel` only in FalloutNV.esm (30). The add-on masters author no such setting. The add-on quest script (`NVDLC01MQ00Script` source text) only shows the message, so the raise happens inside the executable. With Dead Money, Honest Hearts, Old World Blues and Lonesome Road loaded, the engine still stops the roll at 30, where the game allows 50. The `FNV` constant's doc ("cap 30 (50 with all add-ons)") and the `XpCurve` doc both read as if the add-on case is handled.
- **Evidence**: `/tmp/audit/character/scripts/gmst.py` over `CaravanPack`, `ClassicPack`, `DeadMoney`, `FalloutNV`, `GunRunnersArsenal`, `HonestHearts`, `LonesomeRoad`, `MercenaryPack`, `OldWorldBlues`, `TribalPack`. Only `FalloutNV.esm iMaxCharacterLevel 30` prints. `roll_levels` stops at `level >= level_cap()`, pinned at 30 by `experience_roll_stops_at_the_fallout_new_vegas_level_cap`.
- **Impact**: An FNV player with the add-ons loaded stops levelling at 30: no XP-driven levels, skill points or perks for 20 levels the game grants. This is silent; no log line names the cause.
- **Related**: #5915 (the GMST overlay fixed FO3 + Broken Steel only). Also closed #2943 (its fix `26c9a477c` documented that DLC cap raises were not modelled; #5915's docs now imply they are).
- **Suggested Fix**: First source how the executable triggers the raise, without guessing; the candidate is a loaded-master check for the four story add-ons. Then express it as data on the FNV profile, for example a `(master name, cap delta)` table applied after the GMST overlay. Until then, correct the two `leveling.rs` docs to say the add-on raise is not modelled.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (FO3 add-on caps and any other executable-applied rule not authored in a master)
- [ ] **TESTS**: A regression test pins this specific fix
