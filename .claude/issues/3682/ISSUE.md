# #3682 — PERF-D2-2026-08-30-03: the per-instance `GpuInstance` loop probes two `std::collections::HashMap`s per draw per frame, and the #3061 guard structurally cannot see them

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D2-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3682

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `crates/renderer/src/texture_registry.rs:134` (`texture_has_alpha: HashMap<TextureHandle, bool>`) and `:141` (`texture_avg_rgb: HashMap<TextureHandle, [f32; 3]>`); read sites `crates/renderer/src/vulkan/context/draw.rs:2939-2942` and `:2981-2990`
- **Status**: NEW — a new site of the #3061 (CLOSED) hot-path-hashing cluster, in a file that cluster's fix and guard never covered
- **Description**: In the `for draw_cmd in draw_commands` loop that builds `GpuInstance`,
  `handle_avg_rgb(draw_cmd.texture_handle)` is called **unconditionally for every draw command**
  (outside the `skip_batch` gate, because RT hits read `avg_albedo` off off-frustum instances), and
  `handle_has_alpha(draw_cmd.texture_handle)` is called for every alpha-blend draw. Both resolve
  through `std::collections::HashMap` — SipHash-1-3 — over a `TextureHandle` key that is a **dense
  index** (`let handle = self.textures.len() as TextureHandle;`, `texture_registry.rs:678`) into the
  registry's own `textures: Vec<TextureEntry>` (`:127`). This is the per-frame per-entity keyspace the
  #2923 hot-path-hashing rule names, in the crate the rule names, and it is the highest-volume site in
  the cluster — once per `DrawCommand`, i.e. up to 3 949 probes/frame on `fo4-InstituteBioScience`,
  against #3061's morph/skin sites which are bounded by skinned-entity count.
  The guard that exists to stop this cluster drifting back is a source-text scan whose corpus is
  `include_str!("mod.rs")` + `init.rs` + `draw.rs` + `skinned_blas_refit.rs`
  (`crates/renderer/src/vulkan/context/mod.rs:2823-2828`, `:2877`, `:2882`, `:2977`) — `texture_registry.rs`
  is outside it, so these two fields are invisible to it by construction.
- **Evidence**:
  ```rust
  // crates/renderer/src/vulkan/context/draw.rs:2981-2990 — no gate above it
  let gi_albedo = match self
      .texture_registry
      .handle_avg_rgb(draw_cmd.texture_handle)
  { Some(mean) => [ /* … */ ], None => draw_cmd.avg_albedo };
  ```
  ```rust
  // crates/renderer/src/texture_registry.rs:718-720
  pub fn handle_avg_rgb(&self, handle: TextureHandle) -> Option<[f32; 3]> {
      self.texture_avg_rgb.get(&handle).copied()
  }
  ```
  Both maps are written only at the two DDS-load points (`:693`, `:1001`, `:1013`) — load-time, never
  per frame — so nothing about them is DoS-facing (unlike `path_map: HashMap<String, …>` at `:128`,
  which should stay std). The remaining three callers of `handle_has_alpha`
  (`byroredux/src/cell_loader/terrain.rs:692`, `byroredux/src/cell_loader/spawn/mesh_instance.rs:853`,
  `byroredux/src/scene/nif_loader.rs:1104`) are all load-time.
- **Impact**: Small but strictly-wasted CPU on the frame's largest loop, growing linearly with draw
  count — i.e. worst exactly on the cell (`fo4-InstituteBioScience`, 44.3 FPS p50) that is already the
  slowest of the five. No correctness effect. The structural half matters more than the cycles: the
  guard the project added after revisiting this cluster four times (#1368 → #2174 → #2923 → #3061)
  cannot observe the two busiest remaining sites.
- **Related**: #3061 (CLOSED — the conversion landed for `skin_slots` / `morph_slots` /
  `failed_skin_slots` / `failed_skin_blas` / `blend_pipeline_cache` / `blend_seen_scratch`), #2923,
  #2174, #1368; `PERF-D6-2026-08-24-01` (`AUDIT_PERFORMANCE_2026-08-24.md`, the morph sibling, same class)
- **Suggested Fix**: Because `TextureHandle` is a dense index into `textures`, the right fix removes
  the hashing rather than swapping the hasher: move `has_alpha: bool` and `avg_rgb: Option<[f32; 3]>`
  onto `TextureEntry` and make both accessors `self.textures.get(handle as usize)`. If that is too
  invasive, `FxHashMap` is the minimum. Either way, extend the #3061 source-scan corpus to include
  `texture_registry.rs` so the cluster cannot re-grow outside `context/`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
