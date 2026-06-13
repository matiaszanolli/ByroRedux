## Finding REN2-07 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: cross-cutting (camera-relative delta doc-rot cluster)
- **Status**: NEW (all introduced or made stale by `bccf06f0`/`36f66493`). Validated at HEAD `1e8a25ab`: **4 of the 5 originally-reported sites confirmed**; the `draw.rs:1784-1786` site was found STALE (that comment is the #516 cull-SSBO rationale and reads correctly today) and is dropped from this issue.

## Confirmed sites

1. `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:268-272` — render_origin doc names `ssao.comp`/`composite.frag` as CameraUBO declarers. Both are wrong: ssao.comp declares its own `SSAOParams` block, composite.frag a SkyParams-style block; neither declares `CameraUBO` or `renderOrigin`. The actual declarers are `triangle.vert:79` (uses it) and `triangle.frag:243` (size parity). The "streaming resets" claim is also overstated (see REN2-04).
2. `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:38-59` — `gpu_camera_is_336_bytes` assertion message names the wrong reader trio ("triangle.vert, ssao.comp, composite.frag"); the doc-comment in the same block also still says GpuCamera "must stay 288 B" while the test pins 336 B.
3. `byroredux/src/render/camera.rs:96` — references `CameraView::render_origin`, a field that does not exist (the origin is computed locally at `:156`); also carries the false "streaming resets temporal continuity" claim (part of REN2-04's fix).
4. `crates/renderer/shaders/composite.frag:46` — `camera_pos` documented as "world position" with no relative-vs-absolute qualifier (cosmetic; latent trap for future height-fog work — this is the composite sky/fog UBO, not CameraUBO).

## Suggested Fix

One doc pass alongside the REN2-01/REN2-03/REN2-04 fixes (same branch).

## Completeness Checks
- [ ] **SIBLING**: Sweep remaining render-origin commentary for the same stale claims (grep `renderOrigin`/`render_origin` doc comments)
- [ ] **TESTS**: N/A for pure doc fixes, but update the assertion message in `gpu_camera_is_336_bytes` so the test self-documents correctly

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
