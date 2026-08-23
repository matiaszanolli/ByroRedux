# Incremental Audit — 2026-08-23

**Scope**: `git diff HEAD~10..HEAD` (10 commits, `46cee62b..eac9b0e0`)
**Method**: `.claude/commands/audit-incremental/SKILL.md`, cross-checked against
`_audit-common.md` / `_audit-severity.md`. `cargo check --workspace` and
`cargo test --workspace --no-run` both pass clean against the current tree.

## 1. Change summary

```
git log --oneline HEAD~10..HEAD
eac9b0e0 Progress on #3231: wire morph blend into skin_vertices.comp
5f4dea46 Progress on #3231: grow GpuInstance for GPU morph-target blending
c1339301 Progress on #3231: forward morph-target vertex deltas into ImportedMesh
7fbc5baf Fix #2221: wire animated alpha/color/shader/flipbook sinks into draws
6fad32ac Fix #1749: finish extracting VulkanContext::new() into init.rs/teardown.rs
900aa081 Fix #973: apply MSWP material-swap list per-shape at mesh-spawn time
a9a42a16 Fix #3079: re-point audit-speedtree SKILL.md is_spt dispatch to synth_child.rs
a58418ce docs: correct stale EX-07 deadline-bounded-streaming claims (#2376)
1df3e2f4 feat(ai): wire single-tile NAVM pathing into wander/patrol_system (#2372)
46cee62b feat(ai): wire single-tile NAVM pathing into escort_system (#2372)
```

69 files changed, 4663 insertions(+), 2036 deletions(-). Five themes:

1. **#3231 — GPU morph-target blending (3 commits).** `crates/nif/src/import/mesh/morph.rs`
   (new) extracts `NiGeomMorpherController` vertex deltas into `ImportedMesh`;
   `GpuInstance` grows 128→160 B (`morph_delta_address` / `morph_weight_address` /
   `morph_target_count`); `skin_vertices.comp` + `SkinPushConstants` (12→32 B) gain
   the actual GPU blend. **On the committed HEAD every new field is a hard-coded
   `0` stub** (`context/draw.rs`, `context/skinned_blas_refit.rs`) — the real
   `MorphSlot` GPU resource that would populate them non-zero is present in the
   **working tree only** (`crates/renderer/src/vulkan/morph_compute.rs` is
   untracked; `crates/renderer/src/lib.rs`'s re-export and `byroredux/src/render/skinned.rs`'s
   changes are uncommitted). This is intentional incremental staging, not a
   defect — flagged here only so the stub state isn't misread as "wired but
   broken." Out of scope for this delta audit (uncommitted).
2. **#2221 — animated alpha/color/shader/flipbook sinks (1 commit).** `anim_convert.rs`
   grows `attach_animation_sinks` to resolve `TextureFlipChannel` → bindless
   handles and attach `AnimatedTextureFlip`; `systems/animation.rs` adds
   `apply_texture_flip_channels`; `render/static_meshes.rs` reads all the new
   sinks (alpha/diffuse/ambient/specular/emissive/shader-color/shader-float/
   texture-flip) into `DrawCommand`; `GpuMaterial` grows 348→364 B for the
   still-unsampled `shader_color_*`/`shader_float` fields.
3. **#973 — per-shape MSWP material swap at mesh-spawn time (1 commit).**
   `cell_loader/spawn/mesh_instance.rs::resolve_mesh_paths` re-evaluates the
   REFR's XMSP swap table + FNAM filter against each shape's own authored
   material path, not just the REFR overlay's single shared one.
4. **#1749 — `VulkanContext::new()` extraction (1 commit).** `context/mod.rs`
   (1902 lines removed) → `context/init.rs` (1639 new) + `context/teardown.rs`
   (387 new). Confirmed mechanical: ~1850 of ~1877 removed lines in `mod.rs`
   reappear verbatim in the two new files; the `include_str!("mod.rs")`
   self-referential tests were correctly re-pointed at `init.rs`/`teardown.rs`.
