# EXT-D2-2026-10-26-02: #6256's default-LTEX cover classification is overridden to 0.0 on any localised load order

Labels: low,terrain-exterior,bug,game:fo4
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW (possibly intended; unconfirmed on game data) / **Dimension**: 2 / **Tier Violated**: n/a (ground-cover input, not a canonical translate value)
- **Game Affected**: FO4 (the case #6256 targets), Skyrim/FO3/FNV for BTXT-absent bases / **Location**: `byroredux/src/cell_loader/terrain.rs` `cover_density_inputs` (override block after `base_authored_grass`) and `authored_grass_for_splat_layers` / **Status**: NEW (adjacent to closed #6256)
- **Description**: #6256 maps an ATXT form-0 layer to `ltex_form_id = None` and classifies it by the default LTEX editor ID
  (`layer_cover_key` -> e.g. `LCWDefault01Grass01`, "a grass layer" on FO4). But when `AuthoredCover::localised` is true,
  `cover_density_inputs` replaces every lane's affinity with `cover.union_density(&authored_grass[lane])`, and
  `authored_grass_for_splat_layers` yields an empty list for `ltex_form_id == None` (its own doc: "empty = ... the executable's default land
  texture"). `union_density(&[])` is `0.0` ("authored no grass"), so the editor-ID classification only survives when the load order has no
  texture->grass links at all.
- **Evidence**: `resolve_authored_cover` sets `localised` whenever any `LTEX.GNAM` names a kept GRAS (true for vanilla Skyrim/FNV; FO4 not
  checked against data - no game-data run allowed). The #6256 test `atxt_form_zero_resolves_the_default_land_texture` asserts only
  `layer_cover_key` / `layer_affinity`, not the composed `CoverDensityInputs`.
- **Impact**: An FO4 default-texture layer that the fix intends to classify as grass grows no procedural blades on a localised load order
  (same for an absent/zero BTXT base). Whether the engine's built-in default texture should inherit a GNAM list is undecided: the
  default is an exe-side texture (no source for its grass links), so this may be correct; the fix and the override just disagree silently.
- **Related**: #6256 (closed), #4906, #4903, #6271 (the extraction that moved this code).
- **Suggested Fix**: Decide the semantics and pin them with a test on `cover_density_inputs` (localised + `ltex_form_id None`): either skip the
  override for `None` lanes (keep the editor-ID classification) or state in `layer_cover_key`'s doc that it is the non-localised fallback only.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
