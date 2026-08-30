# #3570 — REN-2026-08-30-D10-01: the depth-capture readback hardcodes `D32_SFLOAT` while `find_depth_format` can select `D16_UNORM`, and `VulkanContext::depth_format` is never consulted

**Labels**: `medium,renderer,vulkan,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3570 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`depth_capture_record_copy` L135-136, `depth_capture_finish_readback` L49/L94-98); `crates/renderer/src/vulkan/context/helpers.rs` (`find_depth_format` L26); `crates/renderer/src/vulkan/context/mod.rs` (`depth_format` field, L1875)
- **Status**: New
- **Description**: The capture path assumes 4 bytes per depth sample in both
  halves — `buffer_size = width * height * 4 /* D32_SFLOAT */` when sizing the
  staging buffer, and `slice.chunks_exact(4).map(f32::from_le_bytes)` when
  decoding it. `find_depth_format` is a *fallback chain*:
  `let candidates = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM];` — it
  returns whichever the physical device reports first with
  `DEPTH_STENCIL_ATTACHMENT` optimal-tiling support. Vulkan mandates
  `D16_UNORM` support for depth attachments but does **not** mandate
  `D32_SFLOAT` (only that one of `D32_SFLOAT` / `X8_D24_UNORM_PACK32` be
  supported), so the D16 arm is genuinely reachable. The selected format is
  already stored on the context as `self.depth_format` and `depth_capture.rs`
  never reads it.
- **Evidence**:
  - `helpers.rs:26` — `let candidates = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM];`
  - `depth_capture.rs:135-136` — `extent.width as vk::DeviceSize * extent.height as vk::DeviceSize * 4 /* D32_SFLOAT */`
  - `depth_capture.rs:49` — `let expected = width as usize * height as usize * 4;`
  - `depth_capture.rs:94-98` — `// D32_SFLOAT: one f32 per sample …` then `.chunks_exact(4).map(|b| f32::from_le_bytes(...))`
  - `mod.rs:1875` — `depth_format: vk::Format,` (present, unused by this module)
  - No `aspect`/stencil hazard: both candidates are depth-only, so
    `ImageAspectFlags::DEPTH` and the absence of stencil-interleaving handling
    are correct as written — the format *width* is the only wrong assumption.
- **Impact**: On a device that falls back to `D16_UNORM`, the staging buffer is
  allocated at 2× the needed size (harmless) but the readback reinterprets
  pairs of adjacent unorm16 samples as one f32, at half the sample count and
  at the wrong pixel positions. `analyze_depth_field` would then report
  `distinct_codes` / band occupancy that is pure noise, with only a partial
  tell (some garbage bit patterns decode outside `[0,1]` and land in
  `stats.invalid`). Because the whole point of this code is to supply the
  before/after evidence for the #3308 reversed-Z architectural decision, a
  silently-wrong capture is worse than no capture. Zero impact on the dev
  RTX 4070 Ti, which selects `D32_SFLOAT`.
- **Suggested Fix**: Read `self.depth_format` in both halves. Either (a) gate
  the capture with an early `log::warn!` + `return` when the format is not
  `D32_SFLOAT`, so the tool refuses rather than lies, or (b) carry the format
  through `depth_capture_pending_readback` alongside the extent and decode
  `D16_UNORM` as `u16 as f32 / 65535.0`. Option (a) is the smaller change and
  matches the module's existing "diagnostic, single consumer" posture; either
  way the `/* D32_SFLOAT */` comments become an assertion instead of an
  assumption.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D10-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
