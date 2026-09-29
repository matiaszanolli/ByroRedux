# UI-D7-2026-09-29-03: `launch_hud` leaks the overlay textures that did register when one of its three registrations fails

**Labels**: low,bug,ui,memory

**Source report**: `docs/audits/AUDIT_UI_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: MenuXml
- **Location**: `byroredux/src/hud.rs:575-613` (`match (register(ctx), register(ctx), register(ctx))`, `_ =>` arm at
  `:610`)
- **Status**: NEW (the pattern dates from dc306a6a0, 2026-09-18)
- **Description**:
  - All three swapchain-extent `register_rgba` calls run before any result is checked.
  - If any of them returns `Err`, the `_ =>` arm logs the failure and returns `None`. The handles that succeeded are
    dropped without being released, so they stay in the `TextureRegistry` and in VRAM for the whole session.
- **Impact**:
  - This happens only when an allocation fails at launch, most likely under memory pressure, which the leak then makes
    worse.
  - Up to two images are orphaned: 8.3 MB each at 1080p, 33 MB each at 4K.
  - It happens once, not per frame.
- **Related**: #4892 (collapsing the now-unneeded 3-handle rotation to a single handle removes this path)
- **Suggested Fix**: Register the handles one at a time and release the earlier ones if a later one fails, or register
  a single handle as #4892 proposes.

**Validated at HEAD 9fcfdc3fc**: `launch_hud` in `byroredux/src/hud.rs` still evaluates `match (register(ctx), register(ctx), register(ctx))` and its `_ =>` arm only logs "UI texture registration failed" and returns `None` without releasing the handles that succeeded.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
