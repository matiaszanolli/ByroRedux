# #5230 — FO4-D2-01: the #3905 spawn neutral-roughness pass overwrites the glass classifier's mirror roughness (0.04 → 0.5) on BGSM mirror panes

https://github.com/matiaszanolli/ByroRedux/issues/5230

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D2-01)

- **Severity**: LOW (latent; 0 vanilla FO4 shapes and 0 FO76 shapes). It escalates to HIGH under the NIFAL divergent-`Material` rule the day a BGSM mirror pane exists.
- **Dimension**: BGSM/BGEM merge (the #3639/#3905 neutral-roughness pair)
- **Location**:
  - `byroredux/src/material_translate.rs:1360-1379` (`unresolved_gloss_neutral_roughness`).
  - `byroredux/src/helpers.rs:11,183-196` (`MIRROR_ROUGHNESS = 0.04`, the mirror-pane branch).
  - The spawn call sites: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1367` and `byroredux/src/scene/nif_loader.rs:1504`.
- **Status**: NEW. The interaction is pre-existing: 7e2abb730 (#3905) landed on top of the mirror branch e5d02f83d. No matching issue exists.
- **Description**:
  - The spawn pass infers "the BGSM's `(1 − smoothness)` hit the clamp floor" from `roughness <= 0.04`.
  - The glass classifier's mirror-pane branch also writes exactly 0.04 (with metalness 1.0) on a kind-0 material, and `from_bgsm` does not block it.
  - The spawn pass then runs later, sees 0.04 with `bgsm_pbr_scalars_authored` and no gloss handle, and rewrites roughness to 0.5.
  - The result is metalness 1.0 / roughness 0.5: a blurred chrome sheet that is neither authored nor the classifier's mirror.
- **Evidence**: `helpers.rs:183-195` versus `material_translate.rs:1369-1378`; no test combines a mirror pane with `bgsm_pbr_scalars_authored`. FO4 corpus probe, 226,068 NIFs: 26 mirror-named shapes, 0 mirror panes. Vanilla BGSM mirrors are authored kind 1 (EnvironmentMap), and the branch returns early for those. FO76: 62 mirror-named NIFs, 0 panes.
- **Impact**: none on vanilla. Modded BGSM panes named `*mirror*` on a glass-keyword texture with no smooth-spec map would hit it.
- **Related**: #3639, #3905, #4255, #5012.
- **Suggested Fix**: carry an explicit "roughness at the BGSM floor" provenance bit from the merge, and gate the spawn pass on that rather than on the value. Alternatively, have the classifier's forced mirror state exempt itself. Pin it with a BGSM mirror-pane fixture.

## Completeness Checks
- [ ] **TESTS**: a BGSM mirror-pane fixture (kind 0, mirror name, `bgsm_pbr_scalars_authored`, no gloss handle) asserts the classifier's 0.04 survives the spawn pass
