# #3797 — LC-2026-08-30-D7-01: AlphaFlags bit 13 'No Sorter' is parsed into the flags word and never decoded, so the engine back-to-front-sorts every alpha-over draw including the ones the author opted out of

**Severity**: LOW as filed, escalates to MEDIUM if a census finds authored occupancy
**Location**: `crates/nif/src/import/material/mod.rs` (`apply_alpha_flags`), `byroredux/src/render/mod.rs` (`draw_sort_key` slot 3)
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-30.md` (LC-2026-08-30-D7-01)

`apply_alpha_flags` decoded five of `NiAlphaProperty`'s eight flag members. Bit 13 (mask
0x2000, "No Sorter" — Gamebryo's per-property instruction to `NiAlphaAccumulator` to draw in
accumulation order rather than depth-sorted) was never read, and Redux's alpha-over sort key
puts `!sort_depth` unconditionally in slot 3 — a shape whose author disabled the sorter was
sorted anyway.

**Suggested Fix** (in order): (1) census `flags & 0x2000` across Oblivion/FO3/FNV mesh
archives before any code change — severity depends on it; (2) if non-zero, surface as
`MaterialInfo.no_sorter` → canonical → `DrawCommand`, make slot 3 of the alpha-over key
`(!no_sorter, !sort_depth)`; (3) if zero, record the measurement and stop.

## Census result (2026-09-02, throwaway `crates/nif/examples/_tmp_no_sorter_census.rs`, deleted after use)

- **Oblivion** (`Oblivion - Meshes.bsa`): 8032 NIFs scanned, 1315 `NiAlphaProperty` instances,
  **74 meshes / 76 properties set the bit** — concentrated in Ayleid ruin interiors
  (`meshes\dungeons\ayleidruins\...`), shipwrecks (`meshes\architecture\ships\...`), and
  moss/rubble clutter (`meshes\rocks\westweald\...`).
- **FO3** (`Fallout - Meshes.bsa`): 10989 NIFs, 9353 `NiAlphaProperty` instances, **0 set**.
- **FNV** (`Fallout - Meshes.bsa`): 14881 NIFs, 11143 `NiAlphaProperty` instances, **0 set**.

Non-zero occupancy on Oblivion — escalated to the fix path per the issue's own criteria.

## Fix implemented

Threaded `no_sorter: bool` through the full NIFAL boundary, matching `two_sided`'s existing path:

- `crates/nif/src/import/material/mod.rs`: `MaterialInfo::no_sorter`, decoded in
  `apply_alpha_flags` (`alpha.flags & 0x2000 != 0`).
- `crates/nif/src/import/types.rs`: `ImportedMaterial::no_sorter`, carried through the
  `MaterialInfo` → `ImportedMaterial` builder.
- `byroredux/src/components.rs`: new `NoSorter` marker component (mirrors `TwoSided`).
- `byroredux/src/material_translate.rs`: `attach_blend_and_facing_markers` attaches `NoSorter`
  when `raw.no_sorter`.
- `byroredux/src/render/static_meshes.rs`: queries the `NoSorter` marker, sets
  `DrawCommand.no_sorter`.
- `byroredux/src/render/particles.rs`: `no_sorter: false` (procedural billboards have no
  `NiAlphaProperty` — SIBLING check per the issue's own checklist, no change to particle
  batching behavior).
- `crates/renderer/src/vulkan/context/mod.rs`: `DrawCommand::no_sorter` field.
- `byroredux/src/render/mod.rs`: `draw_sort_key` grew a new slot 3 (`no_sorter` partition,
  12-tuple total). True alpha-over draws with `no_sorter=true` fall back to the SAME
  state-clustered shape the additive branch uses (render_layer/two_sided/blend/depth_state/mesh
  dominate; `sort_depth` only a within-mesh tiebreaker) instead of the global back-to-front
  order — `DrawCommand` has no real "file order" data source (`#2459`), so this is the closest
  available approximation to "accumulation order" without threading new data through the NIF
  import pipeline. The two partitions (sorted vs. no_sorter) do not interleave by raw depth
  against each other.
- `byroredux/src/save_io/registry_completeness_tests.rs`: allowlisted the new `NoSorter`
  marker (rederived identically from NIF data every load, same posture as `TwoSided`).

Regression tests added: `no_sorter_bit_sets_flag` / `no_sorter_bit_absent_leaves_flag_false` /
`no_sorter_bit_coexists_with_alpha_blend` (parser), `no_sorter_flag_attaches_the_no_sorter_marker`
/ `no_no_sorter_flag_attaches_no_marker` (marker translation),
`no_sorter_draw_does_not_follow_the_depth_sort` / `no_sorter_same_mesh_draws_stay_contiguous_for_instancing`
(sort key — the issue's own TESTS checklist item).
