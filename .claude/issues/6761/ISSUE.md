# CHAR-2026-11-07-L-02: ArmorModel documents fArmorRatingPCBase as a Skyrim.esm / executable value, but no Skyrim master authors it

**Issue**: #6761
**Labels**: documentation, low, game:skyrim, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-02, Dim 1). Game: Skyrim.

## Description
`ArmorModel`'s value for `fArmorRatingPCBase` (1.0) is right, but the stated provenance is not: the module doc puts it under a `Skyrim.esm` column, `SKYRIM`'s doc says its defaults are `Skyrim.esm` byte-scanned, and the header calls them "executable defaults of six game settings" while seven are read. Same class as #6442 (a compiled-in fallback and an authored value are different things).

## Location
`crates/core/src/character/armor.rs:3-19`, `:48`, `:70-77`; `docs/engine/charal-skyrim-ruleset.md:297-302`.

## Evidence
Byte scan of Skyrim.esm (SE and LE), Update, Dawnguard, Dragonborn, HearthFires and the cc masters: six of seven settings are authored (0.12, 0.03, 80, 1.4, 1.0, 2.5); `fArmorRatingPCBase` has no `GMST` record. Its 1.0 comes only from the CK *Settings* page (`/mnt/data/src/reference/ck-uesp-wiki/Category/Settings.wiki:323`, "1.000000").

## Impact
None on gameplay; a future audit looking for the master value finds none, or re-litigates the row.

## Related
#6442, #6567.

## Suggested Fix
State in `armor.rs` and the Skyrim capture that `fArmorRatingPCBase` is a CK-page default no master authors (overlay kept for mods), and say "master-authored, byte-scanned" for the other six.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
