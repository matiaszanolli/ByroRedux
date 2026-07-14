**Source:** FO4 compatibility audit — Dimension 5 (FO4 Shader Flags & PBR Routing), `docs/audits/AUDIT_FO4_2026-07-13.md`
**Severity:** MEDIUM · **Status when filed:** NEW, CONFIRMED against current code (incomplete fix of the *closed* #1592)

## Description
#1592 wired the FO4 `F4SF2` bit 25 (`Alpha_Test`) shader flag to OR `info.alpha_test = true` into `MaterialInfo` for inline FO4 NIFs that signal a cutout via the shader flag alone (no sibling `NiAlphaProperty`). But that branch sets **only** the boolean — it leaves `info.alpha_threshold` at the `MaterialInfo::default()` value of `0.0`. The fragment shader disables the discard when the threshold is `0.0`, so the consumed flag has no visible effect: the cutout does not happen.

## Evidence
- `crates/nif/src/import/material/walker.rs:341-342`:
  ```rust
  if shader.shader_flags_2 & crate::shader_flags::fo4_slsf2::ALPHA_TEST != 0 {
      info.alpha_test = true;
  }
  ```
  — no `info.alpha_threshold` write.
- `crates/nif/src/import/material/mod.rs:952`: `alpha_threshold: 0.0` (the default).
- `crates/renderer/shaders/triangle.frag:178`: `if ((inst.flags & INSTANCE_FLAG_DIFFUSE_ALPHA) == 0u && mat.alphaThreshold == 0.0) { ... }` — with `alphaThreshold == 0.0` the alpha-test discard path is not taken.
- `apply_alpha_flags` (`mod.rs`) is the ONLY writer of a usable threshold, and it runs **only** when a `NiAlphaProperty` is present — precisely the case this flag branch exists to cover the absence of.
- The walker comment ("the GREATEREQUAL/default threshold stands in") is factually wrong — the default `0.0` produces no cutout.
- Test `fo4_alpha_test_flag_sets_field` asserts only `info.alpha_test == true`; it never checks that a usable threshold reaches the shader, so the gap is untested.

## Impact
An FO4 inline NIF that requests a cutout purely via `F4SF2` bit 25 (foliage / fence / grate / chain-link edges with no sibling `NiAlphaProperty`) renders as a solid opaque quad — the cutout silhouette is lost. Blast radius is **limited**: vanilla FO4 cutout meshes almost universally ship a `NiAlphaProperty` (which sets the threshold correctly), so this bites inline/modded FO4 content that relies on the flag alone. This is not an active regression vs the pre-#1592 state (which also produced no cutout) — it is an **incomplete fix**: the flag is now consumed but still has no effect.

## Suggested Fix
When setting `info.alpha_test = true` from the FO4 flag with no `NiAlphaProperty` present, seed a usable default threshold (`info.alpha_threshold = 128.0/255.0` — Bethesda's conventional cutout value) and keep `alpha_test_func = 6` (GREATEREQUAL). Add a threshold assertion to `fo4_alpha_test_flag_sets_field` so the shader-visible effect is pinned.

## Related
Closed #1592 (the fix that introduced the branch), #1733 (doc-fix sibling on the same block).

## Completeness Checks
- [ ] **SIBLING**: confirm no other shader-flag→MaterialInfo branch sets a bool without its backing scalar (e.g. parallax/refraction thresholds)
- [ ] **CANONICAL-BOUNDARY**: the threshold seed stays at the NIFAL parser→MaterialInfo boundary (import), not re-derived in the shader/renderer
- [ ] **TESTS**: extend `fo4_alpha_test_flag_sets_field` to assert a shader-usable `alpha_threshold > 0.0` reaches `Material`
