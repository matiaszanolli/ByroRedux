# TD7-2026-10-25-04: inline clamp bounds on material-authored values are repeated across shader includes, outside the provenance gate

Labels: low, shaders, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6392
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Age**: lighting.glsl clamp 2026-08-25; ray_hit.glsl 9ade75063 2026-07-30; material_sampling.glsl 218b425be 2026-06-16
- **Location**:
  - Fresnel exponent `clamp(x, 0.25, 16.0)` x3: `shaders/include/pbr.glsl:112,129` and `include/lighting.glsl:155` (the rim-light exponent; also the fallback `2.0` at `:154`).
  - Parallax layer budget: `include/material_sampling.glsl:115-116` `clamp(maxPasses, 4.0, 32.0)` / floor `8.0` (primary) vs `include/ray_hit.glsl:347-348` `clamp(mat.parallaxMaxPasses, 4.0, 12.0)` / floor `4.0` (secondary). CPU default is `DEFAULT_PARALLAX_MAX_PASSES = 4.0` (`core/ecs/components/material.rs:21`) and nothing on the CPU clamps to the shader's range.
  - `presentation.frag:188-193` radial-blur: loop `i <= 6`, `/ 6.0`, `/ 7.0` — three coupled literals for one tap count.
- **What**: The gate only inspects declared SCREAMING constants, so repeated inline clamp bounds slip through; changing the Fresnel range in `pbr.glsl` but not `lighting.glsl` would make rim light and Schlick disagree on the same authored exponent. The primary/secondary parallax caps (32 vs 12) are intentional per the comments but are bare numbers on both sides.
- **Fix**: `FRESNEL_POWER_MIN/MAX`, `PARALLAX_PASSES_MIN/PRIMARY_MAX/SECONDARY_MAX` (+ floors), `PRESENTATION_RADIAL_TAPS` in `shader_constants_data.rs`; optionally have the importer clamp `parallax_max_passes` to the same range so the contract is stated once.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD7-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