5. **NAVM single-tile pathing (2 commits, #2372).** `wander_system`,
   `patrol_system`, `escort_system` gain resident-tile waypoint routing via
   `resolve_cached_waypoints`/`step_along_waypoints`/`pop_reached_waypoint`,
   mirroring the existing `travel_system`/`follow_system`/`guard_system` shape.

## 2. Routing map

| Changed path | Routed to | Notes |
|---|---|---|
| `crates/nif/src/import/mesh/morph.rs` (new), `mesh/{mod,ni_tri_shape,bs_tri_shape,bs_geometry}.rs`, `import/types.rs` | `/audit-nif`, `/audit-nifal` | Morph extraction + `ImportedMesh.morph_targets` wiring |
| `crates/nif/src/anim/{entry,mod}.rs` | `/audit-nif` | `walk_controller_chain` made `pub(crate)`, reused by morph extraction |
| `crates/core/src/animation/{interpolation,types,mod}.rs` | `/audit-ecs` Dim 10, `/audit-nifal` Dim 7 | `sample_texture_flip_index` + shared `sample_float_keys` |
| `crates/core/src/ecs/components/animated.rs`, `ecs/components/mod.rs`, `ecs/mod.rs` | `/audit-ecs` | `AnimatedTextureFlip` / `TextureFlipEntry` components |
| `byroredux/src/anim_convert.rs`, `systems/animation.rs` | `/audit-ecs` Dim 10, `/audit-nifal` Dim 7 | Sink attach/apply for #2221 (flipbook half) |
| `byroredux/src/render/static_meshes.rs`, `render/particles.rs`, `render/draw_sort_key_tests.rs` | `/audit-ecs`, `/audit-renderer`, `/audit-performance` | Animated-sink → `DrawCommand` wiring, `GpuMaterial` field additions |
| `crates/renderer/src/vulkan/scene_buffer/{constants,gpu_types,gpu_instance_layout_tests,shader_contract_tests}.rs`, `material.rs` | `/audit-renderer`, `/audit-nifal` (GPU-struct rows) | `GpuInstance` 128→160 B, `GpuMaterial` 348→364 B |
| `crates/renderer/shaders/{include/bindings.glsl,triangle.vert,ui.vert,water.vert,caustic_splat.comp,skin_vertices.comp}` (+ `.spv`) | `/audit-renderer` + GPU-struct-sync rule | All 6 `GpuInstance`/`SkinPushConstants` mirrors checked — in lockstep |
| `crates/renderer/src/vulkan/context/{mod,init,teardown,draw,resize,resources,skinned_blas_refit}.rs`, `frame_upscaler.rs` | `/audit-renderer`, `/audit-safety`, `/audit-concurrency` | #1749 constructor split; `#3231` stub fields |
| `crates/renderer/src/vulkan/skin_compute.rs`, `water.rs`, `acceleration/tests/mod.rs` | `/audit-renderer` | `SkinPushConstants` growth, `DrawCommand` literal updates |
| `byroredux/src/cell_loader/{spawn,spawn/mesh_instance,refr,precombined,references/synth_child}.rs` | per-game `/audit-fo4` | #973 per-shape MSWP |
| `byroredux/src/scene.rs`, `scene/nif_loader.rs` | per-game audits | `attach_animation_sinks` call-site parity (both load paths updated identically) |
| `byroredux/src/systems/{escort,patrol,wander}.rs` | `/audit-ecs`, `/audit-performance` | NAVM single-tile pathing, mirrors `travel`/`follow`/`guard` shape |
| `crates/spt/src/import/mod.rs` | `/audit-speedtree` | `morph_targets: None` for synthesized placeholder billboards |
| `byroredux/src/save_io/registry_completeness_tests.rs` | `/audit-save` | `AnimatedTextureFlip` added to the derived-component allowlist |
| `docs/engine/{exterior-readiness-plan,navmesh-pathfinding}.md` | `/audit-tech-debt` (doc rot) | Progress-tracking updates, not reviewed for content accuracy |

## 3. Findings

### F1 — MSWP per-shape swap breaks "later-wins" for duplicate-source entries (MEDIUM)

**Changed in**: `byroredux/src/cell_loader/spawn/mesh_instance.rs` (commit `900aa081`, `resolve_mesh_paths`)

The new per-shape swap loop:

```rust
let mut swapped = current.clone();
for entry in &refr_ov.material_swaps {
    if entry.source.eq_ignore_ascii_case(&swapped) && !entry.target.is_empty() {
        swapped = entry.target.clone();
    }
}
```

compares each entry's `source` against `swapped`, a variable that is
reassigned on every match. This means:

- For two `material_swaps` entries with the **same** `source` (a duplicate
  BNAM→SNAM pair — legal in the MSWP format and exactly the case the
  "later-wins" comment above this loop, and `refr.rs`'s own comment at
  line 379-380 ("the spawn path applies them per shape with later-wins
  semantics matching the MSWP file format"), claim to handle) — only the
  **first** matching entry ever fires. After it fires, `swapped` no longer
  equals the shared `source` string, so the second entry's comparison
  against `swapped` fails and it is silently skipped. This is the reverse
  of the documented and intended "later entry overrides" semantics.
- Conversely, if one entry's `target` happens to equal a *later* entry's
  `source` (incidental string collision, not a duplicate), the two entries
  silently **chain** (A→B→C) — behavior nothing in the format or the
  surrounding comments describes or intends.

**This is a real, confirmed regression relative to the sibling reference
implementation already in the same file family** — `refr.rs`'s
`build_refr_texture_overlay` (the original, single-`material_path`,
REFR-level MSWP application from #971, unchanged in this diff) implements
the same "later-wins" rule correctly, by comparing every entry against the
**fixed** `current` value and simply re-overwriting `ov.material_path` on
every match:

```rust
// refr.rs:393-399 (unchanged, correct reference implementation)
for entry in &table.swaps {
    if entry.source.eq_ignore_ascii_case(&current) && !entry.target.is_empty() {
        ov.material_path = Some(pool.intern(&entry.target));
    }
}
```

The new per-shape consumer in `mesh_instance.rs` should use the same
pattern (compare against a fixed `current`, keep overwriting `swapped`)
rather than comparing against the mutating running value. This is exactly
the "silent divergence — value built at two sites and only one edited"
pattern this skill's checklist calls out by name.

**Impact**: narrow but real — triggers only when a single MSWP record lists
more than one swap entry for the same source BGSM/BGEM (vanilla MSWPs
average ~2.18 entries per the codebase's own count, so duplicates are
plausible in denser variant tables, e.g. multi-tier color swaps that touch
the same base material twice). When it triggers, the wrong material variant
is silently applied to that shape — visually wrong content, no crash, no
error.

**Fix**: compare `entry.source` against `current` (never mutated), not
`swapped`, mirroring `refr.rs`'s existing correct loop.

**Dedup check**: not covered by the parallel NIFAL/Safety sweep (that sweep's
two flagged items are the morph index-space desync tied to #3231 and the
`GpuInstance` layout-growth review — neither touches `#973`/MSWP). No
existing open/closed issue found for this specific defect (`gh issue list`
search on MSWP/#973 turned up only #973 itself, closed, whose own suggested-fix
pseudocode in the issue body carries the identical latent bug — worth noting
when filing, since the shipped code matches the issue's own snippet).

### F2 — `AnimatedTextureFlip` cross-clip merge gap has no test coverage (LOW / missing test)

**Changed in**: `byroredux/src/anim_convert.rs` (commit `7fbc5baf`)

`insert_missing_sinks` (pre-existing helper, reused unchanged) skips
inserting a component on any entity that already carries one of that type.
For scalar sinks (`AnimatedAlpha`, `AnimatedShaderFloat`, …) this is the
documented, correct "a second clip must not reset an already-live value"
policy, and is covered by an explicit test
(`second_clip_does_not_overwrite_live_value`-style tests already exist for
several scalar sinks).

`AnimatedTextureFlip` is different: it's a `Vec<TextureFlipEntry>` keyed by
`texture_slot`, explicitly designed (per its own doc comment) to hold
*multiple* independent flipbooks on one entity — "a shape can in principle
carry more than one `NiFlipController` targeting different texture slots."
That merge is correctly handled **within a single `attach_animation_sinks`
call** (one clip's channels grouped into one `HashMap<EntityId,
Vec<TextureFlipEntry>>` before insertion). But if a **second, later-attached
clip** on the same entity (e.g. a different `AnimationStack` layer, or a
second NIF-embedded clip) introduces a texture-flip channel for a **new**
`texture_slot` the entity doesn't have yet, `insert_missing_sinks` sees the
entity already has *an* `AnimatedTextureFlip` (from the first clip) and
drops the whole insert — the second clip's slot is never added to the Vec.
`apply_texture_flip_channels`'s `.find(|e| e.texture_slot == channel.texture_slot)`
then permanently no-ops for that channel.

This is an inherited limitation of the pre-existing `insert_missing_sinks`
design (the same trade-off already accepted for `AnimatedMorphWeights`), not
a new bug introduced by this diff, and the comment describing multi-slot
support only claims (and only tests) the single-clip case. No test exercises
the cross-clip scenario for the new `AnimatedTextureFlip` type specifically,
so it's recorded as a missing-test item rather than a finding requiring a
code fix — see §4.

## 4. Missing tests

- `byroredux/src/anim_convert.rs` — no test for two separately-attached
  clips on the same entity each targeting a *different* `texture_slot`
  (the scenario in F2). Existing tests only cover (a) one clip with
  multiple slots in a single call, and (b) a second clip's *scalar* sink
  not overwriting a live value — neither exercises the Vec-merge gap for
  `AnimatedTextureFlip`.
- `byroredux/src/cell_loader/spawn/mesh_instance.rs` — no test with two
  `material_swaps` entries sharing the same `source` (the exact case that
  would have caught F1). The two new tests
  (`mswp_swaps_apply_per_shape_not_just_the_overlay_material_path`,
  `mswp_filter_is_re_evaluated_per_shape`) each use one swap per distinct
  source only.
- `crates/renderer/src/vulkan/morph_compute.rs`, the `MorphSlot`-consuming
  changes in `context/draw.rs`/`context/skinned_blas_refit.rs`, and
  `byroredux/src/render/skinned.rs` are present only in the **uncommitted
  working tree**, not in this commit range — noted for completeness (see
  §1 item 1) but out of scope for a missing-test call here since they
  aren't part of the audited diff yet; they do already carry their own
  passing unit tests (`vulkan::context::draw::morph_gpu_fields_tests::*`)
  in the working tree as observed during this audit.

Everything else in the diff (morph vertex-delta extraction, `GpuInstance`/
`GpuMaterial`/`SkinPushConstants` layout growth, the animated-sink →
`DrawCommand` → `GpuMaterial` pipeline, the `VulkanContext` constructor
split, and the NAVM single-tile pathing in escort/patrol/wander) ships with
thorough, specific regression tests for the new code paths, including the
edge cases (vertex-count mismatch, truncation cap, unresolved data_ref,
empty source paths, negative curve values, paused/no-cache states, and
full `DrawCommand → GpuMaterial` interning round-trips).

---

Severity counts: **0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.**

Files flagged for missing test coverage:
- `byroredux/src/anim_convert.rs`
- `byroredux/src/cell_loader/spawn/mesh_instance.rs`

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-08-23.md
```
