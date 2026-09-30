# #4874: REN-D7-2026-09-24-04: denoise/resolve doc rot — TAA gamma 1.25, à-trous 'future', SSAO 'same frame', bloom 'Gaussian', stale composite.frag comments

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D7-2026-09-24-04**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: Denoiser/TAA/Bloom

`renderer.md` and `shadow-pipeline-tradeoffs.md` state a TAA `gamma = 1.25` (code: mean ± 1.5σ, #1108, with octahedral-normal rejection); `renderer.md` calls the shipped à-trous pass "future"; `ssao.rs` docs say the AO is read "the same frame" (the slot written two frames earlier; sibling of closed #2798); `shader-pipeline.md` calls the bloom downsample "Gaussian" (it is a 2×2 box); a stale `depth_params.z` comment in `composite.frag`, a `fog_params` UBO comment that should say *authored fog* near/far, and a `caustic_splat.comp` comment "composited after TAA".

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

