# #2797: REN-D7-2026-08-12-02: ui.vert slot-0 comment argues pre-#807 premise, contradicts gpu_types.rs corrected contract

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
The UI slot-0 comment argues from the pre-#807 premise ("`materials[0]` is the FIRST scene material"), contradicting `gpu_types.rs`'s corrected contract — and slot-0 semantics is what the over-cap fallback rests on. Cites `scene_buffer.rs:172-176`; `scene_buffer` has been a **directory** since Session 34/35. `material.rs:281-294` is likewise rotted (`as_bytes` is near 594).

## Location
`crates/renderer/shaders/ui.vert`, `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`, `crates/renderer/src/vulkan/scene_buffer/descriptors.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D7-2026-08-12-02).

---

# #2798: REN-D8-NEW-02: record_ssao_pass doc claims current-frame AO with no lag, actual AO sample is two frames old

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
Doc says AO is "current-frame (no lag)" because SSAO runs before composite — but `composite.frag` has no AO binding at all; the sole reader is `triangle.frag` in the **main render pass**, which runs earlier. With per-FIF AO images the sampled AO is **two frames old**, not zero and not one. `triangle.frag`'s own "computed last frame" is closer but still off by one slot.

## Location
`crates/renderer/src/vulkan/context/post_passes.rs` (`record_ssao_pass` doc)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D8-NEW-02).

---

# #2799: REN-D8-NEW-03: composite_dep_in comment calls scene_image_views the swapchain image, contradicts module docstring

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
The `composite_dep_in` comment calls attachment 0 "the swapchain image"; it is `scene_image_views[i]`, an offscreen `HDR_FORMAT` image. The dependency's *reasoning* is still correct — only the noun is stale — and the module docstring at the top of the same file is already right, so the file contradicts itself.

## Location
`crates/renderer/src/vulkan/composite.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D8-NEW-03).

---

# #2800: REN-D8-NEW-04: svgf_temporal_clamps_fireflies test doc names a dead TAA sibling test removed by e5d02f83

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
`svgf_temporal_clamps_fireflies_before_history_branch`'s doc names a TAA sibling test (`taa_comp_floors_alpha_for_moving_pixels_under_parked_camera`) added by `c6342845` and removed by `e5d02f83` — a dead symbol in the doc of a regression guard whose whole purpose is surviving refactors. Live nearest sibling: `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`.

## Location
`crates/renderer/src/vulkan/svgf.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D8-NEW-04).
