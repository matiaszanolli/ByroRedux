# #4433: NIFAL-D8-2026-09-16-06: `MaterialInfo::texture_set` says inline NIF shaders never expose the standalone `specular` role, which has been false since #2998/#3085

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4433
- **Labels**: low,nifal,documentation,doc-rot
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (doc)
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: — (doc)
- **Game Affected**: FO4, FO76, Skyrim (model-space-normal slot 7)
- **Location**: `crates/nif/src/import/material/mod.rs:1332-1334`
- **Status**: NEW
- **Description**: The comment "Standalone BGSM/BGEM roles are populated by the downstream material-file translator; inline NIF shaders do not expose them." sits above `specular: self.specular_map`. It dates from `1d94eb24` and predates `79202bfc4`. `slot_to_role` now routes FO4 slot 7, FO76 slot 6 and Skyrim MSN slot 7 into `TextureRole::Specular`, and `apply_bs_lighting_shader` writes those into `info.specular_map` (`crates/nif/src/import/material/dedicated_shader.rs:260`).
- **Evidence**: `git blame` shows the comment from `1d94eb246` and the `specular` line from `79202bfc4`.
- **Impact**: A reader auditing role provenance would conclude that an inline `specular` value must be a BGSM leak, the wrong premise for the `smooth_spec`/`specular` mis-merge check this dimension exists to make.
- **Related**: #2998, #3085, #2742
- **Suggested Fix**: Move the comment to the roles it still describes (`flow`, `glass_*`), and note that `specular` has inline NIF producers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
