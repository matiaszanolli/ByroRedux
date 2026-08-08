# FO4-D2-2026-08-07-01: fill_from_bgsm forwards only 6 of 11 BGSM/BGEM texture roles

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2594
**Finding ID**: FO4-D2-2026-08-07-01

**Severity**: MEDIUM
**Dimension**: 2 (Materials)
**Location**: `byroredux/src/cell_loader/refr.rs:185-225` (`fill_from_bgsm`)
**Status**: NEW

## Description
`RefrTextureOverlay::fill_from_bgsm` forwards only 6 of the 11 texture roles
that `merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
covers for the same BGSM/BGEM inputs. For `.bgsm`: it fills diffuse, normal,
glow, specular (from `smooth_spec_texture`), env, and height — but drops
`lighting_texture`, `flow_texture`, `wrinkles_texture`, and `greyscale_texture`
entirely. For `.bgem`: it fills normal, glow, env — but drops the specular
and lighting texture roles, and never treats BGEM's greyscale texture as an
LUT the way the canonical merge path does.

## Evidence
```rust
// byroredux/src/cell_loader/refr.rs:200-216 (.bgsm arm)
Self::fill(&mut self.diffuse, Some(f.diffuse_texture.as_str()), pool);
Self::fill(&mut self.normal, Some(f.normal_texture.as_str()), pool);
Self::fill(&mut self.glow, Some(f.glow_texture.as_str()), pool);
Self::fill(&mut self.specular, Some(f.smooth_spec_texture.as_str()), pool);
Self::fill(&mut self.env, Some(f.envmap_texture.as_str()), pool);
Self::fill(&mut self.height, Some(f.displacement_texture.as_str()), pool);
// lighting_texture, flow_texture, wrinkles_texture, greyscale_texture: never forwarded

// .bgem arm (:221-223) only forwards normal/glow/env — specular and lighting dropped
```
`merge_external_material` (`byroredux/src/asset_provider/material.rs:659`)
covers the full role set for the same inputs, so this is a real divergence
between the two BGSM/BGEM consumption paths, not an intentional narrower
contract.

## Impact
Reachable only when a REFR's XATO/XTNM TextureSet override is MNAM-only
(i.e. the override resolves through `fill_from_bgsm` rather than the
canonical merge path). In that case, the dropped texture roles silently
never reach the overlay — e.g. a REFR-level BGSM override with a wrinkle map
or flow map renders as if that texture didn't exist, with no diagnostic.

## Suggested Fix
Extend `fill_from_bgsm` to forward the same texture-role set
`merge_external_material` does (lighting/flow/wrinkles/greyscale for BGSM;
specular/lighting/greyscale-as-LUT for BGEM), or factor the role list into a
shared helper both call so they can't drift again.

## Related
FO4-D2-2026-08-07-02 (zero test coverage on this function — precisely why
this went unnoticed).

## Completeness Checks
- [ ] **SIBLING**: Check `merge_external_material`'s BGEM greyscale-as-LUT handling is mirrored, not just the texture assignments
- [ ] **TESTS**: A regression test pins the full role set forwarded by `fill_from_bgsm` (see FO4-D2-2026-08-07-02)
