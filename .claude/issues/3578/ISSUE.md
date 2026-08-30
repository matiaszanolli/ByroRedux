# #3578 — REN-2026-08-30-D3-03: `PresentationPushConstants` smuggles two `u32` bitfields through `f32` lanes, against the codebase's own `uvec4` idiom

**Labels**: `low,renderer,shaders,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3578 --json state`.

---

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/presentation.rs` (`PresentationPushConstants::render_debug_flags`, `::render_debug_mode`), `crates/renderer/shaders/presentation.frag` (`PresentationParams`)
- **Status**: New (introduced in the `969d81c8..HEAD` delta, with the new presentation pass)
- **Description**: The new presentation pass declares its two debug integers as
  `f32` on both sides of the boundary and round-trips them through the float
  representation: the host writes `f32::from_bits(u32)` and the shader reads them
  back with `floatBitsToUint`. The bit patterns involved are overwhelmingly
  **denormal** floats. This works today, but it makes a GPU struct's correctness
  depend on float-representation preservation for data that has no reason to be
  typed as float, and it inverts the idiom the rest of the renderer uses.
- **Evidence**: `presentation.rs:22-34` declares `render_debug_flags: f32` /
  `render_debug_mode: f32`; `presentation.rs:542-543` populates them:
  ```rust
  render_debug_flags: f32::from_bits(input.render_debug_flags),
  render_debug_mode:  f32::from_bits(input.render_debug_mode),
  ```
  `presentation.frag:27-28` mirrors them as `float renderDebugFlags; float renderDebugMode;`
  and `:135-136` recovers them:
  ```glsl
  uint dbgFlags  = floatBitsToUint(params.renderDebugFlags);
  uint debugMode = floatBitsToUint(params.renderDebugMode);
  ```
  Smallest normal `f32` is bit pattern `0x00800000`, so **every** `DBG_*` mask whose
  highest set bit is ≤ 22 is a denormal — that is `DBG_BYPASS_POM` (`0x1`),
  `DBG_VIZ_NORMALS` (`0x4`), … through `DBG_VIZ_FSR_TEMPORAL` (`0x400000`), i.e. most
  of the commonly-used views. Likewise `render_debug_mode` is a small enum
  (`RENDER_DEBUG_FINAL = 0` … `RENDER_DEBUG_MODE_MAX`), so **every non-zero debug mode
  is a denormal**. Masks that set bits 23-30 together reach the `Inf`/`NaN` exponent
  band.

  The rest of the engine does the opposite and does it correctly:
  `GpuCamera.render_debug` is `[u32; 4]` / `uvec4 renderDebug`, and where a *float*
  needs to ride in it, `triangle.frag:791` bitcasts **out of** the uint lane
  (`uintBitsToFloat(renderDebug.y)`) — a uint lane never subjects the payload to
  float interpretation. `presentation.rs` is the only site that runs the cast in the
  fragile direction.

  Honest scope: no driver-observed failure is confirmed. Vulkan denorm flush-to-zero
  (`VK_KHR_shader_float_controls`) is specified for floating-point *operations*, and
  neither the push-constant load nor `OpBitcast` is one, so current behaviour is
  expected to be correct. The finding is that the struct relies on that reasoning
  for no benefit.
- **Impact**: Latent robustness risk on any implementation that canonicalises
  denormal or NaN payloads across a float-typed push-constant load, which would zero
  or corrupt the debug mask. Debug-path only. The larger cost is consistency: a
  reader of `PresentationPushConstants` sees two float fields whose values are
  meaningless as floats, and the mismatch with the `uvec4 renderDebug` idiom two
  files away invites the wrong fix.
- **Suggested Fix**: Type both fields `u32` in `PresentationPushConstants` and `uint`
  in `presentation.frag`'s `PresentationParams`, assigning `input.render_debug_flags`
  / `input.render_debug_mode` directly and dropping both `floatBitsToUint` calls.
  Both fields sit in the same 16-byte block as `exposure` and the explicit
  `padding: f32`, so the struct stays exactly 128 B and
  `presentation_push_constants_match_shader_alignment` (which asserts size 128 and
  the `exposure`/`lens`/`fade_color` offsets) continues to pass unchanged.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D3-03

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
