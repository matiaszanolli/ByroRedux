# #3547: SK-D6-02: PLACEMENT_LOD_RADIUS_CELLS' rustdoc links to the deleted OBJECT_LOD_RADIUS_CELLS and states a relationship that no longer holds

**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-30.md` — Dimension 6 (Specialty Blocks + Distant LOD)
**Severity**: LOW
**Location**: `byroredux/src/cell_loader/placement_lod.rs` — `PLACEMENT_LOD_RADIUS_CELLS` doc comment

## Description

`PLACEMENT_LOD_RADIUS_CELLS` is documented as *"Mirrors
[`super::object_lod::OBJECT_LOD_RADIUS_CELLS`]"*. That item **no longer exists anywhere in
the tree**, so this is a rustdoc intra-doc link to a deleted symbol, and the stated
relationship is false.

## Evidence

Verified 2026-08-30:

```
$ grep -rn "OBJECT_LOD_RADIUS_CELLS" --include='*.rs' .
byroredux/src/cell_loader/placement_lod.rs:74:/// [`super::object_lod::OBJECT_LOD_RADIUS_CELLS`]; the placement scheme is
```

One hit — the doc link itself. The constant it names is gone: the flat object-LOD ring was
replaced by the quadtree band ladder in `lod_bands.rs`, which for Skyrim streams levels
**4 / 8 / 16** out to `max_cells = 250 000 / 4096 ≈ 61` cells. The placement scheme
(Oblivion `.lod`, FO3/FNV legacy blocks) is still a flat 16-cell ring, so the two schemes no
longer mirror each other at all.

Corroborating corpus measurement — Skyrim's shipped `.bto` object-LOD set is **744 level-4 /
248 level-8 / 86 level-16 quads** out of 1,078, i.e. a level-4-only reader would drop 334
quads (31%). The ladder is doing real work; the doc describes the scheme it replaced.

## Impact

A broken intra-doc link (rustdoc emits a resolution warning) plus a false statement about how
the two LOD schemes relate — the exact pair of facts a reader consults this constant to
learn.

## Suggested Fix

One-line doc correction: state the flat 16-cell ring is the placement scheme's own choice,
and point at `lod_bands.rs` for the baked-`.bto`/`.btr` band ladder that replaced the old
ring. (The audit skill's matching stale description is noted separately and is not part of
this issue.)

## Related

Skyrim LOD band ladder verified clean against the game's own `Ultra.ini` in the same audit
(SK-D6-01) — the code is correct; only the doc drifted.

## Completeness Checks
- [ ] **SIBLING**: check `object_lod.rs` and `lod_bands.rs` for other references to the deleted constant or the flat-ring model
- [ ] **TESTS**: a doc-only change; a `cargo doc` run with no broken-intra-doc-link warning is the gate
