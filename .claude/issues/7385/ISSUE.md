# #7385 — REN-D10-2026-11-23-03: The `MAX_LIGHTS` overflow warn is emitted on every frame, with no episode latch

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D10-03). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `scene_buffer/upload.rs` `SceneBuffers::upload_lights` (the `if lights.len() > MAX_LIGHTS { log::warn!(...) }` arm); contrast the `moved_surface_overflow_episode_starts` latch beside it (#6493)
- **Status**: NEW (sibling of #6493 which is still open for the moved-surface warn; `gh ... --search "Light SSBO overflow warn"` finds only closed bundles). Same class as `upload_instances`' `Instance SSBO overflow` warn.
- **Description**: `upload_lights` runs once per frame and warns whenever `lights.len() > MAX_LIGHTS` (1023). A scene held over the cap (heavy modded interior, or many combustion field lights appended after `collect_lights`) logs one `Light SSBO overflow` line per frame. The guard `upload_lights_warns_on_overflow_like_upload_instances` pins that the warn exists and what it says, not how often it fires. The condition is real but also surfaced as `rt.integrity` `lights_dropped` (verdict FAIL), so the per-frame line adds no information after the first.
- **Impact**: log flood (~60 lines/s) in exactly the scenes that already run hot; no correctness impact.
- **Related**: #6493, REN-D1 the instance-overflow warn.
- **Suggested Fix**: reuse the episode latch (`moved_surface_overflow_episode_starts`) for the light and instance warns, and extend the source-scan guard to assert the latch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
