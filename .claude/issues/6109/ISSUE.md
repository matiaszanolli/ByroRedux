# REN-SD-2026-10-19-03: Particle emitter greyscale-LUT texture refs are never released on unload

Labels: low,renderer,memory,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6109

**Severity**: LOW (the refcount leaks, bounded by the number of distinct LUT paths; no per-frame growth) · **Dimension**: Memory/Lifecycle

## Location
- `byroredux/src/cell_loader/spawn.rs` emitter spawn (`greyscale_lut = em.greyscale_lut_map ... resolve_texture(...)`)
- `byroredux/src/scene/nif_loader.rs` emitter spawn (`greyscale_lut = emitter.greyscale_lut_map ... resolve_texture(...)`)
- `byroredux/src/cell_loader/unload.rs` `collect_victim_gpu_handles` (no `ParticleEmitter` read)
- `crates/core/src/ecs/components/particle.rs` `ParticleEmitter::greyscale_lut_index`

## Description
Both spawn paths call `resolve_texture` for the emitter's authored greyscale palette, which is `acquire_by_path` or an enqueue and holds one ref per call. The returned handle is written into the plain `u32` `ParticleEmitter.greyscale_lut_index` through `apply_emitter_overlays`. `collect_victim_gpu_handles` does not read `ParticleEmitter`, so unload never decrements. Additionally the ref is acquired before the two early exits that spawn no emitter at all (the fog-volume replacement `medium_from_particle` and the unresolvable-sprite skip), so there the LUT ref is orphaned the moment it is taken.

## Evidence
`unload_greyscale_lut_tests.rs` pins only the `MaterialTextureHandles` role; `grep -n ParticleEmitter byroredux/src/cell_loader/unload.rs` is empty (verified at publish time).

## Impact
A LUT texture (small, palette-sized) per distinct path stays resident with a growing refcount for the session; the FNV ownership soak cannot see it because FNV particles author no BGEM LUT. Skyrim and FO4 effect shaders do.

## Related
#1341 (same leak fixed for `MaterialTextureHandles`; not a regression of it), #1656, #3590 / #4044 (LUT introduction).

## Suggested Fix
Either resolve the LUT only after the emitter is certain to spawn and store it in a swept handle component, or have `collect_victim_gpu_handles` read `ParticleEmitter::greyscale_lut_index` and pin it with a second case in `unload_greyscale_lut_tests`.

Source: docs/audits/AUDIT_RENDERER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
