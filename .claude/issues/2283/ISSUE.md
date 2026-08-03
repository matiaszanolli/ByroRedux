# NIF-D4-01: BsTriShapeKind::LOD triangle-count cutoffs still unreachable — regression of closed #1207

**Severity**: MEDIUM
**Dimension**: Geometry Handoff
**Game Affected**: SkyrimSE / FO4 (the two games shipping distant-LOD tri-shape wire blocks)
**Location**: `crates/nif/src/blocks/mod.rs:453-456`; `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:568-591`; `crates/nif/src/import/mesh/bs_tri_shape.rs:204-207`; `crates/nif/src/import/walk/mod.rs:477-500,1113-1138`
**Source**: docs/audits/AUDIT_NIF_2026-08-03.md (NIF-D4-01) — regression of #1207 (closed; the original fix never actually fired on real dispatch content)

## Description

Two distinct wire block names dispatch to two different Rust representations. `"BSLODTriShape"` (SkyrimSE) parses as `NiLodTriShape`, a classic-`NiTriShape`-bodied struct carrying its own `lod0_size`/`lod1_size`/`lod2_size` fields that are never read past the parser struct. `"BSMeshLODTriShape"` (FO4/SkyrimSE-DLC) parses via `BsTriShape::parse_lod()` — the only producer of `BsTriShapeKind::LOD{lod0,lod1,lod2}` anywhere in the crate — but the dispatcher immediately overwrites that value with `.with_kind(BsTriShapeKind::MeshLOD)` one line later. So `BsTriShapeKind::LOD` is constructed and discarded in the same expression on every real parse and can never be the persisted `kind` the importer sees. `import/mesh/bs_tri_shape.rs`'s `extract_bs_tri_shape` matches on exactly that unreachable variant to populate `bs_lod_cutoffs`; `NiLodTriShape`-typed shapes never even reach that function (the walker unwraps to the inner classic `NiTriShape` and calls the `ni_tri_shape.rs` extractor, which hardcodes `bs_lod_cutoffs: None`).

## Evidence

`grep -rn "BsTriShapeKind::LOD" crates/nif/src` shows the only non-doc, non-test production hit outside the enum definition is `bs_tri_shape.rs:589` inside `parse_lod`, whose sole caller (`blocks/mod.rs:454-456`) discards the value via `.with_kind(MeshLOD)`. The #1207 regression test (`lod_kind_surfaces_three_cutoffs`) hand-constructs a `BsTriShape` fixture with `kind: BsTriShapeKind::LOD{...}` directly, bypassing the real block dispatcher entirely — so it passes without ever exercising the path that would catch the gap. The #988 regression test (`bs_lod_tri_shape_imports_geometry_not_dropped`) builds a `NiLodTriShape` fixture with populated LOD sizes but only asserts the mesh renders, never that the values reach `ImportedMesh`.

## Impact

No present-day rendering regression — `grep -rn "bs_lod_cutoffs" byroredux/src` returns no consumers yet; the field is documented as "for an eventual M35 LOD selector." The blast radius is entirely forward-looking: when that selector is built, it will silently receive `None` for every mesh on both games, with no authored cutoff data to work from — the exact gap #1207 was opened to close, still open in practice.

## Suggested Fix

Either thread `NiLodTriShape`'s own `lod0_size`/`lod1_size`/`lod2_size` into `bs_lod_cutoffs` from the `NiLodTriShape` walker branch (the wire type that actually carries them on real SkyrimSE content), or drop the `.with_kind(MeshLOD)` override / add a `MeshLOD{lod0,lod1,lod2}` variant so `BSMeshLODTriShape`'s parsed cutoffs survive to import. Either way, rewrite the #1207/#988 regression tests to drive the real dispatcher on a synthetic `"BSLODTriShape"`/`"BSMeshLODTriShape"` block rather than hand-built fixtures, so they can't keep passing against an unreachable code path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other tri-shape LOD variants)
- [ ] **TESTS**: Rewrite #1207/#988 regression tests to drive the real block dispatcher, not hand-built fixtures
