# FO4-2026-11-10-D2-01: the "matches the classifier neutral" claim survives in two sibling sites after #6746

Labels: low,documentation,doc-rot,import-pipeline,game:fo4,legacy-compat
Source: docs/audits/AUDIT_FO4_2026-11-10.md

- **Severity**: LOW
- **Dimension**: 2 (BGSM merge, doc rot)
- **Location**: `byroredux/src/asset_provider/material/merge.rs:1520-1523`; `byroredux/src/asset_provider/tests/bgsm_merge.rs:1046-1049`
- **Status**: NEW (sibling of closed #6746; its "SIBLING: same wording checked in related files" box was left open. The 0.5-vs-0.85 value question is open as #6818 and not re-filed.)
- **Description**: #6746 corrected the doc on `NEAR_MIRROR_NEUTRAL_ROUGHNESS` (it is 0.5 = `Material::default()` / the skin-body arm, **not** the classifier's no-signal terminal `PbrMaterial::NO_SIGNAL_NEUTRAL` = 0.85). The same false sentence is still in the merge arm comment ("`0.5` matches the neutral roughness `classify_pbr_keyword`'s arms already use elsewhere ... rather than inventing a second 'no data' convention") and in the assertion message of `bgsm_merge_falls_back_to_neutral_roughness_when_smoothness_is_one_with_no_gloss_map` ("fall back to the neutral roughness classify_pbr_keyword's arms use").
- **Evidence**: `grep -n "arms already use\|arms use" merge.rs bgsm_merge.rs` returns exactly these sites; `crates/core/src/ecs/components/material.rs:894` defines `NO_SIGNAL_NEUTRAL` (0.85).
- **Impact**: comment and message only. A reader is told 0.5 is the shared no-data convention, which is the premise #6818 asks someone to measure.
- **Suggested Fix**: reword both to the post-#6746 wording ("a chosen mid roughness, equal to `Material::default()`; see #6818").
