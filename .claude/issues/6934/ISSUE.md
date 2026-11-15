# #6934 — INC-2026-11-14-22: Doc rot from the BO64, XCT and music commits: stale claims and misattached doc comments

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-22). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: see list
- **Changed in**: commits `7af55a0df`, `9405138e9`, `45b3b0a64`

## Description
- `crates/plugin/src/equip.rs` (`resolve_armor_meshes` comment): "Starfield.esm 0 / 1106 author bits → rule is a total no-op" and "no per-addon biped-slot field" — now false; all 1 106 Starfield ARMAs carry BO64, so the #3411 gate is live there (near no-op: 2 of 1 017 ARMOs have a race-blind redundant ARMA).
- `crates/plugin/src/esm/cell/index.rs`: `interior_cell_key_of_refr` was inserted between `locate_refr` and its doc comment, so `locate_refr` is undocumented and the helper carries its spawn-ladder paragraph.
- `crates/plugin/src/esm/records/misc/equipment.rs`: `parse_arma_reads_starfields_bo64_mask` sits under the #5358 BODT doc comment, so `parse_arma_reads_bodt_biped_flags` lost its doc.
- `byroredux/src/inventory/mod.rs` (`EquipTarget` doc): "`MAX_BIPED_SLOTS = 32`" / "all 32 of them".
- `crates/audio/src/lib.rs`, `crates/plugin/src/esm/records/misc/world.rs`, `byroredux/src/components/environment.rs`: point at the deleted `dispatch_region_ambient_music`; repoint to `crate::music`.

## Suggested Fix
Move each doc comment back to its item; restate the Starfield BO64 measurement; repoint the REGN music references.

## Completeness Checks
- [ ] **VALIDATE**: no comment names a deleted symbol (`git grep dispatch_region_ambient_music` is empty)
