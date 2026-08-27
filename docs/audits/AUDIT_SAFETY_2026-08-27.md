# Safety Audit — 2026-08-27

Run as part of the `streaming-deep` `/audit-suite` preset.
Protocol: `.claude/commands/_audit-common.md` · severity scale:
`.claude/commands/_audit-severity.md` · dimensions:
`.claude/commands/audit-safety/SKILL.md` (all 11).

Delta since the previous sweep (`AUDIT_SAFETY_2026-08-24.md`): **~40 commits**
in three days, concentrated in exactly the area this preset weights — resumable
streaming (`#3298` chunked global geometry SSBO rebuild, `#2369` persistent-CELL
crossing), GPU morph-target deformation (`#3231`/`#3233`/`#3244`, a brand-new
per-entity GPU resource on the spawn path), the BGEM glass-optics + Bethesda
soft/rim/back lighting material growth (`GpuMaterial` 396 → 432 B), and a new
`crates/sdk` crate.

## Scope line

All 11 dimensions executed. Per-dimension depth actually achieved, stated honestly:

| Dim | Depth | Notes |
|---|---|---|
| 1 FFI lifetime | **Medium** | `fsr3-sys` + `cxx-bridge` re-verified statically; both unchanged since the last sweep. `crates/ui` re-confirmed to contain **zero** `unsafe`. No code read line-by-line beyond the `unsafe` surface. |
| 2 Memory corruption / UB | **Deep** | ECS cached-pointer contract, `read_pod_vec`, `pex` opcode transmute contiguity (counted: 51 variants, `MAX_OPCODE = 51`), `sfmaterial` checked decode, `#[repr(C)]` GPU structs all re-derived from source. |
| 3 Leaks & drop ordering | **Deep** (preset priority) | All three regression guards re-verified; the new `MorphSlot` lifecycle traced spawn → eviction → unload-victim drain → teardown; exterior cancel path traced end to end. |
| 4 Unsafe-block discipline | **Deep, mechanised** | 718 `unsafe {` blocks repo-wide, **718 with a SAFETY comment**. Zero gaps. |
| 5 Vulkan spec | **Shallow-static only** | No validation-layer run: the preset forbids launching `byroredux`. Everything below is derived from code, never from an observed VUID. See the standing-rule note. |
| 6 R1 material layout | **Deep, mechanised** | `GpuMaterial` Rust ↔ GLSL diffed field-for-field in order (108 vs 108, exact match); all five `GpuInstance` GLSL copies diffed against the Rust struct. |
| 7 RT IOR / glass | **Medium** | All five regression guards checked by name + value; debug-bit collision scan run (32 bits, no collision). |
| 8 NPC / animation spawn | **Deep** (preset priority) | `#772` sentinel, `#790` dedup, `MAX_TOTAL_BONES` guard, plus a full trace of the resumable `NpcSpawnJob` cancel/ownership path. |
| 9 NIFAL NaN/Inf | **Deep** | `sanitize_finite` coverage diffed against the live `Material` float-field list; collision + emitter finiteness gates re-verified. |
| 10 debug-ui teardown | **Medium** | Teardown ordering, deferred texture free, and lock scoping re-verified. |
| 11 mod-runtime | **Medium** | Audited as a contract (still no engine consumer). Two of its findings remain OPEN issues → noted, not re-reported. |

Additional coverage notes:

- **Un-owned subsystems**: `crates/sdk` (new this week) was checked only for
  `unsafe` (it has none) — its contract surface was NOT audited. `crates/hkx`,
  `crates/mod-runtime`, `crates/save` re-confirmed to contain zero `unsafe`;
  for the first two that absence is itself the safety property.
- `cargo check --workspace --all-targets` was run (read-only) and **exits 0** —
  the `fragment_coverage` build break reported as SAFE-BUILD-2026-08-24-01 is
  fixed. Warnings only, all `unused_mut` in `_tmp_*` scratch examples.
- **No engine process was launched** and no Vulkan render-pass / barrier /
  pipeline-state change is proposed anywhere in this report, per the standing
  no-speculative-Vulkan-fixes rule.

Dedup performed against `/tmp/audit/issues.json` (400 issues, open + closed) and
`docs/audits/` (29 prior safety reports).

**Cross-agent de-confliction**: the persistent-CELL / `PersistentCellApplyJob`
abandonment and its `AnimationClipRegistry` handle leak are owned by the
concurrency agent and are NOT re-reported here. Rapier body/collider release is
owned by the physics agent (re-verified as intact, recorded as PASS only).
CHARAL population correctness in `npc_spawn.rs` is owned by the character agent;
only the safety facet is covered below.

## Summary

**4 findings**: 1 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW.

