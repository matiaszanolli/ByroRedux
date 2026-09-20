# REN-D5-2026-09-20-02: write_rgba_inplace's hazard contract claims an extent assertion that does not exist — only caller-supplied w/h vs pixels is checked, never the texture's creation extent; unchecked u32 size math

- **ID**: REN-D5-2026-09-20-02
- **Labels**: medium,renderer,memory,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: MEDIUM · **Dimension**: Memory/Lifecycle
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D5-2026-09-20-02)

**Location**: `crates/renderer/src/vulkan/texture.rs` — `overwrite_rgba_pixels` / `write_rgba_inplace` (introduced by `dc306a6a0`)

**Description**
The doc delegates the in-flight hazard to the HUD's 3-slot rotation but also documents an extent assert; `Texture` stores no extent, so nothing checks the upload against the image's creation extent. A second consumer relying on the documented assert would upload into a smaller image — a GPU-side failure mode invisible to cargo test. Independently surfaced by audit D4 and D5; merged.

**Evidence**
Read of the fn: the only check is pixels.len() vs the caller-supplied width/height product; the creation extent is not retained on `Texture`.

**Impact**
Latent today (single consumer is the correctly-rotated HUD path); silent corruption for any future caller.

**Suggested Fix**
Store the creation extent on `Texture` (or thread it through) and assert the upload against it; use checked size math.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
