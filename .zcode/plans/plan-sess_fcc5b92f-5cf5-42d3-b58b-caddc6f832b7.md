# Volumetric fire/smoke/explosion visual quality — implementation plan

**Goal:** make the transported combustion system read as physical rather than procedural, via three ranked waves: (1) spectral soot + anisotropic local-light phase, (2) multiple-scattering energy compensation for dense smoke, (3) single-pass BFECC advection correction + turbulence retune. Each wave lands as its own commit with its own A/B gate. The plan ends with a durable `--combustion-lab` golden-frame regression case.

**Reference code (`~/Downloads/fluid_gpl`, thesis companion):** mined, nothing portable. It is a 2D OpenCL Stam solver (first-order semi-Lagrangian, Jacobi/GS pressure projection, Steinhoff vorticity confinement) with *no* BFECC/MacCormack implementation — the thesis treats those as review only. We take from it only scheme-ordering confirmation; our BFECC is written fresh (Selle et al. 2008 single-pass form). No code is copied, so no license entanglement.

## Constants plumbing (applies to every wave)

New tunables follow the 4-site house path: canonical value in `crates/core/src/combustion.rs` → re-export in `crates/renderer/src/shader_constants_data.rs` → `#define` emit in `crates/renderer/build.rs` → value pin in `crates/renderer/src/shader_constants.rs::generated_header_contains_all_defines`. A vec3 constant follows the `LUMA_REC709` macro precedent (`#define NAME vec3(a, b, c)`), the only non-scalar precedent in-tree. After shader edits: `glslangValidator -V -I. volumetrics_inject.comp -o …` from `crates/renderer/shaders/` (the `-I.` is required), then `scripts/check-shader-artifacts.sh`. Pin tests in `volumetrics.rs` (~90 exact code strings in `transported_combustion_coupled_physical_contracts` + `combustion_profiles_feed_one_canonical_transported_contract`) are the contract — every intentional shader change updates its pins in the same commit.

## Wave 1 — spectral soot albedo + anisotropic local-light phase (~half day)

- `crates/core/src/combustion.rs`: replace `SOOT_SINGLE_SCATTER_ALBEDO: f32 = 0.18` with `SOOT_SINGLE_SCATTER_ALBEDO_RGB: [f32; 3]`, starting ≈ `[0.22, 0.16, 0.12]` (soot absorbs blue → warm scatter/tint); tune on lab.
- `volumetrics_inject.comp:2010`: `optical.rgb += sootSigmaT * COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO_RGB;`. Note the deliberate side effect: raising σs lowers σa = σt − σs, slightly shifting emission; judge in A/B, compensate via lab tuning if needed.
- Local-light in-scatter (`:2715`): replace `ISOTROPIC_PHASE` with a dual-lobe HG using the existing `medium_henyey_greenstein` (cosθ between `toLightDir` and `view_dir`); new consts `COMBUSTION_LOCAL_LIGHT_PHASE_FORWARD_G` (~0.35), `..._BACKWARD_G` (~−0.12), `..._MIX` (~0.65). Sun path untouched.
- Update the `COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO` pin (volumetrics.rs:2144) to the new name; add pins for the phase consts.
- Gate: crate pin tests + `renderer-eval.sh` A/B on `--combustion-lab` (judge cooled smoke plume tint and firelit smoke).

## Wave 2 — multiple-scattering energy compensation (~1 day)

- `volumetrics_inject.comp` main(): compute `vec3 local_albedo = local_medium.scattering / max(local_medium.extinction, ε)` and `vec3 ms_gain = 1 + W1·local_albedo + W2·local_albedo²` (new consts, defaults ~0.6 / ~0.3). Apply **only to the local-medium share** of scattering: split `scattering_coef` so the global authored fog term is bit-identical and `local_medium.scattering × ms_gain` feeds both the sun term (`:2618–2620`) and the local-light loop (`:2715`). Emission and `authored_inscatter` untouched — emission is direct, authored fog stays authored.
- Explicit verification steps (known couplings): #2809 emissive-fraction temporal keying shifts where scattering rises (check flame flicker isn't over-smoothed); composite beyond-grid aerial-perspective ×4 clamp; re-measure flame-region luminance (memory-budget.md:432 numbers — mean 7.18 / max 188 — become the before/after reference); combustion surface lights are emission-driven so should be unchanged — confirm on lab.
- No new images, no UBO changes.

