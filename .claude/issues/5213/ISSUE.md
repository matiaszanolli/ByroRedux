# #5213 — REN-D7-2026-10-03-03: denoise/resolve doc residue that #4874 and its predecessors missed — retracted Halton "LCM-6 period" rationale in `renderer.md`, and a phantom bindless STORAGE_IMAGE row in `shader-pipeline.md`

**Labels**: low,renderer,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `docs/engine/renderer.md` TAA "Per-frame flow" step 1; `docs/engine/shader-pipeline.md` `## Descriptor Sets` table, row `0 | 1`
- **Status**: NEW (residual of #3606 and #4019, both closed)
- **Description**:
  1. `renderer.md` still says the jitter has "period 16 — #1093 — chosen as the nearest power of two above the natural LCM-6 period". `taa_jitter`'s own doc in `frame_params.rs` carries the 2026-08-31 correction: Halton sequences are aperiodic, so there is no LCM-6 period and the real motivation for 16 is an open question. #3606 fixed the two code sites. #4874 rewrote steps 2–3 of this same paragraph but left step 1.
  2. The descriptor table lists `0 | 1 | STORAGE_IMAGE (bindless) | Per-pass read/write images | bloom, svgf, taa`. The bindless layout (`build_bindless_descriptor_bindings`) is two `COMBINED_IMAGE_SAMPLER` arrays: binding 1 is the environment cubemap array (`include/bindings.glsl`: `layout(set = 0, binding = 1) uniform samplerCube cubemaps[]`). Bloom, SVGF and TAA bind only their private set 0, where binding 1 is `uMotion`/`motionTex`/`dst`. #4019 corrected the caustic/volumetrics credits in this table but not this row, which dates to 78540d8ef.
- **Evidence**: `grep -n "LCM" docs/engine/renderer.md` → step 1. `build_bindless_descriptor_bindings` → `[0, 1].map(… COMBINED_IMAGE_SAMPLER …)`.
- **Impact**: Audit and onboarding only. This is the table the skills tell auditors to trust instead of re-deriving descriptor facts.
- **Related**: #3606, #4019, #4874.
- **Suggested Fix**: Replace step 1's rationale with "period 16 (#1093; motivation open, see `taa_jitter`)". Change row `0|1` to `COMBINED_IMAGE_SAMPLER` (bindless `samplerCube` array) used by `triangle` (and any other `bindings.glsl` includer).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
