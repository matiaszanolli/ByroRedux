# REN-SD-2026-10-19-04: ObScript `SwapTexture` acquires one texture ref for N holders and leaks every displaced original

Labels: medium,renderer,memory,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6110

**Severity**: MEDIUM (premature free of a shared texture, or a ref-underflow warning, on unload; plus leaked originals) · **Dimension**: Memory/Lifecycle

## Location
`byroredux/src/asset_provider/texture.rs` `apply_texture_swap` (`let handle = resolve_texture(ctx, tex_provider, Some(&path));` then `handles.insert(mesh, TextureHandle(handle))` for each matched mesh); driver `byroredux/src/app_step.rs` `step_texture_swaps`.

## Description
One `resolve_texture` takes one ref. The handle is then assigned to every mesh under the target whose base texture path contains the slot name. Unload releases one ref per `TextureHandle` holder, so M matched meshes release M refs against one acquired: net -(M-1). The meshes' previous `TextureHandle` values were acquired at spawn and are overwritten without a `drop_texture`, so each original leaks one ref; swapping the same slot twice leaks the earlier swap's ref too.

When M >= 2 and the texture is shared with other resident meshes, the extra releases reach zero early and the other holders' bindless slot is redirected to the checkerboard. When it is not shared, the surplus decrement logs `drop_texture(..) on already-released handle`.

## Evidence
`apply_texture_swap` body (resolve once before the loop, loop only inserts); `resolve_texture_view_with_clamp` doc: "each resolve pairs with one drop_texture on cell unload ... exactly one outstanding ref per caller". The loop never reads the previous `TextureHandle`.

## Impact
Wrong textures on unrelated meshes after the swap target's cell unloads, and the originals pinned. The command is rare (endgame slides), so blast radius is small.

## Related
#524 (refcount model), #4427 (flipbook frames, the same ledger done correctly), REN-SD-2026-10-19-05.

## Suggested Fix
Acquire once per matched mesh (loop `resolve_texture`, or bump the registry ref by M-1) and `drop_texture` the displaced handle before overwriting it.

Source: docs/audits/AUDIT_RENDERER_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
