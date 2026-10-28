# TD2-2026-10-27-01: SceneBuffers per-frame buffer families are spelled at six sites with no coverage check; #6250 missed the destroy site

Labels: low, renderer, tech-debt, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD2-2026-10-27-01: SceneBuffers per-frame buffer families are spelled at six sites with no coverage check; #6250 missed the destroy site

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD2-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (fields ~:39-237, bundle struct ~:486-516, per-family alloc loop ~:575/:663, bundle literal ~:735, move into `Self` ~:1059-1084); `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`destroy`, ~:361-426).

## Description
Every `Vec<GpuBuffer>` family must be written at six places. `destroy` tears them down with eleven hand-unrolled `for buf in &mut self.X { buf.destroy(..) } self.X.clear();` loops. #6250 added `moved_surface_buffers` at five of the six sites (fields :237, bundle :503, alloc :575/:663, literal :735, move :1084) and missed `destroy`. That miss is SAFE-2026-10-27-01's allocator-`Arc` leak, which fails the teardown `unwrap` and has kept the lavapipe lane red since c7463c04d. Nothing checks that each field reaches `destroy`.

## Evidence
`grep moved_surface descriptors.rs` returns nothing, while `buffers.rs` has 6 hits; the `destroy` loops in `descriptors.rs` list light/camera/bone/instance/previous_model/material/dalc/indirect/selected_ray_probe families only.

## Impact
Any future per-frame buffer family can silently skip teardown (allocator leak, red lavapipe lane). Structural root of SAFE-2026-10-27-01.

## Related
SAFE-2026-10-27-01 (fix together), open #6371 (compute-pass teardown, same hand-written-teardown class).

## Suggested Fix
Fix with SAFE-01. Either give `SceneBuffers` a `fn per_frame_families_mut(&mut self) -> [&mut Vec<GpuBuffer>; N]` that `destroy` iterates (a new family becomes a compile-visible edit), or add a source-scan test that every `Vec<GpuBuffer>` field named in `buffers.rs` appears in `destroy`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

