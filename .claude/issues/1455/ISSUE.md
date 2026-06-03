# FO4-D8-NEW-04: BGSM grayscale_to_palette_scale not forwarded from merge_bgsm_into_mesh

**Severity**: MEDIUM  
**Source**: AUDIT_FO4_2026-06-02 (D8-NEW-04)  
**Location**: `byroredux/src/asset_provider.rs` — BGSM chain walk in `merge_bgsm_into_mesh`

`BgsmFile.grayscale_to_palette_scale` (`bgsm.rs:135`) has matching `ImportedMesh.grayscale_to_palette_scale` (`types.rs:611`) but is never forwarded. Affects NPC/creature colour-variant palette intensity when non-default (deathclaw, supermutant skin tone variants).

**Fix**: Add child-first guard-and-set. Companion to #1454 (`fresnel_power`).
