# Investigation: RT-9 / #672 — radius=0 light disk floor

## Audit premise (re-verified)

Audit cites `triangle.frag:1425` for `lightDiskRadius = max(radius * 0.025, 1.5)`.
Actual line is **1549** (line numbers shifted post-audit). Logic confirmed.

The 1.5u floor is dead for any Bethesda-authored radius ≥ 60u (since
`60 * 0.025 = 1.5`). XCLL radii are 256–4096 → `radius * 0.025` always
dominates → 1.5u floor is a defensive net for `radius == 0`.

## Where radius=0 can leak in

`crates/plugin/src/esm/cell/support.rs:67-72` reads LIGH `DATA` bytes 4..8
as `u32 → f32` raw. A malformed/dev LIGH record (or one with the radius
field intentionally zeroed) propagates `radius=0` into `LightData.radius`.

`byroredux/src/cell_loader.rs` constructs `LightSource` from
`LightData.radius` at **4** sites:

| Line | Path | Behaviour on radius=0 |
| ---- | ---- | --------------------- |
| 1656 | LIGH-only entity (no mesh) | copies `ld.radius` raw → leaks |
| 1702 | fxlight effect mesh + ESM LIGH | copies `ld.radius` raw → leaks |
| 2540 | NIF-imported light | already has `radius > 0.0 ? r : 4096.0` fallback BUT only for `light.radius`; `esm_radius == Some(0.0)` also leaks |
| 2940 | per-mesh ESM fallback | copies `ld.radius` raw → leaks |

## Shader consequence

A radius=0 light arriving at the lights buffer:
- `effectiveRange = radius * 4.0 = 0` → `max(effectiveRange, 1.0) = 1.0`
- `ratio = dist / 1.0` → for any dist > 1u, `window = clamp(1 - ratio², 0, 1) = 0`
- `atten = 0` → `contribution < 0.001` at line 1462 → light skipped before
  it ever reaches the reservoir / shadow-ray phase.

So the audit's penumbra-collapse description is technically reachable only
if dist ≤ 1u (light at the shaded fragment), which is a degenerate case.
The bigger functional bug is that radius=0 lights are **completely
invisible** — they contribute zero atten, zero light, no shadows.

## Fix

Importer-side clamp. Add `light_radius_or_default(radius) -> f32` in
`cell_loader.rs`, apply at all 4 LightSource construction sites, also
patch the `esm_radius == Some(0.0)` hole at the NIF-direct site. Default
fallback = 4096.0u (mirrors the existing NIF ambient/directional default
already at line 2537). Shader floor stays at 1.5u — becomes truly
unreachable but kept as a defensive net (no behavioral change for
positive-radius lights).

## Scope

- 1 source file (`byroredux/src/cell_loader.rs`)
- 1 test file (extend `cell_loader_nif_light_spawn_gate_tests.rs`)
- No shader change, no SPIR-V recompile.
