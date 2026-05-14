# Issue #1013

**Title**: REN-D18-001: Composite drops volumetric transmittance entirely — only scattering wired (latent under #928)

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D18-001
**Severity**: HIGH (latent — fires the moment `VOLUMETRIC_OUTPUT_CONSUMED` flips)
**File**: `crates/renderer/shaders/composite.frag:412`; `crates/renderer/src/vulkan/volumetrics.rs:7-8` (header doc)

## Premise verified (current `main`)

`volumetrics.rs` header line 8 documents the modulation as `final = scene * vol.a + vol.rgb` (Frostbite §5.3 standard form: scene attenuated by transmittance, then in-scattered radiance added). `composite.frag:358` computes `combined = direct + indirect * albedo + caustic;` and line 412 does `combined += vol.rgb * 0.0;`. Even with the `* 0.0` removed, the line is purely additive — `vol.a` (cumulative transmittance written by `volumetric_integrate.comp:66`) is NEVER read. The integration pass correctly multiplies transmittance across the walk (`trans_cumulative *= exp(-extinction*dt)`).

## Issue

Latent — fires the moment `VOLUMETRIC_OUTPUT_CONSUMED` flips to true (#928 disabled-path gate). When re-enabled, god-ray scattering will add to the scene but the receding-into-fog attenuation that should darken distant geometry will be missing — distant terrain stays at full radiance + glow on top, the inverse of the intended look.

## Fix

Replace `combined += vol.rgb * 0.0;` with `combined = combined * vol.a + vol.rgb;` in lockstep with flipping `VOLUMETRIC_OUTPUT_CONSUMED = true`. Order matters: attenuate first, then add inscatter — inscatter is energy that arrived between camera and fragment and is NOT itself attenuated by `T_cum` (the integrate pass already weighted each slab's contribution by its own running transmittance).

## Test

Composite-output golden-image diff with synthetic high-scattering scene (~σ=0.05); a near-camera bright surface should fade to fog colour across a fixed distance, matching analytic `exp(-σ·d)`.

## Related

- #924 covers the disabled-mix fallback in the aerial-perspective branch at composite.frag:472 — distinct path.
- #928 controls the disable gate; this fix should land BEFORE the gate flip.

## Completeness Checks

- [ ] **UNSAFE**: N/A — GLSL
- [ ] **SIBLING**: Verify `vol.rgb * 0.0` sentinel is removed in lockstep with gate flip; ensure no other site reads vol but ignores .a
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Golden-image regression with σ=0.05 synthetic scene

