# NIFAL-D6-2026-08-07-03: mat.set writes canonical PBR scalars with no clamp or finite guard, bypassing the resolve_pbr invariant

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2489
**Finding ID**: NIFAL-D6-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 6 — NIFAL Material
**Location**: `byroredux/src/commands/scene.rs::MatSetCommand::execute` (`set_scalar` arms for `metalness` / `roughness` / `ior` / `alpha`)
**Status**: NEW

## Description
`Material::metalness`/`roughness` carry a documented engine-wide invariant — "fully resolved, clamped to the renderer ranges (`metalness ∈ [0,1]`, `roughness ∈ [0.04,1]`)" — which the render path relies on by reading them verbatim into `GpuMaterial`. `mat.set` is the only writer that reaches these fields after `translate_material`, and it stores the parsed `f32` directly:
```rust
let set_scalar = |slot: &mut f32, vals: &[&str]| -> Result<String, String> {
    let v = MatSetCommand::floats(vals, 1)?;
    *slot = v[0];                      // no clamp, no is_finite check
    Ok(format!("{:.4}", v[0]))
};
```
Rust's `"NaN".parse::<f32>()` / `"inf".parse::<f32>()` both succeed, and `mat.set <id> roughness 0` is a plausible typo that lands below the 0.04 floor.

## Evidence
The sibling write path treats this as load-bearing — `material_translate.rs:310` returns `None` rather than let a non-finite glossiness reach `roughness`, with the rationale "NaN GGX terms poison the lit color and stick in SVGF/TAA history" (#1535). `mat.set` has no equivalent guard, so a NaN typed at the console produces exactly the failure #1535 was filed to prevent — and it persists in the temporal history buffers after the value is corrected. The Cornell harness is built around driving these fields live, so this is a reachable workflow, not a theoretical one.

## Impact
Debug-tooling only (no shipping content path), but the failure is sticky and easy to misattribute to the renderer rather than to the console input. Also affects `mat.set ... ior` for the fire-refraction overload, whose translate-side sibling `material_optical_scalar` *does* sanitize (`clamp(0,1)` + NaN → 0).

## Related
#1535 (the NaN-roughness guard this bypasses); #2249 / REN-D21-03 (added the `ior` arm); #2330 / SKY-D7-03 (the other post-translate writer).

## Suggested Fix
Route the three PBR arms through the same clamps `resolve_pbr` applies (or simply call `m.resolve_pbr()` after the mutation — it is idempotent and already clamp-only for non-NaN input), and reject non-finite input with the existing `Err(String)` path so the console reports it.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: `mat.set` routes through `resolve_pbr` so console writes and `translate_material` writes stay in lockstep
- [ ] **TESTS**: A regression test confirms `mat.set <id> roughness NaN` is rejected rather than silently stored
