# #3431: UI-D6-2026-08-27-06: the Ruffle wgpu device, its `TextureTarget` and the UI texture ring are entirely absent from `docs/engine/memory-budget.md`

- **Severity**: LOW
- **Dimension**: Render Path & Device Lifecycle (doc gap)
- **Profile**: both
- **Location**: `docs/engine/memory-budget.md` (no occurrence of `ruffle`, `wgpu`, `scaleform` or `ui`) · `crates/ui/src/player.rs:93-109`, `:284-289`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D6-2026-08-27-06)

## Description

`/audit-ui` Dim 6 asks explicitly for the second GPU device to be quantified "against `docs/engine/memory-budget.md`". The doc has no row for it. Measured against the VRAM baseline (budget total under ~4 GB) the omission is small but not nil, and it is process-lifetime: `shared_descriptors()` stores the `Arc<Descriptors>` in a `static OnceLock`, which is never dropped, so the second `VkInstance`/`VkDevice`/`VkQueue` and every Ruffle pipeline object are released by the OS at exit rather than by wgpu.

## Evidence

Per `SwfPlayer`, `TextureTarget::new` (`ruffle_render_wgpu`) allocates one `Rgba8Unorm` render texture plus one `MAP_READ` readback buffer at `padded_bytes_per_row x height`. At 1920x1080 that is 8.29 MB + 8.29 MB. Add `SwfPlayer::pixel_buffer` (8.29 MB host), the engine-side UI `VkImage`, and up to `MAX_FRAMES_IN_FLIGHT` (2) deferred-destroy copies of it (UI-D6-2026-08-27-04) — approximately 25-42 MB plus one whole extra logical device.

The `shared_descriptors` doc comment honestly records the trade ("one idle logical device is retained after the last menu's player is dropped") but understates it: the `OnceLock` never releases it at all.

A grep of `docs/engine/memory-budget.md` for `ruffle|wgpu|scaleform` returns zero hits.

## Impact

Doc gap only. Nothing here is a leak that compounds.

## Related

#2733 (created the singleton), sibling finding UI-D6-2026-08-27-04.

## Suggested Fix

One `### Scaleform UI` section in `docs/engine/memory-budget.md` with the three numbers above and the process-lifetime note.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other subsystems missing from the budget doc)
