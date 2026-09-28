# #4965: REN-D6-2026-09-27-03: #4566's Oblivion PBR-override ceiling (99%) sits below Oblivion's measured fill (100%) — the corpus guard is red at HEAD

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4965
- **Labels**: low,nifal,test-gap,game:oblivion,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D6-2026-09-27-03**._

- **Severity**: LOW. The test is opt-in and fails false; no runtime effect.
- **Dimension**: NIFAL Material (owner `/audit-nifal` Dim 9)
- **Location**: `crates/nif/tests/translation_completeness.rs:478` (Oblivion lane, `assert_pbr_override_ceiling(s, label, 99.0)`).
- **Status**: NEW. It was introduced by the #4566 fix `57b852717`.
- **Description**: `57b852717` claims each ceiling sits "~10pp above its measured fill". Oblivion's measured `metO`/`rghO` is 100.0%: every mesh has a `NiMaterialProperty`, hence `specular_authored`, hence a classifier signal. That matches the historical 100% in `AUDIT_NIFAL_2026-06-13/06-28`. So the ceiling can never pass on a machine with Oblivion data.
- **Evidence**: My run at HEAD printed `Oblivion meshes= 567 … metO=100.0% rghO=100.0%` and then panicked with `[Oblivion] metalness_override fill > 99.0% (got 100.0%)`. The other 7 rows fit their ceilings: FO3 94.3, FNV 96.7, SkyrimLE 92.4, SkyrimSE 93.8, FO4 99.4, FO76 15.8, Starfield 5.1.
- **Impact**: The upward-drift guard, and every assertion after it in the harness, cannot run green for anyone with Oblivion installed. The rows after Oblivion never reach their assertions.
- **Related**: #4566, #4393, #2707.
- **Suggested Fix**: Set the Oblivion ceiling to 100.0 (by construction), or better, assert the exact structural reason (`specular_authored` on every Oblivion mesh) and ceiling the other signal classes separately.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
