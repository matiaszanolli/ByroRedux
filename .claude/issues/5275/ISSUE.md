# #5275: UI-D5-2026-10-05-01: #4889's "warn once per failure episode" re-arms on every submitted frame, so a persistent staging failure warns, and re-allocates, every frame

Labels: low,ui,bug,renderer,memory
Filed from: docs/audits/AUDIT_UI_2026-10-05.md

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D5-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: Render & Overlay Upload
- **Profile**: both, plus the ground-cover atlas, which shares the arena
- **Location**:
  - `crates/renderer/src/texture_registry/dynamic_rgba.rs:58-68` (`submitted`) and `:87-96` (`note_staging_skip`)
  - `crates/renderer/src/texture_registry/mod.rs:722-727`
  - `crates/renderer/src/vulkan/context/draw.rs:633`
- **Status**: NEW. This is a defect in the 792c56de3 fix for #4889.
- **Description**:
  - `submitted(slot)` sets `staging_skip_logged = false` unconditionally, on the premise that "a completed frame means
    staging works again".
  - `note_frame_submitted` calls `submitted` after every successful `queue_submit`. That includes the very frame whose
    upload was skipped: the skip returns `Ok`, so the frame records and submits as normal.
  - Under a failure that persists (BAR or host-visible exhaustion), frame N warns and sets the flag. Frame N submits,
    which clears the flag. Frame N+1 warns again.
- **Evidence**: control flow: `begin_frame_recording.rs:81-87` gets `Ok` from the degraded path, then
  `draw.rs:633 note_frame_submitted(frame)`, then `dynamic_rgba.rs:67 self.staging_skip_logged = false`. The pin
  `staging_failures_skip_the_overlay_frame_instead_of_failing_it` only counts call sites.
- **Impact**:
  - One `log::warn!` per frame (60 per second), plus one host-visible allocation attempt of about 8.3 MB (1080p) or
    33 MB (4K) per frame, during exactly the memory-pressure episode the flag was meant to keep quiet.
  - Cosmetic otherwise; the frame still renders.
- **Related**: #4889 (closed)
- **Suggested Fix**:
  - Re-arm only after a recording actually uploaded its dirty set, for example in `record_pending_rgba_uploads` once
    every dirty update reached `recorded_slot = Some(frame)`.
  - Add a `DynamicRgbaUploads` unit test for skip → submit → skip that expects one warning.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
