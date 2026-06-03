# FO4-D8-NEW-03: BGSM fresnel_power not forwarded from merge_bgsm_into_mesh

**Severity**: MEDIUM  
**Source**: AUDIT_FO4_2026-06-02 (D8-NEW-03)  
**Location**: `byroredux/src/asset_provider.rs` — BGSM chain walk in `merge_bgsm_into_mesh`

`BgsmFile.fresnel_power` (`bgsm.rs:69`) has matching `ImportedMesh.fresnel_power` (`types.rs:626`) but is never forwarded. Vanilla FO4 default (5.0) matches ImportedMesh default, so no regression on stock content. Mod-authored BGSM with non-default Fresnel silently falls to 5.0.

**Fix**: Add child-first guard-and-set alongside existing scalars in the chain walk. Companion to #1455 (`grayscale_to_palette_scale`).
