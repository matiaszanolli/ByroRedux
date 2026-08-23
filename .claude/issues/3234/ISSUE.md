# 3234: NIFAL-D8: fill_from_bgsm binds smoothness mask into specular role, drops real specular_texture

**Severity**: HIGH · **Dimension**: NIFAL Shader-flags/Effects (texture-role vocabulary) · **Report**: `docs/audits/AUDIT_NIFAL_2026-08-23.md` (NIFAL-D8-2026-08-23-01)

## Description

BGSM defines two distinct texture fields: `smooth_spec_texture` (always read — "smoothness in alpha, specular RGB") and `specular_texture` (`version > 2` only — "standalone specular, PBR-style separate"). The canonical single boundary for external material merge, `merge_external_material` (`byroredux/src/asset_provider/material.rs:1331-1338`, `:1396-1406`), keeps these separate, exactly per the role docs in `crates/nif/src/import/types.rs:314-324`.

`RefrTextureOverlay::fill_from_bgsm` (`byroredux/src/cell_loader/refr.rs:246-255`) — the **second**, independent BGSM→role resolver that lets REFR `XATO`/`XTNM` overrides and, since `900aa081` (#973), per-shape `XMSP` material swaps reach texture roles — gets this backwards on its `.bgsm` arm: it routes `smooth_spec_texture` into `self.specular` and never reads `specular_texture` at all. The `.bgem` arm is correct (BGEM has only one `specular_texture` field).

The wrong value is not inert: `self.specular` is consumed at `mesh_instance.rs:236-241` via `pick(7, o.specular, TextureRole::Specular)`, and for `TextureSlotLayout::Fallout4` (every FO4/FO76/Starfield NIF, post-#3186) slot 7 → `Specular` **unconditionally** — no gate blocks it.

## Evidence

```rust
// crates/bgsm/src/bgsm.rs:26-34 — the two distinct fields
pub smooth_spec_texture: String,   // always read
// v > 2:
pub specular_texture: String,      // standalone, PBR-style

// byroredux/src/asset_provider/material.rs:1331-1338, :1396-1406 — canonical, CORRECT
fill(&mut material.textures.smooth_spec, &bgsm.smooth_spec_texture, ...);
fill(&mut material.textures.specular, &bgsm.specular_texture, ...);

// byroredux/src/cell_loader/refr.rs:251-255 — overlay path, WRONG
Self::fill(&mut self.specular, Some(f.smooth_spec_texture.as_str()), pool);
// f.specular_texture is never referenced anywhere in this function's .bgsm arm.
```

The regression-test fixture (`byroredux/src/cell_loader/refr_texture_overlay_tests.rs:507-561`, `fill_from_bgsm_forwards_every_bgsm_texture_role`) currently pins the bug as correct — no fixture in the file ever sets `specular_texture`, so the dropped-field half has zero coverage in either direction.

## Impact

Any FO4/FO76/Starfield REFR resolving through `fill_from_bgsm` gets its specular-colour channel bound to a grayscale-ish smoothness mask (sampled as an RGB tint via `specColor *= texture(...).rgb` at `triangle.frag:383-388`) instead of modulating roughness via `glossMapIndex`, and loses its authored standalone specular-colour map outright when one exists. `smooth_spec_texture` is read unconditionally at every BGSM version, so this is the base/legacy slot, not a rare edge case. `900aa081` (#973) measurably **widens reachability** today from a single REFR-level `material_path` to every shape of a multi-shape mesh whose MSWP swap target is a `.bgsm` ("Raider armour colour variants, station-wagon rust patterns, Vault decay overlays" per that commit's own description). No render-time fallback masks this.

## Related

#2594 (added the `inner`/`lighting`/`flow`/BGEM-specular forwarding to this same function but did not touch or question the pre-existing `smooth_spec_texture → specular` line); #2595 (closed as "zero test coverage" — the tests that closed it are exactly the mirror-test fixtures above, which encode the bug rather than catching it); #1076; `900aa081`/#973 (widened reachability without touching the defect).

## Suggested Fix

Add `Self::fill(&mut self.specular, Some(f.specular_texture.as_str()), pool)` reading the correct field in the `.bgsm` arm; either add a `smooth_spec` field to `RefrTextureOverlay` for the gloss mask, or drop the `smooth_spec_texture` read entirely if a REFR-level gloss override is out of scope. Then fix `fill_from_bgsm_forwards_every_bgsm_texture_role` to set both fields to different values and assert each lands in its own role.

## Completeness Checks
- [ ] **SIBLING**: Check no other second-resolver function in this file has the same smooth_spec/specular mix-up
- [ ] **TESTS**: Corrected fixture asserting `smooth_spec_texture` and `specular_texture` land in distinct fields
- [ ] **CANONICAL-BOUNDARY**: Confirm the fix keeps `fill_from_bgsm` behavior in lockstep with `merge_external_material`'s role assignment
