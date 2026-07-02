# CONC-D3-02: animation_system access declaration omits three color-sink component writes

_Filed as #1785 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: LOW · **Dimension**: ECS Lock Ordering / Scheduler Access Declarations · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D3-02)

## Location
`byroredux/src/main.rs:783-813` (declaration) vs. `byroredux/src/systems/animation.rs:150-172` (writes).

## Description
`apply_color_channels` lazily takes `world.query_mut::<AnimatedAmbientColor>()` (animation.rs:154), `AnimatedSpecularColor` (:156-162), and `AnimatedShaderColor` (:170-172) for `ColorTarget::Ambient/Specular/ShaderColor` channels. The `add_to_with_access` declaration at main.rs:791-812 declares `AnimatedDiffuseColor` and `AnimatedEmissiveColor` writes (main.rs:803-804) but none of the other three, despite the comment "The declaration is the UNION across all paths."

## Evidence
`grep AnimatedAmbientColor\|AnimatedSpecularColor\|AnimatedShaderColor byroredux/src/main.rs` → zero hits; `animation.rs:153-155` `write_lazy!(ambient_q, AnimatedAmbientColor, …)` expands to `world.query_mut::<AnimatedAmbientColor>()`.

## Impact
The scheduler's conflict analyzer (and the #1394/#1602 startup `debug_assert_eq!` guards) trust declarations. A future Update-stage parallel system touching any of the three storages would be co-scheduled with animation as "no conflict," opening a genuine cross-thread write-write / ABBA window none of the startup asserts can see. Latent today — animation is the only parallel system in `Stage::Update`.

## Related
CONC-D4-01 (sibling declaration gap).

## Suggested Fix
Add `.writes::<AnimatedAmbientColor>() .writes::<AnimatedSpecularColor>() .writes::<AnimatedShaderColor>()` to the animation declaration in main.rs.

## Completeness Checks
- [ ] **LOCK_ORDER**: The completed declaration is the true UNION across every `apply_color_channels` path
- [ ] **SIBLING**: Cross-check the sibling declaration gap CONC-D4-01 (`physics_sync_system`) in the same pass
- [ ] **TESTS**: A declaration-vs-acquisition check (or `sys.accesses` assertion) pins the animation write surface
