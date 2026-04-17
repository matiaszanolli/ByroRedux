# #321 — Caustic scatter pass (Option A)

## Shipped scope

Option A from the issue: per-refractive-pixel scatter, single-bounce
refraction, ray-query to find the hit surface, atomic splat at the hit's
screen-space position. Caustic is a scalar luminance buffer (R32_UINT
fixed-point); composite re-modulates by the receiver's local albedo so
colored surfaces pick up the caustic with their own tint.

Option B (photon-style) and Option C (fake texture projection) are out of
scope — Option A landing first unblocks the visible lighting gap on
window / goblet / bottle scenes. Option B / C remain open enhancements.

### What's wired

| Layer                                           | File                                                       |
|-------------------------------------------------|------------------------------------------------------------|
| Refractive-source flag bit 2 on `GpuInstance`   | `scene_buffer.rs` (docs), `context/draw.rs` (CPU wiring)   |
| Shader struct sync (bit 2 comment)              | `shaders/triangle.vert`, `triangle.frag`, `ui.vert`        |
| Caustic compute shader                          | `shaders/caustic_splat.comp`                               |
| Caustic pipeline Rust module                    | `vulkan/caustic.rs`                                        |
| Registered in workspace                         | `vulkan/mod.rs`                                            |
| SceneBuffers accessor for instance buffer       | `vulkan/scene_buffer.rs`                                   |
| Composite binding 5 = caustic `usampler2D`      | `vulkan/composite.rs`, `shaders/composite.frag`            |
| VulkanContext field + creation + drop           | `vulkan/context/mod.rs`                                    |
| Per-frame dispatch + TLAS rebind                | `vulkan/context/draw.rs`                                   |
| Swapchain-resize rebuild                        | `vulkan/context/resize.rs`                                 |

### Design choices

- **Scalar caustic (R32_UINT atomic accumulator)**: atomic adds work on
  u32 storage images on every desktop GPU. Fixed-point 16.16 via
  `CAUSTIC_FIXED_SCALE = 65536.0` gives ample precision + headroom.
  Color tint is approximated by modulating the splat intensity with the
  source instance's pre-computed average albedo — colored glass dims
  accordingly. Full RGB caustics (3× atomic accumulators) is a V2.
- **Fixed IOR = 1.5** (glass): wired as a tunable in `CausticPipeline`
  (`ior` field) so we can expose a per-material value later. Water
  (1.33) would just mean a separate flag bit + per-flag IOR in the
  shader.
- **Gate on `alpha_blend && metalness < 0.3`** CPU-side: matches the
  fragment shader's `isGlass` heuristic closely enough to keep alpha-
  tested foliage and metal out of the splat set.
- **Scatter (not gather)**: each refractive pixel computes its own
  refracted direction, ray-queries the receiving surface, and projects
  back to screen coordinates for `imageAtomicAdd`. Gather would require
  per-output-pixel neighborhood scanning with huge error bars — scatter
  gives correct-at-source caustic positions.
- **Per-FIF storage**: two accumulator images (MAX_FRAMES_IN_FLIGHT).
  Each frame clears its own slot via `vkCmdClearColorImage`, then
  dispatches. Composite samples the *current* frame's slot.
- **Layout**: accumulator lives in `GENERAL` throughout its lifetime
  (same policy as SVGF history). Composite samples via a second image
  view bound as `COMBINED_IMAGE_SAMPLER` → SPIR-V `usampler2D`.

### Draw order

```
main RP → G-buffer ready (SHADER_READ_ONLY)
SVGF.dispatch (indirect denoise)
Caustic.write_tlas(current_tlas) + Caustic.dispatch (clear → splat)  ← NEW
TAA.dispatch (HDR reprojection)
SSAO.dispatch
Composite.dispatch (reads caustic binding 5)
```

### Barriers

- `HOST → COMPUTE` on the params UBO.
- `COMPUTE|FRAGMENT → TRANSFER` + `TRANSFER → COMPUTE` around the
  accumulator clear (`vkCmdClearColorImage` + `TRANSFER_WRITE`).
- `COMPUTE → FRAGMENT` after the dispatch so composite's fragment
  shader sees the atomic adds.

### Sync with the Shader Struct Sync invariant

The new bit 2 of `flags` is named in lockstep across
`scene_buffer.rs`, `triangle.vert`, `triangle.frag`, and `ui.vert` —
the existing `gpu_instance_layout_tests` (size + offset asserts) still
passes because the struct shape didn't change; only the semantics of
a previously-unused flag bit.

### Verified

- `cargo check --workspace` clean.
- `cargo test --workspace` 631 passing (+2 from new GpuInstance layout
  tests added in #318; no caustic-specific unit test — the feature is
  visual and requires a live scene).
- `cargo run --release` launches, `Caustic pipeline created: 1280x720`
  appears in the info log, no Vulkan validation errors through the
  first few frames.

### Known limitations (deferred to V2)

- Single-bounce refraction — thick glass (~1 cm wall) has two
  refraction interfaces; we only model one. Flat panes and windows
  look right; curved bottles are approximate.
- Scalar (grayscale) caustic luminance with receiver-albedo tint. For
  strongly colored glass a 3× R32_UINT RGB accumulator is the right
  upgrade.
- No explicit denoise pass. TAA absorbs most of the motion-induced
  shimmer; if noise under camera motion is objectionable, a short
  temporal accumulation on the caustic buffer (same shape as SVGF's
  history) is the next step.
- Cluster-cull integration: the shader loops the first N lights
  (default 8) unconditionally rather than the per-tile cluster light
  list. For exterior scenes with many lights this means the "wrong"
  8 lights get caustic rays; landing the cluster list is a V2.
