# #1166 Investigation — Bloom reads pre-TAA HDR while comment claims post-TAA

## Wiring trace

1. **TAA writes** to `self.history[f].view` (`taa.rs:546-548`, descriptor binding 5 `out_taa`). Confirmed.
2. **Composite is rewired** at `context/mod.rs:1715-1717`:
   ```rust
   let taa_views: Vec<vk::ImageView> = (0..n_frames).map(|i| t.output_view(i)).collect();
   c.rebind_hdr_views(&device, &taa_views, vk::ImageLayout::GENERAL);
   ```
3. `composite.rebind_hdr_views` (`composite.rs:1108-1126`) only updates the **descriptor sets** via `update_descriptor_sets`. It does NOT mutate the `hdr_image_views` field — that field still holds the original raw HDR attachment views snapshotted at line 1677.
4. **Bloom dispatch** at `draw.rs:2324` reads `composite.hdr_image_views[frame]` — the **original raw HDR**, not TAA's output. There is no `bloom.rebind_hdr_views` mirror.

## Diagnosis

The audit is correct on both halves:
- **Comment first half is wrong**: bloom does NOT read post-TAA. It reads pre-TAA raw HDR.
- **Comment second half is right**: "Bloom uses TAA-jittered input ... the blur pyramid suppresses sub-pixel jitter — visually equivalent to bloom on TAA output but with simpler wiring." This is what the code actually does, with a justification.

## Decision: Option B (fix comment, keep wiring)

Per `feedback_speculative_vulkan_fixes` memory: Vulkan render-pass / pipeline / barrier changes with failure modes invisible to `cargo test` should not ship without RenderDoc validation. The audit explicitly says the visual artifact is "below obvious by inspection — needs RenderDoc validation."

The wiring may be intentional (cheaper — bloom doesn't need TAA-output's layout-transition barrier; the blur pyramid smears out sub-pixel jitter anyway). The comment's later half rationalizes this.

Option A (rewire bloom to TAA output) is a speculative Vulkan change. Defer until RenderDoc capture confirms a visible artifact.

Option B: drop the misleading first half; keep + clarify the second half. The comment becomes a definitive statement of intent.

## SIBLING check

SSAO at `draw.rs:2305-2309`: dispatched with `(device, cmd, frame, vp_arr, inv_vp_arr, camera_pos)` — no HDR view passed. SSAO reads depth + normals only. Not affected by the HDR rebind question.

Composite at `mod.rs:1715-1717`: explicitly rewired to TAA output. Correct.

No other HDR-sampling consumer found.
