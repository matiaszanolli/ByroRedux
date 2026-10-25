# TD7-2026-10-25-02: combustion temperature clamps/floors live as shader-local literals; the exemption's "no Rust counterpart" is false for the 4200 K ceiling

Labels: low, shaders, renderer, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6390
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Age**: b3ee51921 2026-08-18 (ceiling); regimes in core unchanged since
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:421` `const float MAX_COMBUSTION_TEMPERATURE_K = 4200.0;` (exempted at `shader_constants.rs:225`), `:1419` inline `nuclearExplosion ? 3800.0 : 2300.0`, `:420` `IGNITION_TEMPERATURE_K = 720.0`, `:422` `MAX_COMBUSTION_SPEED_MPS = 28.0`; core side `crates/core/src/combustion.rs:282-283` `CombustionRegime::NUCLEAR_EXPLOSION { temperature_k: 4200.0, .. }` (others: FLAME 1850, EXPLOSION 2800, GAS_FLAME 3400, ADIABATIC 2350).
- **What**: `sanitizedChemistry` (`:1730-1737`) and the source-temperature clamp (`:1417-1421`) clamp transported chemistry to `[AMBIENT, 4200]`. 4200 is exactly the nuclear regime's authoring temperature in core, so raising any regime above 4200 K (e.g. a plasma/magic regime — the `GAS_FLAME` doc anticipates one) is silently clamped by the shader with no test or constant tying the two. Everything else in the combustion model flows core -> `shader_constants_data.rs` (`ADIABATIC_FLAME_TEMPERATURE_K`, `COMBUSTION_SOOT_OXIDATION_*`, `COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2`, ...); these four did not. The 3800/2300 K floors match no core regime (core: 4200 / 1850-2800), i.e. they are already a divergent duplicate.
- **Fix**: add `COMBUSTION_MAX_TEMPERATURE_K = CombustionRegime::NUCLEAR_EXPLOSION.temperature_k()` (const fn) and the floors to `shader_constants_data.rs`, drop the `MAX_COMBUSTION_TEMPERATURE_K` exemption, and add a core test that no `CombustionRegime` exceeds the ceiling.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD7-2026-10-25-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
