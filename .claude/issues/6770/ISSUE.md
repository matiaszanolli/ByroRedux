# CHAR-2026-11-07-L-11: Two profile rows have no capture line: add_perk_raises_rank and the perk_entry_points dialect

**Issue**: #6770
**Labels**: documentation, low, game:fnv, game:fo3, game:fo4, game:skyrim, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-11, Dim 5). Game: FO3, FNV, FO4, Skyrim.

## Description
Two profile rows have no capture line: `add_perk_raises_rank` (true on FO3/FNV/FO4, false on Skyrim as an open question) and the `perk_entry_points` dialect. They state per-game behaviour recorded only in `profile.rs` rustdoc and the PRD. The convention since #6284 is that each profile row is documented in the game's capture so it can be checked against a document, as `charal-fo4-ruleset.md:527-545` does for bleedout, encumbrance, kill XP and barter. The Skyrim open question is not in the Skyrim capture.

## Location
`crates/core/src/character/profile.rs` (`add_perk_raises_rank`, `PerkEntryPointDialect`) against `docs/engine/charal-{fnv-fo3,fo4,skyrim}-ruleset.md` (`grep -n 'AddPerk\|raises' docs/engine/charal*.md` is empty).

## Evidence
Sources exist but are cited only in rustdoc: `geck-uesp-wiki/(main)/AddPerk.wiki:3` ("Increments the rank of the specified perk"), `falloutck-uesp-wiki/(main)/Perk.wiki:34` ("Each time a perk is added to an Actor the perk's rank will increase"), both under `/mnt/data/src/reference`.

## Impact
A later source finding on Skyrim `AddPerk` has no capture to correct.

## Related
#6619, `docs/prd/perk-entry-points.md` (PERK-BODY-001, PERK-GATE-004).

## Suggested Fix
One short "Perk rows" section per capture citing the two wiki lines and marking Skyrim unverified.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
