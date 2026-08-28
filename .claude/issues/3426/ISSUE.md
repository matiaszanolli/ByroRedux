# #3426: UI-D6-2026-08-27-01: the Scaleform overlay is blended into the render-resolution HDR G-buffer, so every menu is fogged, bloomed, exposure-scaled, ACES tone-mapped, TAA-accumulated and FSR-upscaled

- **Severity**: HIGH
- **Dimension**: Render Path & Device Lifecycle
- **Profile**: both (Skyrim AVM1 + Fallout 4 AVM2)
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:930-956` · `crates/renderer/src/vulkan/context/geometry_pass.rs:606-620` · `crates/renderer/src/vulkan/context/helpers.rs:148-190` · `crates/renderer/shaders/presentation.frag:160`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D6-2026-08-27-01)

## Description

The UI quad is drawn at the tail of the *main geometry render pass*, alpha-blended into **color attachment 0 — the HDR direct-lighting G-buffer target**, at render (not output) resolution. Every post-process stage downstream therefore operates on the menu as if it were world radiance: height-fog / volumetric transmittance (keyed off the world depth still present under the UI quad, since the UI pipeline sets `depth_write_enable(false)`), underwater god rays, the M58 bloom add, TAA temporal accumulation with a **zero** motion vector and **no** FSR reactive or transparency mask, FSR upscaling, and finally `aces(graded * params.exposure)` in `presentation.frag`.

## Evidence

The blend state is unambiguous:

```rust
// crates/renderer/src/vulkan/pipeline.rs:930
let ui_hdr_blend = vk::PipelineColorBlendAttachmentState::default()
    .color_write_mask(vk::ColorComponentFlags::RGBA)
    .blend_enable(true)
    .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
```

and the code already names the fix it does not implement:

```rust
// crates/renderer/src/vulkan/pipeline.rs:949-952
// The Scaleform overlay writes no FSR mask. Marking it reactive
// would only paper over the real fix, which is moving the overlay
// out of the render-resolution pass entirely so it is composited
// after upscale and never enters temporal reconstruction.
```

The tone-map is unconditional on the whole upscaled image: `vec3 presented = aces(graded * params.exposure);` (`presentation.frag:160`). The Narkowicz ACES fit maps linear `1.0` to `2.54/3.16 ~= 0.80`, so — before any exposure scaling — **pure white menu text and white UI chrome reach the swapchain at ~80 % grey**, and the 0.18 mid-grey a font renderer emits lands at ~0.267.

The upload compounds this: `TextureRegistry::update_rgba` -> `Texture::from_rgba` (`crates/renderer/src/vulkan/texture.rs:88`) declares `vk::Format::R8G8B8A8_SRGB`, so Ruffle's sRGB-encoded `Rgba8Unorm` capture is linearised by the sampler on the way in and then re-encoded by a curve (ACES) that is not the sRGB OETF.

## Impact

Every Bethesda menu this layer can load renders with wrong luminance and saturation, ghosts under TAA when it animates (no motion vector, no reactive mask), blurs under FSR reconstruction, and — in an exterior cell with fog — is attenuated by the fog term belonging to the world geometry behind it. This is not a cosmetic nicety: it is the concrete blocker under the "font fidelity" row `docs/engine/ui.md` lists as Pending, and it makes any future menu-fidelity comparison against vanilla meaningless until it moves. It also means the `--menu` smoke gate can only ever prove "a menu loaded", never "a menu looks right".

## Related

None filed — a full-text search of the issue list and of `docs/audits/` finds no issue or prior finding for the overlay's compositing stage. `docs/engine/ui.md`:286 ("The UI is drawn at the tail of the main render pass") states the fact but none of its consequences. Cross-audit: the *pass ordering* belongs to `/audit-renderer`, but the defect is UI-only, so it is reported once here.

## Suggested Fix

Move the UI quad out of the geometry pass into its own post-`presentation.frag` composite (native output resolution, no tone-map, no temporal history), and upload the Ruffle capture as `R8G8B8A8_UNORM` once it is no longer being linearised for an HDR target. This is the fix `pipeline.rs:949-952` already describes.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other overlay/fullscreen passes, other blend-state tables)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
