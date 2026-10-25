# TD7-2026-10-25-03: the water rain-control bit-pack is a bare-literal CPU/GPU contract with no shared constants and no round-trip test

Labels: low, water, shaders, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6391
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Age**: c5612af8d 2026-08-20
- **Location**: pack `byroredux/src/render/water.rs:73-84` (`* 1023.0`, `<< 10`, `<< 20`, `clamp(0.0, 0.999)`); unpack `crates/renderer/shaders/water.frag:929-934` (`& 1023u`, `>> 10u`, `>> 20u`, `/ 1023.0`, then `x / max(1 - x, 0.001)`).
- **What**: Three 10-bit lanes are quantized with a `v/(v+1)` map in the binary crate and decoded in the shader; the lane width, shifts, scale and the 0.999 clamp (which bounds the decode at ~999) exist only as duplicated literals on both sides. The only guard is a source-substring assert (`vulkan/water.rs:1325`, checks `floatBitsToUint(push.normal_falloff.w)` exists). The file that owns the push-constant layout (`vulkan/water.rs`) does not own the encoding.
- **Fix**: move the pack/unpack pair beside `GpuWaterParams`, export `RAIN_CONTROL_BITS/LANES` through `shader_constants_data.rs`, generate the GLSL mask/shift defines, and add a Rust round-trip test (pack -> unpack in Rust mirroring the GLSL math) pinning the 0.999 clamp.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD7-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
