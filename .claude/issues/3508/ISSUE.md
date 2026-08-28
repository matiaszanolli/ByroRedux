# #3508: FO3-2026-08-27-D4-02: docs/feature-matrix.md attributes _far.nif placement LOD to FO3 and calls multi-band selection deferred; both are contradicted by the code

**Labels**: low, terrain-exterior, documentation, doc-rot, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D4-02` (LOW, Dimension 4 — cell loading / doc-rot).

## Location
`docs/feature-matrix.md:54`

## Description
The row reads

```
| **Terrain LOD (M35)** | ~ Partial | `.btr` (Skyrim+/FO4) + `.bto` + `_far.nif` (Oblivion/FO3/FNV) shipped; distance-based multi-band selection + `.btr` normal maps deferred |
```

Two claims are false for FO3 at HEAD.

(a) `_far.nif` placement LOD is **Oblivion-only**: `byroredux/src/cell_loader/placement_lod.rs::placement_lod_supported` gates on `GameKind::Oblivion` alone (#2086) —

```rust
pub(crate) fn placement_lod_supported(game: GameKind) -> bool {
    game == GameKind::Oblivion
}
```

— and its own test `placement_lod_supported_is_oblivion_only` asserts `!placement_lod_supported(GameKind::Fallout3NV)`. `object_lod.rs`'s module doc records that FO3 ships 2 `_far.nif` and **zero** `distantlod\*.lod`, so the scheme is a documented no-op for FO3.

(b) "distance-based multi-band selection … deferred" was made false by #2371: FO3 runs the four-level quadtree ladder (`LodBandLadder::for_terrain_game` / `for_object_game` → `fallout_legacy()`), verified in this audit to emit levels 4 and 8.

## Impact
Doc-only, but it is the row an auditor or contributor reads to decide whether FO3 distant-object work is already done, and it points at the wrong module. It also mis-credits the `_far.nif` scheme to a title that ships none, which is exactly the premise #2086/#3321 spent two cycles correcting inside the code.

## Related
#3321, #2086 (both closed); `_audit-common.md` already flags feature-matrix lag generally, but this row is a specific wrong attribution rather than a stale status.

## Suggested Fix
Split the row: `.btr`/`.bto` for Skyrim+/FO4, `landscape\lod` quadtree (terrain + `blocks\` objects) for FO3/FNV, `DistantLOD\*.lod` + `_far.nif` for Oblivion only; drop the "multi-band selection deferred" clause.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related rows of `docs/feature-matrix.md` (other per-game LOD/terrain attributions)
