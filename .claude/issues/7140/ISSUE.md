# #7140: PAR-D5-2026-11-20-03: The #6848 "remaining parsed-but-unmerged groups" ledger still omits seven parsed BGSM/BGEM groups

**Labels**: low,nifal,documentation,doc-rot,game:fo4,game:fo76
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7140

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D5-03 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Trigger Input**: parsed BGSM/BGEM fields with no `ImportedMaterial` sink and no ledger entry (measured in vanilla `Fallout4 - Materials.ba2` and `SeventySix - Materials.ba2`).
- **Location**: ledger at `byroredux/src/asset_provider/material/merge.rs:1473-1529`; fields in `crates/bgsm/src/base.rs`, `bgsm.rs`.
- **Status**: NEW (residue of FO4-2026-11-10-D2-04; #6848 closed with the comment claiming "the remaining parsed-but-unmerged groups")
- **Description**: Walking every `BaseMaterial` / `BgsmFile` / `BgemFile` pub field against the non-comment code of `material/{merge,provider,mod}.rs` and the rest of the binary leaves these fields with zero consumers and zero ledger mentions: `base.refraction` + `base.refraction_power` (BGSM and BGEM), `environment_mapping_window` / `environment_mapping_eye`, `hide_secret`, `displacement_texture_bias` / `displacement_texture_scale` and the `tessellation_pn_scale` / `tessellation_base_factor` / `tessellation_fade_distance` triple (only the `tessellate` flag is listed), and the FO76 `terrain` block (`terrain`, `terrain_unk_int1`, `terrain_threshold_falloff`, `terrain_tiling_distance`, `terrain_rotation_angle`). (`adaptive_emissive_final_exposure_min` is covered by the ledger's `adaptive_emissive_*` wording; `receive_shadows` is deliberately declined there; `distance_field_alpha_texture` is documented on the field, #2642.) `base.refraction` is the only renderer-relevant one: `material/mod.rs:251-262` explains why it must not mean glass but never says it has no sink.
- **Evidence**: probe `bgsmcount` (authored non-default counts): FO4 `refraction_power != 0`: 10 BGSM / 6 BGEM; `environment_mapping_window` 13, `environment_mapping_eye` 8, displacement texture slot 58 (that slot is bound at `merge.rs:930`); FO76 `base.refraction` 53 BGSM / 44 BGEM, `refraction_power != 0` 69 / 98, `terrain` 150 BGSM, displacement scale/bias and tessellation scalars non-zero on all 25,948 (parser default, not authored intent). Consumer greps: no `.refraction*`, `.terrain*`, `.tessellation_*`, `.displacement_texture_{bias,scale}`, `.environment_mapping_{window,eye}`, `.hide_secret` read anywhere in `byroredux/src` outside tests.
- **Impact**: Documentation only (no visual claim, as with #6848). The ledger's stated purpose is to let the next completeness sweep tell "not yet wired" from "overlooked"; refraction distortion (heat shimmer, force fields; FO76 69 + 98 files) is exactly such a candidate and is not recorded.
- **Related**: #6848, #2704, #4667, #2626 (refraction is not glass).
- **Suggested Fix**: Add the groups and counts above to the ledger comment (or `docs/engine/nifal.md`), and extend `bgsm_merge.rs`'s ledger pin, if it has one, so a new parser field cannot land unlisted.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
