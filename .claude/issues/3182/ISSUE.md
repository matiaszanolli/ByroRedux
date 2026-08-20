# NIFAL-D1-2026-08-20-01: the mesh-water spawn block is 39 lines of verbatim copy-paste at both load paths — the duplication translate_material exists to prevent, re-created for a new category

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3182
Finding: NIFAL-D1-2026-08-20-01
Labels: medium,nif-parser,renderer,bug
Source: docs/audits/AUDIT_NIFAL_2026-08-20.md

Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 1 — Material, mesh-water slice). NIFAL canonical-translation finding — see `/audit-nifal` for the tier model this violates.

**Severity**: MEDIUM
**Tier violated**: `single-boundary`
**Game Affected**: all — Oblivion/FO3/FNV `WaterShaderProperty` + Skyrim+/FO4 `BSWaterShaderProperty`; `is_water_shader` is set from both `crates/nif/src/import/material/legacy_properties.rs:639` and `crates/nif/src/import/material/dedicated_shader.rs:612`.

**Location**: `byroredux/src/scene/nif_loader.rs:1027` and `byroredux/src/cell_loader/spawn/mesh_instance.rs:724`

## Read this together with its two siblings

This is one of **three** findings on the mesh-water slice that crossed NIFAL for the first time in this delta (`8110f359` -> `1a428278`). Together they say one thing: **mesh water got the derivations centralised but not the boundary discipline** — this is the exact duplication `translate_material` was created to prevent, re-created wholesale for a new category. The other two are the `WaterKind -> foam_strength` divergence and the four uncited constants (including a hardcoded world **+X** flow direction, which is the most actionable of the three). Fix them as one pass; they share the same fix site.

## Description

The individual *derivations* were centralised correctly — `water_material_from_mesh`, `water_kind_from_mesh_geometry` and `water_volume_from_mesh` all live once in `byroredux/src/material_translate.rs`. What was **not** centralised is the *composition*: which components get inserted, in what order, with what literals, and under which guard. That block is duplicated in full at both spawn sites.

Stripping comments and leading whitespace, the two blocks are **39 lines each** and differ **only** in the three placement-variable names (`translation`/`quat`/`mesh.scale` vs `final_pos`/`final_rot`/`final_scale`).

This is structurally identical to the pre-NIFAL state that `material_translate.rs`'s own module doc describes:

> *"the `Material` struct literal was built verbatim at two sites … kept in sync by hand. That duplication was itself a translation leak: a field added to one site and not the other silently diverged the two load paths."*

## Evidence

Normalised diff of `nif_loader.rs:1027-1065` against `mesh_instance.rs:724-772` (comments and indentation stripped):

```
1d0
< if mesh_water {
33,35c32,34
< translation,          > final_pos,
< quat,                 > final_rot,
< mesh.scale,           > final_scale,
39a39
> }
```

Everything else is byte-identical: the `WaterPlane { kind, material, damage_per_second: 0.0 }` literal, the `if let Some(flow)` insert, the `bound_center` construction, and the `water_kind != WaterKind::Waterfall` volume guard.

The `damage_per_second: 0.0` literal in particular is now **hardcoded twice** (`nif_loader.rs:1044`, `mesh_instance.rs:747`) while the ESM path resolves the same field from the WATR record (`byroredux/src/cell_loader/water.rs:492`, `:825`) — so the field that `06f84f0d` ("apply authored legacy damage") made authored on one path has two hand-written zeroes on the other.

## Impact

**No wrong value today** — the two blocks agree. What is broken is the *guarantee*. Every future mesh-water change (a fifth component, a damage source, a different waterfall-volume rule) has to be made twice with nothing to catch a miss: no compile error, and no test compares the two paths' component sets.

This is the same class as #2206 and #3072, both of which are live proof that the two load paths *do* drift when nothing forces them together.

## Suggested Fix

Add one

```rust
pub(crate) fn spawn_mesh_water(
    world, entity, material: &Material, mesh_name,
    positions, normal_idx, flow_idx,
    position, rotation, scale, local_center, local_radius,
)
```

to `byroredux/src/material_translate.rs` beside the four helpers it already owns, and call it from both sites. That is the declared boundary for this category (Dimension 9's "New categories must declare a boundary"), and it gives `damage_per_second` one home in which to grow a real source.

## Related

- Sibling findings from the same sweep: the `WaterKind -> foam_strength` divergence and the four uncited mesh-water constants.
- #3072 (`furniture: None` — same "one path does it, the other doesn't" shape).
- #2490 (raw-material -> marker-component block copy-pasted at both spawn sites — the identical defect for a different component group; OPEN). Kept separate because the fix sites differ.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the new helper lives at the NIFAL boundary (`byroredux/src/material_translate.rs`), per-game logic stays there, and nothing is re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: both spawn sites (`nif_loader.rs`, `spawn/mesh_instance.rs`) call the single helper — neither keeps a partial local copy
- [ ] **TESTS**: a regression test asserts the two load paths produce the same component set for the same water mesh (this is the guarantee that is currently missing entirely)
- [ ] **SPEC**: `docs/engine/nifal.md` gains the mesh-water boundary entry (currently zero occurrences of "water")
