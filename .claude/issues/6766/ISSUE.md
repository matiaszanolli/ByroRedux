# CHAR-2026-11-07-L-07: Capture documents lag the code: ten fVATS* defaults, Skyrim's jail divisor and SendPlayerToJail status, and the crime row's scope

**Issue**: #6766
**Labels**: documentation, low, game:fnv, game:fo3, game:skyrim, game:oblivion, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-07, Dim 3 + Dim 5). Game: FNV, FO3, Skyrim, Oblivion.

## Description
Capture documents lag the code. (a) The FNV/FO3 VATS section's two lists (the six FNV re-tunes, the wiki-only set) cover 11 settings; ten more that both masters author appear in neither, and the second list says "neither master authors them", true only for the 11 it names. (b) The Skyrim crime section states fines but no sentence rule, and still lists jail and the `SendPlayerToJail` native's engine side as unbuilt although `36d7bfbd6`, #6569 and #6573 built them (`PayCrimeGold` and guard AI remain unbuilt). (c) The Oblivion capture says `crime()` is "Oblivion only" though `CrimeModel::SKYRIM` landed in `4cfcee4b9`.

## Location
`crates/core/src/character/vats.rs:166-216` (`FALLOUT3`) against `docs/engine/charal-fnv-fo3-ruleset.md:513-551`; `crates/core/src/character/crime.rs` `SKYRIM` against `docs/engine/charal-skyrim-ruleset.md:819-852` (heading "guards and jail (not built)", `:848-851`); `docs/engine/charal-oblivion-ruleset.md:838`.

## Evidence
`crates/plugin/examples/probe_gmst_vats` on both masters: `fVATSDistanceFactor` 2, `fVATSScreenPercentFactor` 1, `fVATSShotLongBurstTime` 1.75, `fVATSMeleeWarpDistanceMult` 0.32, `fVATSH2HWarpDistanceMult` 0.27, `fVATSParalyzePalmChance` 0.3, `fVATSPlaybackDelay` 0.17, `fVATSStrangerOdds` 0.1, `iVATSStrangerMaxHP` 150, `fVATSStrangerDistance` 164, identical in both; the GECK wiki *VATS Settings* lists the same. UESP *Skyrim:Crime* `:103` for the Skyrim divisor (100 gold/day, 7-day cap).

## Impact
An auditor cannot verify those rows from the capture (UNSOURCED-row rule), and a reader scoping Skyrim crime work would rebuild jail. Code values match the masters and the wiki, so nothing is wrong at runtime.

## Related
The Skyrim jail cap finding filed from the same audit; #6425 (added the VATS section).

## Suggested Fix
Add a third "authored identically by both masters" list; add the divisor and 7-day cap to the Skyrim crime section and mark `SendPlayerToJail` built; say "Oblivion and Skyrim" at the Oblivion capture line.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