The centre of gravity is `#3298`, the two-day-old resumable geometry-SSBO
rebuild. Making a previously-atomic operation span frames opened a window in
which the CPU-side mesh offsets and the bound GPU buffer describe two different
layouts — the exact hazard class that atomicity was silently providing.

### Prior-report disposition

| Prior finding | Status today |
|---|---|
| SAFE-BUILD-2026-08-24-01 (`fragment_coverage.rs` build break) | **FIXED** — `cargo check --workspace --all-targets` exits 0 |
| SAFE-D2-2026-08-23-01 (#3237, GRUP recursion depth) | Closed & verified in the 08-24 report; re-confirmed closed |
| SAFE-D9-2026-08-23-01 (#3238, Rapier shape clamp) | Closed & verified in the 08-24 report |
| SAFE-2026-08-20-01 (WATR NaN-transparent clamps) | Not re-litigated; the *same class* recurs in a different struct → SAFE-2026-08-27-02 |
| #3050 (mod-runtime log budget has no drain) | Still OPEN. The cap now exists three ways (entries / per-message bytes / total bytes, `runtime.rs:272-287`); only the *drain* is missing. Noted, skipped |
| #3051 (no hostile-bytes `compile` test) | Still OPEN. Noted, skipped |
| #3052 (`REFRACT_PASSTHRU_BUDGET` doc rot) | The skill text now names the real symbols; `MAX_REFRACT_PASSTHRUS` + the adaptive `refractPassthruBudget` both verified present |
| #3244 (MorphSlot weight buffer host-write race) | Issue still OPEN in the cached snapshot, but the fix **is in the tree**: `flush_pending_weights` is called from `draw_frame` after its dual-fence wait and is pinned by `draw_flushes_pending_morph_weights_after_waiting_both_fences`. Not re-reported |

---

## Findings

### CRITICAL

#### SAFE-2026-08-27-01: `#3298`'s chunked geometry rebuild publishes compacted mesh offsets against the pre-compaction GPU buffer for the whole multi-frame copy — BLASes built in that window bake wrong geometry

- **Severity**: CRITICAL
- **Dimension**: 3 (leaks / resource lifecycle) + 5 (Vulkan / AS correctness)
- **Location**: `crates/renderer/src/mesh.rs:1112-1178` (`rebuild_geometry_ssbo`), `:1129` (the `compact_pending_geometry()` call), `:964-1004` (`compact_pending_geometry`), `:1349-1350` (deferred `ssbo_*_count` update), `:1487-1500` (`is_geometry_resident`); consumers at `byroredux/src/app_frame.rs:205-235`, `crates/renderer/src/vulkan/context/resources.rs:307-333`
- **Status**: NEW. **Regression introduced by `ae7179a3` (Fix #3298, 2026-08-25)**. Issue #3298 is CLOSED; nothing in `/tmp/audit/issues.json` covers this consequence.
- **Description**:

  `compact_pending_geometry` does two things at once: it squeezes dropped
  meshes' spans out of `pending_vertices` / `pending_indices`, **and it
  rewrites every surviving mesh's `global_vertex_offset` /
  `global_index_offset` in place** (`mesh.rs:990-998`). Those offsets are the
  live values the draw path and the BLAS builder read.

  Before `#3298`, `rebuild_geometry_ssbo` called `compact_pending_geometry()`
  and then built the replacement buffer **synchronously in the same call**, so
  the offsets and the bound buffer were never out of step across a frame
  boundary — the window was zero frames.

  `#3298` kept the compaction at the top of `rebuild_geometry_ssbo`
  (`mesh.rs:1129`) but moved the *upload* into a resumable state machine:
  `advance_geometry_rebuild` copies one bounded chunk per call, and one call
  happens per frame. `global_vertex_buffer` / `global_index_buffer` keep
  serving every draw **unchanged** until swap-in — which is by design and is
  the point of the change — and `ssbo_vertex_count` / `ssbo_index_count` are
  likewise only updated at swap-in (`mesh.rs:1349-1350`).

  The result is a window of **at least two frames** (the vertex phase and the
  index phase never share a call — `mesh.rs:1230-1234`), and up to ~15 frames
  at the FO4 boundary-crossing sizes the change was written for (~600 MiB /
  `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB), during which:

  - mesh offsets describe the **compacted** layout, and
  - the bound global buffer holds the **uncompacted** bytes.

  Nothing suppresses drawing in that window. `is_geometry_resident` is the only
  gate, and it cannot catch this: it compares the new (compacted, therefore
  *smaller*) offsets against the old (uncompacted, therefore *larger*)
  `ssbo_*_count`, so it answers `true` for every mesh
  (`mesh.rs:1497-1499`).

  The precondition is `geometry_has_holes` — i.e. any scene mesh dropped since
  the last compaction (`mesh.rs:944-950`). That is precisely what a cell unload
  does, so the hazard fires on the same boundary crossings `#3298` targets, not
  on an exotic path.
- **Evidence**:

  Pre-`#3298` (`git show ae7179a3^:crates/renderer/src/mesh.rs:1004-1016`) —
  compaction and build in one call:
  ```rust
  pub fn rebuild_geometry_ssbo(&mut self, …) -> Result<()> {
      // If any scene meshes were dropped since the last build, compact
      // the pending buffers and rewrite every live mesh's offsets.
      self.compact_pending_geometry();
      …                       // ← synchronous build, same call
  ```

  Today (`crates/renderer/src/mesh.rs:1120-1165`):
  ```rust
      self.compact_pending_geometry();          // offsets rewritten NOW
      …
      Ok((new_vertex_buffer, new_index_buffer)) => {
          self.geometry_rebuild = Some(GeometryRebuildInProgress { … });
          return self.advance_geometry_rebuild(…);   // ← one chunk, then return
      }
  ```
  and the counts that gate residency only move at swap-in
  (`crates/renderer/src/mesh.rs:1345-1350`):
  ```rust
      self.global_vertex_buffer = Some(job.new_vertex_buffer);
      self.global_index_buffer  = Some(job.new_index_buffer);
      self.geometry_generation  = self.geometry_generation.wrapping_add(1);
      self.ssbo_vertex_count    = job.target_vertex_count;
      self.ssbo_index_count     = job.target_index_count;
  ```

  The BLAS builder pairs the *current* buffer with the *current* offsets, with
  no generation check between them
  (`crates/renderer/src/vulkan/context/resources.rs:317-322`):
  ```rust
      (None, None) => (
          global_vertex_buffer?,                                   // OLD generation
          global_index_buffer?,                                    // OLD generation
          u64::from(mesh.global_vertex_offset) * vertex_stride,    // NEW (compacted)
          u64::from(mesh.global_index_offset)  * index_stride,     // NEW (compacted)
      ),
  ```
  and `byroredux/src/app_frame.rs:235` calls
  `ctx.restore_missing_static_blas_for_draws(&self.draw_commands)`
  **every frame**, unconditionally — including every frame of the window — so a
  cell whose meshes need first-sight BLASes during a boundary crossing will
  build them from the wrong byte ranges.

  Attempts to disprove, all failed:
  - *Existing BLASes are fine* — true, and irrelevant: a BLAS bakes its
    geometry at build time, so entries built before compaction still describe
    the correct triangles. The bug is confined to builds **inside** the window.
  - *Maybe the residency gate filters the draws* — no, see above; it returns
    `true` because it mixes new offsets with old counts.
  - *Maybe `geometry_dirty` suppresses the frame* — no.
    `app_frame.rs:219` uses `is_geometry_dirty()` only to run the residency
    filter, which passes.
  - *Maybe compaction rarely runs* — `#2678` made it run **only** when a scene
    mesh was dropped, which is exactly the streaming case.
  - *Maybe the atomic fallback is what actually runs* —
    `rebuild_geometry_ssbo:1145` tries the chunked path first and only falls
    back when the second full-size allocation fails.
- **Impact**:
  - **Acceleration structures built with wrong geometry** (severity table:
    CRITICAL) for any mesh whose first-sight BLAS build lands inside the
    window — shadows, reflections and GI trace against triangles that belong to
    a different mesh.
  - Raster draws in the window fetch vertices/indices from the wrong offsets:
    visibly scrambled geometry for every live mesh past the first hole, for 2
    to ~15 frames at every exterior boundary crossing that unloaded a cell.
  - Blast radius is every game and every streaming path; it does not depend on
    plugin data.
  - Not GPU-out-of-range (compacted offsets are ≤ the old counts), so it is
    corruption rather than a device fault — which is precisely why it can hide.
- **Related**: #3298 (the change that introduced it), #2678 (the
  `geometry_has_holes` gate that makes compaction conditional), #2743
  (`geometry_generation`, the existing mechanism a fix could reuse), #2374 (the
  atomic fallback path, which is unaffected).
- **Suggested Fix**: Keep the two halves of compaction apart from the copy.
  Simplest correct option: have `is_geometry_resident` return `false` for every
  scene mesh while `geometry_rebuild_in_progress()` **and** the rebuild's
  snapshot was taken after a compaction — the meshes then stay out of raster
  and TLAS for the window instead of rendering wrong (a pop, not corruption).
  Cleaner option: snapshot the pre-compaction offsets into
  `GeometryRebuildInProgress` and only publish the compacted offsets at
  swap-in, alongside the `geometry_generation` bump. Either way, add a
  regression test pinning that compacted offsets are never observable while
  the old buffer is bound.

---

### MEDIUM

#### SAFE-2026-08-27-02: `Material::sanitize_finite` misses the four BGEM glass-optics fields, so both save-path NaN gates have a hole in exactly the newest material scalars

- **Severity**: MEDIUM
- **Dimension**: 9 (NIFAL boundary — NaN/Inf on the GPU)
- **Location**: `crates/core/src/ecs/components/material.rs:1032-1088` (`sanitize_finite`); uncovered fields declared in the same file's `Material` struct; consumers `crates/save/src/validate.rs:445-458` and `crates/save/src/driver.rs:142-150`; destination `crates/renderer/src/vulkan/material.rs:361-364` (`GpuMaterial` offsets 364-388)
- **Status**: NEW
- **Description**:

  `sanitize_finite`'s documented contract is "Reset **every** non-finite
  (NaN / ±inf) scalar to its `Material::default()` value"
  (`material.rs:1010-1011`). It is the single implementation both halves of the
  save/load NaN defence depend on: `validate.rs:456` probes a clone with it as
  the **pre-save** gate, and `driver.rs:148` calls it as the **post-restore**
  repair — deliberately, so the field list is not duplicated.

  A mechanised diff of the `Material` struct's float fields against the
  `fix_scalar!` / `fix_vec!` calls shows 33 float fields, of which **four are
  not covered**:

  | Field | Type | GpuMaterial offset |
  |---|---|---|
  | `glass_fresnel_color` | `[f32; 3]` | 364-372 |
  | `glass_refraction_scale` | `f32` | 376 |
  | `glass_blur_scale` | `f32` | 380 |
  | `glass_blur_scale_factor` | `f32` | 384 |

  All four were added on 2026-08-25 (`d9d4a6d7`, BGEM v21+ glass optics), after
  the `sanitize_finite` field list was authored under #2687. Every *other*
  scalar added in the same era — `lighting_effect_1/2`, `subsurface_rolloff`,
  `rimlight_power`, `backlight_power`, `fresnel_power`,
  `grayscale_to_palette_scale` — **is** covered, which is what makes this a
  slip rather than a deliberate exclusion.

  There is no enumerating test: the `sanitize_finite` tests
  (`material.rs:1799-1856`) each poison one or two hand-picked fields, so a
  newly-added field is invisible to them by construction.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/components/material.rs — the tail of sanitize_finite
          fix_scalar!(grayscale_to_palette_scale);
          fix_scalar!(ior);
          fix_scalar!(subsurface);
          fix_scalar!(sheen);
          fix_scalar!(sheen_tint);
          fix_scalar!(anisotropic);
          // ← no fix_vec!(glass_fresnel_color)
          // ← no fix_scalar!(glass_refraction_scale / glass_blur_scale /
          //                  glass_blur_scale_factor)
          changed
  ```
  The four fields reach the GPU unchanged
  (`byroredux/src/material_translate.rs:544-546` → `GpuMaterial` →
  `crates/renderer/shaders/triangle.frag:1500`, `:1731-1734`), and the shader's
  apparent clamp is not a rescue: GLSL `clamp`/`min`/`max` are explicitly
  **undefined** when an operand is NaN — the same NaN-transparency trap
  SAFE-2026-08-20-01 documented for `f32::clamp` on the WATR path.

  The upstream parser offers no gate either: `crates/bgsm/src/reader.rs:62-64`
  is a bare `f32::from_bits(self.read_u32()?)` with no finiteness filter, and
  `bgem.rs:136-142` reads all four fields straight through it.
- **Impact**: A world holding a non-finite glass scalar (a hostile or corrupt
  BGEM in a mod archive is the realistic source) **passes** the pre-save
  validation gate and **survives** restore, putting NaN/±Inf into `GpuMaterial`
  — undefined behaviour on the GPU per this project's own severity rules, not
  merely a visual artefact. Scoped to BGEM-bearing content (FO4 and later),
  which is exactly where the glass path is live.
- **Related**: #2687 / SAFE-D9-01 (the finding that created `sanitize_finite`),
  SAFE-2026-08-20-01 (the same NaN-transparency class on WATR), `d9d4a6d7`
- **Suggested Fix**: Add `fix_vec!(glass_fresnel_color)` plus the three
  `fix_scalar!` calls, and — more durably — add a test that constructs a
  `Material` with every float field poisoned via a macro-generated list and
  asserts `sanitize_finite` returns a fully finite struct, so the next field
  addition cannot silently reopen the hole.

---

### LOW

#### SAFE-2026-08-27-03: the new `MorphSlot` unload drain is nested inside the `skin_compute` + `accel_manager` guards it does not need

- **Severity**: LOW
- **Dimension**: 3 (leaks)
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:65-68` (the enclosing guards), `:772-800` (the morph eviction + `pending_morph_unload_victims` drain); producer `byroredux/src/cell_loader/unload.rs:263-266`; resource `crates/renderer/src/vulkan/context/mod.rs:1478-1481`
- **Status**: NEW
- **Description**: `MorphSlot::destroy` needs only `&ash::Device` and the
  allocator — it owns two plain `GpuBuffer`s and no descriptor sets, by explicit
  design (`morph_compute.rs:1-16`). Its eviction loop and its
  `pending_morph_unload_victims` drain, however, sit inside
  `if let (Some(skin_pipeline), Some(ref mut accel)) = (self.skin_compute.as_ref(), self.accel_manager.as_mut())`,
  a guard inherited from the `SkinSlot` loop it was folded beside. Both of those
  are `None` when `device_caps.ray_query_supported == false`
  (`context/init.rs:428`, `:600`), or when skin-pipeline creation fails.

  `MorphSlot`s themselves are created with **no** such gate
  (`byroredux/src/cell_loader/spawn/mesh_instance.rs:725-746` — the only
  condition is `mesh.skin.is_some()` plus non-empty `morph_targets`). So in any
  configuration where those two options are `None`, morph delta buffers (up to
  `MAX_MORPH_TARGETS_PER_MESH` = 64 × `vertex_count` × 16 B per skinned entity)
  accumulate for the whole session across every cell load and unload, with no
  bound and no drain.

  **Why LOW and not HIGH**, stated plainly: `#2494` already established that
  this drain must not be trapped inside a narrower guard, and a per-cell GPU
  leak would ordinarily be HIGH. But the trigger is not reachable in a
  supported configuration today — `triangle.vert:7` carries
  `#extension GL_EXT_buffer_reference : require`, and `bufferDeviceAddress` is
  enabled only when `ray_query_supported` (`device.rs:743`), so a device that
  makes `accel_manager` `None` cannot create the main geometry pipeline at all.
  This is therefore a latent structural coupling, not a live leak. It is worth
  fixing because it is the `#2494` mistake one nesting level out, and because
  the RT-optional path is a plausible future.
- **Evidence**:
  ```rust
  // skinned_blas_refit.rs:65
  if let (Some(skin_pipeline), Some(ref mut accel)) =
      (self.skin_compute.as_ref(), self.accel_manager.as_mut())
  {
      if let Some(ref alloc) = self.allocator {
          …
          // :782 — needs neither skin_pipeline nor accel
          let mut morph_evictees: Vec<EntityId> =
              std::mem::take(&mut self.pending_morph_unload_victims);
  ```
  Teardown is unaffected — `context/teardown.rs:52-56` drains `morph_slots`
  wholesale on `Drop`, so this is a *session-lifetime* leak, not a
  leak-past-shutdown.
- **Impact**: None in any currently-supported configuration. Under a future
  RT-optional path, or a `skin_compute` creation failure on a live RT device,
  per-cell GPU memory grows without bound until process exit.
- **Related**: #2494 (the same class of over-nesting, one level in), #3231,
  #1003
- **Suggested Fix**: Hoist the `morph_evictees` block out to the
  `if let Some(ref alloc) = self.allocator` level (or above the skin guard
  entirely, taking `alloc` locally), and extend
  `skin_eviction_runs_without_global_vertex_buffer_tests` with a source-position
  assertion for the morph drain, mirroring the one `#2494` added for the skin
  drain.

#### SAFE-2026-08-27-04: `failed_skin_slots`' safety rationale claims `EntityId` is generational — it is a bare `u32`

- **Severity**: LOW
- **Dimension**: 2 (memory corruption / UB — documentation of a safety premise)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1488-1490`
- **Status**: NEW
- **Description**: The doc comment justifying why a stale
  `failed_skin_slots` entry is harmless reads:

  > `EntityId` is generational so an entry can't poison a re-issued id. See #900.

  `EntityId` is `pub type EntityId = u32` (`crates/core/src/ecs/storage.rs:10`)
  — a plain integer with no generation field. The *conclusion* is still correct,
  but for a completely different reason: `World::spawn` is monotonic and
  `World::despawn` never reclaims an id, which `crates/core/src/ecs/world.rs:140-143`
  states explicitly ("Entity IDs are NOT reclaimed … Reuse without generational
  tagging would cause silent component-data corruption"). The comment therefore
  asserts a property the ECS deliberately does **not** have, and asserts it as
  the load-bearing reason an entity-keyed cache is sound.

  This matters more than ordinary doc rot because several renderer caches are
  keyed on `EntityId` (`skin_slots`, `morph_slots`, `failed_skin_slots`,
  `failed_skin_blas`, `pending_*_unload_victims`) and a reader who trusts this
  line will conclude id recycling is already handled. It is not — the invariant
  is "ids are never recycled", and that invariant lives in `world.rs`, not here.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/storage.rs:10
  pub type EntityId = u32;

  // crates/core/src/ecs/world.rs:110-117 — monotonic, checked_add, never recycled
  pub fn spawn(&mut self) -> EntityId {
      let id = self.next_entity;
      self.next_entity = self.next_entity.checked_add(1)
          .unwrap_or_else(|| panic!("World::spawn overflowed EntityId (u32::MAX reached)"));
      id
  }
  ```
- **Impact**: Documentation only — no runtime effect today. The risk is a future
  change that introduces id recycling (a free list, a generational rework) and
  passes review because this comment says the renderer caches already tolerate it.
- **Related**: #900, #372 (the issue that established the never-reclaim rule), #36
- **Suggested Fix**: Replace "is generational" with "is never recycled —
  `World::spawn` is monotonic and `despawn` does not reclaim (#372)", and
  cross-reference `crates/core/src/ecs/world.rs:140-143` as the owning invariant.

---

## PASS — verified intact

Recorded so a future sweep does not re-derive them.

### Dimension 1 — FFI lifetime
- `crates/cxx-bridge/src/lib.rs` is still the placeholder: one bridge fn,
  `native_hello() -> String`. **No** `*const`, `&[u8]`, `Box<…>`, or
  reference-taking `unsafe extern "C++"` signature. Scope guard holds.
- `crates/fsr3-sys`: `Context::create` and `Context::dispatch` both carry
  `# Safety` sections with lifetime contracts (`lib.rs:365-379`, `:403-408`);
  every free-function `unsafe` block (`:230`, `:247`, `:262`, `:276`, `:154`)
  has a SAFETY comment. Unchanged since 2026-08.
- Ruffle / wgpu boundary: `crates/ui` contains **zero** `unsafe` — the only grep
  hit is a log string. Frame capture goes through safe wgpu APIs;
  `SwfPlayer::Drop` (`player.rs:658-665`) touches no Vulkan handle.

### Dimension 2 — Memory corruption / UB
- `QueryRead` / `QueryWrite` / `ComponentRef` cached pointers
  (`crates/core/src/ecs/query.rs:58-64`, `:130-143`, `:284-289`): each SAFETY
  comment still matches the field layout, and the cached pointer targets the
  **pinned boxed storage struct**, not an interior `Vec` element — so a
  `storage_mut()`-driven reallocation cannot dangle it. `&mut self` still gates
  `&mut *self.storage`.
- `NifStream::read_pod_vec` (`crates/nif/src/stream.rs:438-470`): `checked_mul`
  overflow guard + `check_alloc` + `T: AnyBitPattern` bound + big-endian compile
  gate, all present.
- `OpCode::from_u8` (`crates/pex/src/opcode.rs:130-137`): 51 enum variants
  counted, `Nop = 0` and no gaps, `MAX_OPCODE = 51`, `byte >= MAX_OPCODE` check
  before the transmute. Both preconditions hold.
- `BuiltinType::from_u32` (`crates/sfmaterial/src/types.rs:37-57`): still a
  checked `match` with `_ => return Err(Error::UnsupportedBuiltin { raw })`. No
  transmute.

### Dimension 3 — Leaks & drop ordering
- **Rapier release (#1520)**: `unload.rs:295` →
  `release_victim_rapier_bodies` (`:510-540`) → `pw.remove_body`;
  `rapier_release_tests.rs` present. (Physics agent's dimension —
  recorded, not analysed further.)
- **Deferred-destroy drain (#418/#732)**: `context/draw.rs:1628` waits the
  fences, `:1773-1797` ticks the three queues afterwards. Shutdown drain via
  `App::shutdown` → `ctx.flush_pending_destroys()` (`app_events.rs:53`).
- **`AllocatorResource` removal (#1406)**: removed before the renderer is
  dropped on both paths — `app_events.rs:57-60` (explicit shutdown) and
  `main.rs:338-343` (`impl Drop for App`, the panic-unwind path).
- **`MorphSlot` (new, #3231)**: full lifecycle traced — created at
  `mesh_instance.rs:725-746`, LRU-evicted and unload-drained at
  `skinned_blas_refit.rs:772-800`, victims queued by `unload.rs:263-266`, bulk-torn
  down at `teardown.rs:52-56`. Delta-buffer size is bounded by
  `MAX_MORPH_TARGETS_PER_MESH = 64` with per-target vertex-count parity
  enforced (`crates/nif/src/import/mesh/morph.rs:75-96`), so untrusted NIF input
  cannot drive an unbounded allocation. Weight staging length is derived from
  `slot.target_count()` (`byroredux/src/render/skinned.rs:288-290`), so the
  `debug_assert`-only length check cannot be violated by a release build.
  Only the guard-nesting note above (SAFE-2026-08-27-03) is open.
- **Resumable exterior cancel**: `ExteriorCellApplyJob::cancel`
  (`cell_loader/exterior.rs:928-935`) releases the reference job's pending clip
  handles and then `unload_cell`s the root. Partially-spawned NPC entities are
  reachable from that unload because `stamp_cell_root_range` runs after **every**
  `load_references_budgeted` call including the `Pending` one
  (`exterior.rs:1699`), and it stamps the whole `first..world.next_entity_id()`
  id range. The "orphaned half-spawned actor" hypothesis was tested and
  **disproved**.
- `MaterialTable`'s dedup map is still cleared per frame; `AnimationClipRegistry`
  path-keyed dedup intact (below).

### Dimension 4 — Unsafe-block discipline
Mechanised sweep of every tracked `.rs` outside `target/` and the vendored
`tools/nifskope/`, excluding `unsafe fn` / `unsafe impl` / `unsafe extern`
declarations: **718 `unsafe {` blocks, 718 with a SAFETY comment** in the
-10/+6-line window (the wider window matters — this codebase frequently places
the comment *inside* the block, e.g. `vulkan/buffer.rs:1093`,
`vulkan/exposure.rs:74`). **Zero gaps.** Per `#2692`, no token-count gap was
chased.

### Dimension 5 — Vulkan spec compliance (static only)
No validation-layer or RenderDoc evidence was gathered — the preset forbids
launching the engine, and the user may have their own instance running. The
items below are code-provable; nothing here asserts a barrier / render-pass /
pipeline-state bug.
- **TLAS resize wait (#1390)**: `acceleration/tlas.rs:992` still calls
  `device_wait_idle()` before freeing the old allocation.
- **`initialize_layouts` coverage**: present on all seven storage-image passes
  (`taa`, `gbuffer`, `bloom`, `water_caustic`, `caustic`, `volumetrics`, `svgf`).
- **Volumetrics dispatch gate**: `VOLUMETRIC_OUTPUT_CONSUMED` is `true`
  (`volumetrics.rs:546`); the single call site gates on it by name
  (`context/post_passes.rs:498`).
- **Morph device addresses**: both buffers are created with
  `SHADER_DEVICE_ADDRESS` and queried via `get_buffer_device_address`
  (`morph_compute.rs:147-157`). `bufferDeviceAddress` is enabled only when
  `ray_query_supported` (`device.rs:743`) — but `triangle.vert:7` already
  `require`s `GL_EXT_buffer_reference`, so a device without the feature cannot
  create the main geometry pipeline regardless. Morph adds **no new** spec
  exposure; recorded rather than reported.
- **Needs runtime confirmation** (`BYRO_VALIDATION=1` or RenderDoc), explicitly
  out of reach for a static sweep: per-frame image-layout transitions across the
  new bloom/volumetric/caustic mip sets, and the visual severity of
  SAFE-2026-08-27-01 (which is code-provable as a data hazard but whose
  frame-count window can only be measured live).

### Dimension 6 — R1 material table
- `GpuMaterial` is **432 B**, pinned by `gpu_material_size_is_432_bytes`
  (`vulkan/material.rs:1494-1495`) — test name and asserted size agree.
- Rust ↔ GLSL diffed **field-for-field in declaration order**: 108 scalar fields
  on the Rust side, 108 on the GLSL side
  (`crates/renderer/shaders/include/bindings.glsl`), names matching one-to-one
  after snake→camel normalisation. 108 × 4 B = 432 B. Every field is a flat
  scalar — no `[f32; 3]` anywhere, so no std430 vec3 padding hazard. The newest
  blocks (BGEM glass optics, the soft/rim/back lighting response) are included
  in the match.
- `gpu_material_field_offsets_match_shader_contract` covers the new fields
  (`material.rs:1882-1894`).
- All **five** `GpuInstance` GLSL copies — `include/bindings.glsl`,
  `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp` — match the
  Rust struct's field order including the new `#3231` morph tail
  (`morphDeltaAddress` / `morphWeightAddress` / `morphTargetCount` + three
  reserved words, 160 B total, all `uint64_t` members 8-byte aligned).

### Dimension 7 — RT IOR refraction
- `MAX_REFRACT_PASSTHRUS = 8` is still the hard loop bound
  (`triangle.frag:1939`, loop at `:1982`), with the adaptive
  `refractPassthruBudget = 2 + qualityTier * 2` early-exit inside it
  (`:1784-1787`) — the tier form, not a fixed 2.
- The passthrough continuation is keyed on glass identity
  (`hitIsGlass || fallbackTexture`, `:2039-2040`), not texture equality.
- `GLASS_RAY_BUDGET = 2_097_152` in lockstep between
  `shader_constants_data.rs:282` and the generated
  `include/shader_constants.glsl:111`.
- Frisvad basis is the active path (`include/math_common.glsl:103-121`, used at
  `triangle.frag:1885`).
- `DBG_VIZ_GLASS_PASSTHRU = 0x80` — a scan of all 32 `DBG_*` bits found **no
  collisions**.

### Dimension 8 — NPC / animation spawn
- **`#772` FLT_MAX sentinel**: `FLT_MAX_SENTINEL = 3.0e38`
  (`crates/nif/src/anim/channel.rs:426`), applied at `:63`, `:155`, `:176`,
  `:211`, with the per-axis "no authored pose" tests intact
  (`anim/tests/transform.rs:242-275`, `:349-370`). Survived the `#2345`
  pre-10.1.0.106 `NiSequence` rework.
- **`#790` clip dedup**: `AnimationClipRegistry::get_or_insert_by_path` /
  `get_by_path` still ASCII-lowercase the key before lookup
  (`crates/core/src/animation/registry.rs:74-115`), and `release` still purges
  the reverse-map entry so a released slot rebuilds rather than returning an
  empty stub.
- **`MAX_TOTAL_BONES` overflow**: `SkinSlotPool::allocate` returns `None` past
  capacity with the one-shot `overflow_warned` log and
  `overflow_attempt_count` telemetry; excess entities fall back to bind pose
  rather than over-indexing (`skin_slot_pool.rs:162-186`).
- **Resumable `NpcSpawnJob` (safety facet only)**: cancel-time ownership traced
  (see Dimension 3); `FrameTimeBudget::unlimited()` provably never yields
  (`work_budget.rs:42-46`, pinned by `unlimited_budget_never_yields`), so the
  `unreachable!` in `spawn_npc_entity` (`npc_spawn.rs:1012`) cannot fire; the
  telemetry vectors on `ReferenceLoadJob` are capped (`npc_spawned_sample` at 8).
  No `unsafe`, no lifetime hazard, no unbounded growth found.

### Dimension 9 — NaN/Inf on the GPU
- `translate_material` still seeds the NaN sentinel and `resolve_pbr` is still
  the only detector/clamp (`material.rs:982-1007`); `static_meshes.rs`'s
  fallback still constructs finite defaults.
- Collision translate rejects non-finite shape params at every emission point
  (`crates/nif/src/import/collision/shape.rs:118`, `:188`, `:265`, `:341`,
  `:715`, `:773`).
- Emitter extraction gates every scalar on `is_finite` plus the `3.0e38`
  sentinel (`crates/nif/src/import/walk/mod.rs:720`, `:792`, `:801-819`,
  `:879`), and `apply_emitter_params` re-checks at the consumer
  (`byroredux/src/systems/particle.rs:407`).
- Only the `sanitize_finite` gap above is open.

### Dimension 10 — debug-ui / egui overlay
- `EguiPass` is destroyed **first** in `VulkanContext::drop`
  (`context/teardown.rs:182-184`), ahead of every allocator-dependent teardown
  and far ahead of device destroy.
- Texture free is still deferred one frame via `pending_free`
  (`egui_pass.rs:233-238`), leaning on `draw_frame`'s fence wait.
- The graphics-queue `Mutex` is locked only around `set_textures`
  (`egui_pass.rs:251-256`); tessellate and `cmd_draw` run with it released, so
  CONC-D1-01 / #1713 has not regressed.
- `DebugUiState` holds no Vulkan handle.

### Dimension 11 — Sandboxed mod runtime
Audited as a contract; still no engine consumer, which is not reported as a
finding.
- **Absence of WASI verified at the manifest**: the workspace pins
  `wasmtime = { default-features = false, features = ["anyhow",
  "component-model", "cranelift", "runtime", "std"] }` — no `wasi` feature, and
  `crates/mod-runtime/Cargo.toml` pulls in only `thiserror` + `wasmtime`.
- **Capability gating**: `logging::Host::log` checks
  `self.grants.contains(LOG_CAPABILITY)` and **bails** on a missing grant
  (`runtime.rs:265-270`) — an error to the guest, not a silent no-op.
- **Log DoS channel bounded three ways**: per-message byte cap, entry-count cap,
  and a `checked_add` total-byte cap (`runtime.rs:272-287`). #3050's *drain* is
  still absent and still OPEN.
- **Resource limits**: `SandboxConfig::validate` now rejects both degenerate
  floors and absurd ceilings, including the `MAX_WASM_STACK_BYTES_CEILING` that
  #3049 added to stop wasmtime aborting the host process
  (`limits.rs:95-160`).
- **Per-instance isolation**: no `static`, `lazy_static`, `OnceLock`, or shared
  `Arc<Mutex<…>>` anywhere in the crate — the only `'static` hits are
  `&'static str` error payloads.

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-08-27.md
```
