# D2-NEW-02: Per-particle unquantized LERPed color defeats MaterialTable dedup — one fresh GpuMaterial per live particle per frame

**Issue**: #1795
**Labels**: medium,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-02)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-02)

## Location
`byroredux/src/render/particles.rs:74-82,138-139,208-209`; `crates/renderer/src/vulkan/context/mod.rs:509,521-523` (`material_hash` over raw `emissive_*` bits)

## Description
Each particle's color is LERPed against `t = age/life` (unquantized, `particles.rs:74`) and folded into `emissive_color`/`emissive_mult` (`:138-139`), both material-table fields post-R1. `material_hash` hashes `emissive_mult.to_bits()` + `emissive_color[..].to_bits()` — raw f32 bits, zero quantization (`context/mod.rs:521-523`) — so `intern_by_hash` (`:208`) takes the miss path per particle every frame: full `GpuMaterial` build + FxHashMap insert + table push + upload, inverting the ~97% dedup-hit rate the #781 fast path assumes. Instancing is unaffected (`material_id` is per-instance); this is the residual after #1649 fixed the depth-vs-mesh sort ordering.

## Evidence
`particles.rs:74` continuous `t`; `:138-139` emissive writes; `:208` intern; `context/mod.rs:521-523` raw-bits hash covers the emissive fields, so distinct colors never dedup.

## Impact
Scales with live particle count. FX-heavy scenes (20-30 emitters, 96-256 particle caps each) can reach ~5-8K unique materials/frame ≈ 1.5-2.3 MB/frame upload plus CPU churn, stacking toward the `MAX_MATERIALS = 16384` cap where overflow silently routes particles to neutral material id 0 (wrong color). Also permanently depresses dedup-ratio telemetry, masking real dedup regressions elsewhere.

## Related
#1649, #781, #780, #797.

## Suggested Fix
Quantize the fade parameter before the color LERP (e.g. 32 steps — imperceptible on additive billboards). Same-emitter particles then collapse to ≤32 materials.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix

