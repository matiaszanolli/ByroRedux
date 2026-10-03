# #5206 — REN-D5-2026-10-03-02: switching from a retained image cover to a model cover overwrites `Artwork::Image` without `drop_texture`

**Labels**: low,renderer,memory,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `LoadingScreen::spawn_model_stage` (`byroredux/src/loading_screen.rs`), the `self.artwork = Some(Artwork::Stage(..))` assignment.
- **Status**: NEW.
- **Description**:
  - `Artwork::Image` is deliberately retained across transitions ("reused on repeated transitions … avoids allocating a new non-reusable bindless slot at every door").
  - `present_image_artwork` releases the previous image (`drop_texture(old)`) when it replaces it.
  - `spawn_model_stage` only drains `retired_stage`, then assigns `self.artwork = Some(Artwork::Stage(...))`. If the previous artwork was a retained `Image`, its texture handle is dropped on the floor. Its refcount never reaches zero, so the registry never redirects or frees the slot.
- **Evidence**: The only release in `spawn_model_stage` before the assignment is `if let Some(stage) = self.retired_stage.take() { retire_stage(...) }`. Nothing inspects the existing `self.artwork`.
- **Impact**: One original-artwork texture and its bindless slot leak, resident until shutdown, per image→model switch. Bounded and rare: it needs a load order mixing image-only and model LSCRs, for example FO4 or Skyrim with a mod adding image screens.
- **Related**: REN-D5-2026-10-03-01 (same file).
- **Suggested Fix**: Before assigning the stage, `if let Some(Artwork::Image { texture, .. }) = self.artwork.take() { ctx.texture_registry.drop_texture(&ctx.device, texture) }`, mirroring `present_image_artwork`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
