# #3595 — REN-2026-08-30-D8-05: #3426's exact-colour-round-trip argument is premised on an sRGB swapchain that `choose_surface_format` does not guarantee and does not log when it misses

**Labels**: `low,renderer,vulkan,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3595 --json state`.

---

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/swapchain.rs:163-173` (`choose_surface_format`); premise stated at `crates/renderer/src/vulkan/presentation.rs:98-102` and `crates/renderer/src/vulkan/pipeline.rs:943-945`
- **Status**: OPEN — observation, no live impact on any supported device
- **Description**: #3426 added an explicit correctness argument for the overlay's
  colour handling: *"Ruffle's capture is sRGB-encoded bytes uploaded as
  `R8G8B8A8_SRGB`, so the sampler linearises it; Vulkan blends in linear space
  against the sRGB swapchain attachment and re-encodes on write."* Every step of
  that chain verifies — Ruffle's `TextureTarget` is `Rgba8Unorm` and Flash colours
  are authored in gamma space (`render/wgpu/src/target.rs:201` and the comment at
  :66-70), `capture_frame` un-premultiplies to straight alpha
  (`render/wgpu/src/utils.rs:174`, matching the pipeline's
  `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`), and `Texture::from_rgba` uploads
  `R8G8B8A8_SRGB` (`vulkan/texture.rs:88`). The one link the argument asserts
  rather than establishes is the last: `choose_surface_format` prefers
  `B8G8R8A8_SRGB` + `SRGB_NONLINEAR` but falls back to `formats[0]` with no
  warning, and the presentation render pass takes whatever
  `swapchain_state.format.format` gives it. On a surface without that pair the
  hardware sRGB encode disappears and the overlay's read-linearise/write-encode
  round trip stops cancelling.
- **Evidence**: `swapchain.rs:163-173` — `.find(|f| f.format == B8G8R8A8_SRGB && f.color_space == SRGB_NONLINEAR).unwrap_or(formats[0])`, no `log::warn!` on the fallback arm; `presentation.rs:255-263` — the colour attachment is built from that format.
- **Impact**: None observed. `B8G8R8A8_SRGB` + `SRGB_NONLINEAR` is present on
  every desktop driver the project targets, and if it were ever missing the whole
  frame (not just the overlay) would be mis-encoded, so the overlay is not the
  first thing that would break. Filed because #3426 turned an implicit assumption
  into a documented invariant without adding anything that enforces or reports it.
- **Suggested Fix**: One-line `log::warn!` on the `unwrap_or(formats[0])` arm
  naming the chosen format and the colour-space consequence. A hard failure would
  be over-reach; a silent fallback under a documented invariant is the gap.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D8-05

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