## Wave 3 — single-pass BFECC advection (the milestone, 2–4 days incl. retune)

Design constraints honored: **no new per-slot images** (protects `FROXEL_VOLUMES_PER_SLOT = 6` and the memory-budget doc pin), reads only `previous*` samplers (preserves the frame-in-flight no-hazard invariant), degrades exactly to current behavior when `dt = 0` (host already zeroes `fog_reference.w` when transport is inactive).

- Inside `transportCombustion` after the existing RK2 backtrace: error term `e = φⁿ(x) − φⁿ(y − v(y)·dt)` with `y = x + (v(x) + wind)·dt` — one extra RK1 backtrace from the forward-projected point, velocity at `y` from the previous dynamics field.
- Corrected channels: chemistry (fuel, temperature, σt, radiance calib) + `optical.rgb`. **Dynamics stays RK2** — velocity correction injects instability and the dye channels carry the visible dissipation.
- Limiter: hard clamp each corrected channel to [min, max] of the 8 texel corners surrounding the source position in previous-grid UVW space (`texelFetch` on the three previous samplers) — the unconditionally-stable BFECC bound.
- Gates: skip entirely unless destination or error sample `carriesCombustion` (empty froxels — most of the grid — pay nothing). `COMBUSTION_BFECC_STRENGTH` const, default 1.0, 0 = exact old path.
- Wall-bleed guard: when `|e|` exceeds a small const, verify the error-trace segment with `combustionPathBlocked`; blocked → `e = 0` (bounds extra TLAS rays to leading edges).
- Same-binary A/B + fallback: `BYRO_BFECC=0` env var (house precedent `BYRO_FIRE_VOLUMES`) read once at boot into a reserved `wind_gust` UBO lane (`.y/.z/.w` documented "reserved"), shader multiplies strength by it.
- Contract maintenance: extend both coupled-contract pin lists with the new BFECC strings.
- Verification: `renderer-eval.sh` frames 1/8/32/64 on `--combustion-lab` — plume persistence across the 32/64 captures is the payoff metric; `render.debug volume` for field inspection; `volumetrics_ms` delta (expect active-froxel-only cost); `scripts/check-render-anchor.sh` ≤ 1.10 p50/p95.
- Follow-on retune (separate commit, same wave): walk `COMBUSTION_TURBULENCE_*` amplitudes and `COMBUSTION_VORTICITY_CONFINEMENT_SPEED` down via pinned-state A/B until perceived detail matches with less procedural forcing; candidate also `FLAME_SOURCE_LATERAL_SPEED`. Target failure mode named in memory-budget.md: the flame breaking into "two or three blocky columns with a squared top" at the default `/8` divisor.

## Wave 4 — durable gate + docs (~half day)

- `byroredux/tests/golden_frames.rs`: add a `--combustion-lab` case on the existing harness (`--bench-frames N --screenshot --bench-mode renderer-static`), thresholds in the existing style anchored to the re-measured flame-region luminance; regenerate via `BYROREDUX_REGEN_GOLDEN=1`. Bin-crate tests run through the rustup 1.96.0 cargo path (AGENTS.md §Tests).
- Docs: refresh the measurement table in `docs/engine/procedural-volumetric-fog.md`; update `docs/engine/memory-budget.md` flame-region numbers and annotate the "/8 degrades" paragraph with the BFECC lever (local high-density combustion volumes remain the architectural end-state, now less urgent).
- ROADMAP/HISTORY refresh at session close per house convention.

## Per-wave loop

`cargo test -p byroredux-renderer` (pins) → recompile `.spv` → `check-shader-artifacts.sh` → `renderer-eval.sh` before/after on `--combustion-lab` → cost gate → next wave; goldens regenerate once at the end.

## Risks

- Pin-test churn is the dominant maintenance cost — budgeted per wave, updated in-commit by design.
- Soot/σs changes couple into σa→emission→moment lights; caught by lab A/B + golden case.
- BFECC can over-sharpen coarse far froxels; the corner-clamp limiter bounds it and the `/8` divisor judgment covers the worst case.
