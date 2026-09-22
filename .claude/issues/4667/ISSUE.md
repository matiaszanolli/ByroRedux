# PAR-D5-2026-09-21-03: Renderer-relevant BGSM/BGEM fields authored in vanilla are dropped without a documented deferral

Labels: low,bug,import-pipeline,game:fo4,game:fo76

## Description
`byroredux/src/asset_provider/material/merge.rs:913-927`'s #2704 comment documents 11 BGSM scalars as "deferred: no consumer", and #2642 separately documents distance-field alpha. Several other authored, non-default fields are also dropped but are undocumented in either ledger:

- FO76 `lum_emittance != 0`: 25,813 BGSM.
- FO76 `use_adaptive_emissive`: 3,272. BGEM `adaptive_emissive_final_exposure_max`: 4,101. Together these are the v22 emissive model.
- FO76 `base.depth_bias`: 230; `base.mask_writes != ALL`: 57.
- FO4 `decal_no_fade`: 356; `dissolve_fade`: 23; `glowmap`: 154 (FO4) / 289 (FO76).
- BGEM `falloff_color_enabled`: 2 / 107; `envmap_min_lod`: 11 / 19.
- `cast_shadows=false`: 327 / 778. The NIF-side `Cast_Shadows` equivalent is also unread.

Correctly ignored, for contrast: `receive_shadows=false` appears on 6,552/6,616 FO4 and 25,888/25,888 FO76 BGSMs, so it cannot plausibly mean "unshadowed" and is rightly left alone.

Verified unchanged at HEAD `ee6d3fb39`: the #2704 comment at `merge.rs:905-927` still lists only the original eleven scalars.

## Evidence
Field counts from probe `bgsm-fields` (`probe_bgsm_fields.log`).

## Impact
No per-field visual claim is made here (runtime semantics of each field are unverified against a reference renderer). The gap is that the "not yet wired vs overlooked" ledger #2704 created omits these fields entirely, so the next completeness sweep can't tell which bucket they belong to.

## Related
#2704 (the ledger this extends), #2642, #1077, PAR-D5-2026-09-21-02 (specular_enabled — a related but distinct BGSM-forwarding gap in the same file), #4430 (open — tracks the `glowmap` flag specifically from a different angle: NIF-flag unreliability, not the dropped-field documentation gap this finding is about)

## Suggested Fix
Extend the #2704 comment (or a doc table) with these fields and their vanilla counts, or wire the ones with clear renderer sinks (`depth_bias`, `mask_writes`).

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix