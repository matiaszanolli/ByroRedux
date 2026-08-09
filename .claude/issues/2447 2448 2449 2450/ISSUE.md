# Issue batch: 2447, 2448, 2449, 2450

## #2447 — PHYS-01 (MEDIUM, nif-parser/legacy-compat)
`extract_ragdoll` applies `BhkRigidBody` CInfo translation/rotation
unconditionally, bypassing the `is_t` gate `extract_from_classic` requires
(fixed for architecture under #2316).
- `crates/nif/src/import/collision/ragdoll.rs:90-104`
- contrast `crates/nif/src/import/collision/mod.rs:316-334`
  (`extract_from_classic`)

Fix: mirror the `is_t` gate in `extract_ragdoll`.

## #2448 — PHYS-02 (MEDIUM, nif-parser/legacy-compat)
`bhkLimitedHingeConstraintCInfo`'s authored perp-axis zero-reference
vectors are decoded then discarded — every elbow/knee joint's angle
limits apply around a synthesized reference frame instead of the
authored one.
- `crates/nif/src/blocks/collision/constraints.rs:150-218` (decode,
  byte-pinned — has the perp fields)
- `crates/nif/src/import/collision/ragdoll.rs:324-333`
  (`limited_hinge_joint` — perp fields never read)
- `crates/nif/src/import/types.rs:1026-1033` (no perp field on
  `ImportedJointKind::LimitedHinge`)
- `crates/physics/src/ragdoll.rs:381-386` (`build_joint` synthesizes an
  arbitrary perp via `any_perp(axis)`)

Fix: thread `perp_axis_in_a1`/`b1` through `ImportedJointKind::LimitedHinge`
and use as authored secondary axis in `build_joint`'s `frame_rot`.

## #2449 — EXAL-01 (MEDIUM, renderer/import-pipeline/legacy-compat)
WRLD NAM3/NAM4 LOD-water is parsed but no LOD-ring water plane is ever
spawned — "dry ocean" beyond streaming radius.
- `crates/plugin/src/esm/cell/wrld.rs:151-169`
- `crates/plugin/src/esm/cell/mod.rs:844-861`
- `byroredux/src/cell_loader/water.rs:77`, `terrain_lod.rs`, `object_lod.rs`

Fix: add `translate_lod_water(wrld)` to `env_translate.rs`, spawn one large
`IsLodTerrain`-marked water quad per ring clipped to exclude full-detail
radius.

## #2450 — EXAL-02 (MEDIUM, import-pipeline/legacy-compat)
Worldspace climate resolution ignores the WRLD parent chain
(WNAM/PNAM) — child worldspaces silently fall back to procedural Mojave
sky.
- `byroredux/src/cell_loader/exterior.rs:799-817`
- `crates/plugin/src/esm/cell/wrld.rs:215-217`
- `crates/plugin/src/esm/cell/mod.rs:821-826`

Fix: chase `parent_worldspace` when no own CNAM exists, gated on the PNAM
inherit bit, inside `env_translate.rs`; log at `warn` when chain
terminates unresolved.
