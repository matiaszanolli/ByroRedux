# Batch: #2328, #2329

## #2328 — FO3-D1-06: Inherited-property precedence inversion

**Severity**: LOW
**Domain**: nif (`byroredux-nif`)
**Location**: `crates/nif/src/import/material/legacy_properties.rs`

`apply_legacy_property_chain`'s documented intent is "shape properties first so
they take priority" (#208), implemented via `is_none()`/`has_…` guards
everywhere except `texture_clamp_mode`/`env_map_scale`, which used a bare `=`
in all seven shader branches — an inherited parent-NiNode property could
silently overwrite the shape's own authored value.

### Fix
- Added `MaterialInfo::texture_clamp_mode_consumed` / `env_map_scale_consumed`
  bools mirroring `alpha_property_consumed`.
- Gated every `texture_clamp_mode`/`env_map_scale` write in
  `apply_pp_lighting_property`, `apply_no_lighting_property`,
  `apply_misc_shader_properties` (Tile/Sky/TallGrass/Water), and
  `apply_base_only_shader_property` on the corresponding `_consumed` flag,
  set to `true` immediately after each write.
- Left the pre-existing `NiTexturingProperty` ad-hoc `== 3` guard in
  `apply_texturing_property` untouched — it's a different, already-correct
  mechanism for that one call site and wasn't among the bare-`=` sites the
  issue cited; touching it risks unrelated behavior drift on the most
  heavily-exercised legacy texture path for no reported bug.
- Added `legacy_property_precedence_tests.rs` (2 tests): shape's own value
  survives an inherited override, and the inherited value still fills the gap
  when the shape has no property of its own.

## #2329 — FO3-D2-03: BSSegmentedTriShape segment table has no bounds check

**Severity**: LOW
**Domain**: nif (`byroredux-nif`)
**Location**: `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs` (`parse_segmented`)

Wire layout is correct (zero drift across 17,172 real FO3 NIFs). Two residual
gaps noted: (a) every segment field is read into `let _` — benign, FO3
dismemberment is driven by `BSDismemberSkinInstance` instead; (b) `num_segments`
had no sanity bound before the loop, unlike the sibling `parse_mesh_emitter`'s
`check_alloc`.

### Fix
Per the issue's own suggested fix (address (b) only — (a) is explicitly benign
and out of scope), added `stream.check_alloc((num_segments as usize)
.saturating_mul(9))?` before the loop, mirroring `parse_mesh_emitter`'s
pattern exactly. Added
`bs_segmented_tri_shape_rejects_corrupt_num_segments` pinning that a
`u32::MAX` segment count is rejected up front rather than walked to EOF.
