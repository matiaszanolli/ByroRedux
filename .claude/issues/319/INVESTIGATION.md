# #319 — R34-01 / R34-02: Stale renderer doc comments

## R34-01: vertex.rs references push constants that no longer exist

Pipeline layout has **no push constants** — `pipeline.rs:220-222` explicitly
states so, and set layouts are `[descriptor_set_layout, scene_set_layout]`
only. Per-instance `model` and `bone_offset` live in the instance SSBO
(set 1, binding 4), populated via #294's global-geometry pipeline.

Stale comments (all in `crates/renderer/src/vertex.rs`):

- **L11-13** — "falls through to the push-constant `model` matrix"
- **L22-24** — "see ... the per-draw `bone_offset` push constant"
- **L32-35** — `Vertex::new` doc: "routes the vertex through the
  push-constant `model` matrix"

Sibling found during `grep -rn "push constant"`:

- **pipeline.rs:416-418** — `create_ui_pipeline` doc says "Uses the same
  pipeline layout as the scene pipelines (push constants + descriptor
  set…)". The UI pipeline in fact shares the scene layout, which has
  descriptors only, no push constants. Folded into the same fix.

Fix: rewrite each comment to reference the instance SSBO (set 1, binding 4)
with the actual field names (`model`, `bone_offset`).

## R34-02: helpers.rs lists wrong gbuffer formats

Actual formats (`vulkan/gbuffer.rs:37,39`):

- `NORMAL_FORMAT   = R16G16_SNORM` (octahedral-encoded, 4 B/px — per #275)
- `MESH_ID_FORMAT  = R16_UINT`     (65534-instance ceiling; background = 0,
  shader writes `id + 1`)

Stale comment (`crates/renderer/src/vulkan/context/helpers.rs:49-51`):

- **L49** — `normal (RGBA16_SNORM) — world-space surface normal`
- **L51** — `mesh_id (R16_UINT) — per-instance ID + 1`
  (format is right; the instance ceiling is the missing context the audit
  flagged)

Fix: update the two lines to the authoritative formats.

## Scope

Doc-only. No shader recompile, no runtime behavior change. Test suite
unaffected.
