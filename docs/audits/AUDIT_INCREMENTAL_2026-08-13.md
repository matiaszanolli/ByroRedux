# Incremental / Delta Audit — 2026-08-13

**Scope**: `--commits 10` → `git diff 16336a09..7900584b` (`HEAD~10..HEAD`, first-parent walk)
**Files changed**: 82 (3,910 insertions, 684 deletions)
**Method**: static only — see the *Build-verification caveat* below.
**Result**: **7 findings** — 0 CRITICAL, 0 HIGH, 4 MEDIUM, 3 LOW.

> **HEAD moved before the audit ran.** The invoking prompt named `92784756` as
> HEAD; by the time the diff was taken, `7900584b` (*Merge PR #2854 —
> feat/2738-navm-geometry*) had landed on top. The audited range therefore
> extends one merge past what was requested, and includes the new NVNM navmesh
> decoder.

> **Build-verification caveat.** `cargo check` could not be used as a
> verification tool: the working tree mutated *during* the audit. At start
> `git status` showed only `?? crates/plugin/examples/_tmp_nvnm_probe.rs`;
> mid-audit it showed `M byroredux/src/{interaction,main}.rs`,
> `M crates/ui/src/player.rs`, `?? byroredux/src/app_{events,frame}.rs`. The 82
> compile errors observed in `byroredux` come from that in-flight uncommitted
> work, **not** from the audited commit range, and are excluded. Every finding
> below is derived by reading the diff plus its minimal surrounding context, and
> each premise was re-checked against the current tree.

---

## 1. Change summary

### First-parent commits in range

| Merge | Payload | Theme |
|---|---|---|
| `7900584b` | `f8e59b4e` | **feat**: decode the Creation-Engine packed `NVNM` navmesh body (#2738) |
| `92784756` | `929d3d3a` | BGSM cycle-cache pollution, effect-shader glass provenance gate, `ScratchTelemetry` doc, unsampled role-lane deferral (#2701/#2710/#2711/#2712) |
| `06098a40` | `c41e87d8` | nif.xml falloff default, collision-authoring doc drift, DLC-wide NIF baselines (#2331/#2333/#2334) |
| `61e00b80` | `a6015730` | bloom FIF budget, skinned-BLAS failure suppression, skin-chain timing, dead `water.frag` items (#2801/#2802/#2803/#2804) |
| `88ff120a` | `a919fcd7` | shared BLAS scratch peak, baked-LOD predicate, `OFST` drop, per-cell `XCCM` climate (#2460/#2452/#2454/#2451) |
| `654e3be6` | — | docs: session 66 closeout |
| `53a398f1` | `76baad3b` | shader-pipeline doc errors, preset citation, anisotropic GGX guards (#2807/#2810/#2811) |
| `663b2a44` | `1f73242d` | real-merge BGSM flag tests, `GpuMaterial::ior` provenance, `MergeOutcome` (#2702/#2703/#2704/#2709) |
| `2ce30136` | `6f5beb1f` | translucency `mat.set` arms, FSR bloom doc, MSN Z-authorship, fault-injection predicate (#2823/#2824/#2826/#2825) |
| `c7e5f30a` | `96124f3c` | draw-sort key measurement, partition self-swap, `GpuBuffer` flush SAFETY, 6 `unsafe fn` docs (#2681/#2682/#2683/#2684) |

### Themes

1. A large audit-bug-bash sweep closing ~20 tracked issues, mostly renderer +
   BGSM/NIFAL + documentation.
2. One genuine new feature: the packed `NVNM` navmesh decoder (+451 LOC in
   `crates/plugin/src/esm/records/misc/world.rs`).
3. Renderer: shared BLAS-scratch peak union, skinned-BLAS failure suppression,
   `GpuBuffer` flush safety documentation, FSR3 fault-injection predicate.
4. NIFAL/material: BGSM cycle-safe template resolution, effect-shader glass
   provenance gate, new `MAT_FLAG_MSN_HAS_AUTHORED_Z` lane.
5. Test-corpus widening: the per-block and block-coverage NIF baselines now walk
   every vanilla DLC mesh archive; six TSVs regenerated.

### Scripting-domain relevance (`scripting-deep` preset)

**The recent diff does not touch the scripting domain.** No file under
`crates/scripting/`, `crates/pex/`, or `crates/papyrus/` is in the range, and
neither is the cell-loader script-attach path
(`byroredux/src/asset_provider/script.rs`, the VMAD/`.pex` attach sites). The
cell-loader files that *are* in the range (`references/import.rs`,
`spawn/mesh_instance.rs`, `lod_support.rs`, `exterior.rs`, `partial.rs`,
`precombined.rs`, `terrain_lod.rs`) are all material/geometry/LOD paths. No file
was weighted for `/audit-scripting`, and nothing in this report is a scripting
finding.

---

## 2. Routing map

| Changed path(s) | Audited under | Risk floor |
|---|---|---|
| `crates/renderer/src/vulkan/acceleration/{blas_skinned,memory,predicates,tests}.rs` | `/audit-renderer`, `/audit-safety` | HIGH |
| `crates/renderer/src/vulkan/context/{mod,screenshot,skinned_blas_refit}.rs` | `/audit-renderer`, `/audit-concurrency` | HIGH |
| `crates/renderer/src/vulkan/{buffer,gbuffer,material,frame_upscaler,skin_compute}.rs` | `/audit-renderer`, `/audit-safety`, `/audit-nifal` | HIGH |
| `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` | `/audit-renderer` + GPU-struct lockstep | HIGH |
| `crates/renderer/shaders/{triangle.frag,water.frag,volumetrics_inject.comp,include/*}` + 2 `.spv` | `/audit-renderer` + GPU-struct-sync rule | HIGH |
| `crates/renderer/{build.rs,src/shader_constants.rs,src/shader_constants_data.rs}` | `/audit-renderer` (generated-GLSL lockstep) | HIGH |
| `crates/fsr3-sys/src/lib.rs` | `/audit-renderer` D23, `/audit-safety` | HIGH |
| `crates/nif/src/blocks/shader.rs`, `blocks/{shader_tests/legacy,dispatch_tests/havok}.rs` | `/audit-nif`, `/audit-fo3` | HIGH |
| `crates/nif/src/import/collision/mod.rs` | `/audit-nifal` (collision chain), `/audit-nif` | HIGH |
| `crates/nif/tests/**` + 6 baseline TSVs | `/audit-regression` | LOW |
| `crates/plugin/src/esm/records/misc/world.rs` | `/audit-skyrim`, `/audit-fo4`, `/audit-legacy-compat` | MEDIUM |
| `crates/plugin/src/esm/cell/{mod,wrld,tests/wrld}.rs` | per-game, `/audit-legacy-compat` | MEDIUM |
| `crates/plugin/examples/dump_navmesh.rs` | `/audit-tech-debt` | LOW |
| `crates/bgsm/src/template.rs` | `/audit-fo4`, `/audit-nifal` | MEDIUM |
| `byroredux/src/asset_provider/{material,tests/*}.rs` | `/audit-fo4`, `/audit-nifal`, `/audit-starfield` | MEDIUM |
| `byroredux/src/material_translate.rs` | **`/audit-nifal` (single boundary)** | HIGH |
| `byroredux/src/helpers.rs` | `/audit-nifal` | HIGH |
| `byroredux/src/scene/nif_loader.rs` | `/audit-nifal` (second material path), per-game | HIGH |
| `byroredux/src/{env_translate.rs,scene/world_setup.rs,scene.rs,streaming.rs,app_step.rs}` | EXAL mirror → `/audit-nifal`, `/audit-ecs` | MEDIUM |
| `byroredux/src/cell_loader/{exterior,lod_support,partial,precombined,terrain_lod,references/import,spawn/mesh_instance}.rs` | per-game `/audit-<game>` | MEDIUM |
| `byroredux/src/render/{mod,draw_sort_key_tests}.rs` | `/audit-performance`, `/audit-renderer` | MEDIUM |
| `byroredux/src/commands/{assets,scene}.rs` | `/audit-ecs` | MEDIUM |
| `crates/core/src/ecs/resources/mod.rs` | `/audit-ecs` | HIGH |
| `docs/**`, `HISTORY.md`, `ROADMAP.md`, `.claude/**` | `/audit-tech-debt` (doc rot) | LOW |

---

## 3. Findings

### INC-2026-08-13-01: Cycle-truncated BGSM chains are never cached — and vanilla FO4's `defaulttemplate_wet.bgsm` puts a broad subtree on that path

- **Severity**: MEDIUM
- **Dimension**: FO4 / NIFAL — external material resolution (performance)
- **Location**: `crates/bgsm/src/template.rs:204-268`; consumed via `byroredux/src/asset_provider/material.rs:504-543`
- **Changed in**: `crates/bgsm/src/template.rs` (commit `929d3d3a`, #2701)
- **Status**: NEW
- **Description**: #2701 correctly stopped `TemplateCache` from caching a chain
  that cycle-detection truncated, and correctly propagates the `truncated` flag
  to **ancestors** so a cached parent can't leak one root's truncation point to
  another. The correctness argument is sound. What the change did not account for
  is that the truncation is not a hypothetical "broken authoring" case on FO4 —
  this repository's own code comment records that **vanilla FO4 ships a
  self-referential template**, and it is a *default* template, i.e. one that many
  shipped materials inherit from. Because the flag propagates upward, *every*
  material whose chain reaches it is excluded from the cache together with the
  template itself, so each one re-reads and re-parses its whole chain out of the
  BA2 (extract + inflate per node) on every `resolve_bgsm`.
- **Evidence**:

  `crates/bgsm/src/template.rs` — the propagate-and-skip:
  ```rust
  let (parent, parent_truncated) = result?;
  truncated |= parent_truncated;
  ...
  if !truncated {
      self.insert(key, Arc::clone(&resolved));
  }
  ```

  `byroredux/src/asset_provider/material.rs:546` — the vanilla cycle, in-tree:
  ```
  // #FO4-D6-NEW — vanilla FO4 ships
  // `materials\template\defaulttemplate_wet.bgsm` with a
  // `root_material_path` field that self-references its own archive path.
  ```
  Corroborated by closed issue **#1148** ("FO4 BGSM template-cycle recovery:
  vanilla defaulttemplate_wet.bgsm self-references").

  Trace for `X.bgsm → defaulttemplate_wet.bgsm`: `resolve_depth(X)` misses →
  reads+parses X → `resolve_depth(wet)` misses → reads+parses wet →
  `parent_key == key` → `truncated = true`, not inserted → back in X,
  `truncated |= true` → **X not inserted either**. Both re-parse on the next call.

  `MaterialProvider::resolve_bgsm` has no second-level memo — `bgsm_cache` *is*
  the `TemplateCache`, and `failed_paths` only caches failures. The cost is paid
  once per imported mesh at `merge_external_material`
  (`cell_loader/references/import.rs:117`, `partial.rs:117`,
  `precombined.rs:304`, `scene/nif_loader.rs:275`).
- **Impact**: Per-mesh archive extraction + zlib/LZ4 inflate + BGSM parse during
  FO4 cell load and precombine spawn, for the whole descendant set of the wet
  template. Cell-load latency and streaming-budget pressure only; no correctness
  or rendering effect. Blast radius is FO4/FO76-era external materials; other
  games are unaffected (clean chains still cache, pinned by
  `cyclic_chains_reparse_while_clean_chains_still_cache`).
- **Related**: #2701 (closed), #1148 (closed), `feedback_no_guessing`
- **Suggested Fix**: Cache the truncated chain under a *walk-scoped* key rather
  than not at all — e.g. key the entry on `(path, truncation_anchor)`, or memoise
  the raw parsed `BgsmFile` per path separately from the resolved chain so the
  archive read and parse are paid once even when the chain assembly is not
  cacheable. The latter is the smaller change and preserves #2701's invariant
  exactly.

---

### INC-2026-08-13-02: `apply_cell_climate_override` runs every frame outside the grid-changed guard, so both of its `warn!` paths log at frame rate

- **Severity**: MEDIUM
- **Dimension**: EXAL / exterior streaming
- **Location**: `byroredux/src/app_step.rs:59-74`; `byroredux/src/env_translate.rs:330-338`; `byroredux/src/scene/world_setup.rs:414-432`
- **Changed in**: `byroredux/src/app_step.rs`, `env_translate.rs`, `scene/world_setup.rs` (commit `a919fcd7`, #2451)
- **Status**: NEW
- **Description**: The new per-cell `XCCM` climate hook is deliberately called on
  **every** `step_streaming` tick, not only on a cell-boundary crossing (the
  in-code justification — a session starting inside an override cell — is valid).
  The steady-state early-out (`if effective == *applied_climate { return false; }`)
  keeps that cheap. But **two `log::warn!` sites sit on the path *before* or
  *instead of* that early-out**, so in their failure modes they fire once per
  frame for as long as the player stands in the offending cell.
- **Evidence**:

  `app_step.rs` — unguarded, above `let grid_changed = …`:
  ```rust
  crate::scene::apply_cell_climate_override(
      &mut self.world, ctx, &state.tex_provider, &state.wctx,
      player_grid, &mut state.applied_climate_form,
  );
  let grid_changed = state.last_player_grid != Some(player_grid);
  ```

  Warn #1 — inside `resolve_cell_climate`, which runs *before* the equality
  check, so it fires even in perfect steady state:
  ```rust
  log::warn!(
      "resolve_cell_climate: cell XCCM climate {override_fid:08X} is not among parsed CLMT \
       records — falling back to the worldspace climate {worldspace_climate:08X?}",
  );
  ```
  Reachable whenever a CELL authors `XCCM` (parsed at
  `crates/plugin/src/esm/cell/{walkers.rs:322,wrld.rs:385}`) pointing at a
  FormID absent from `EsmIndex.climates` — a missing master or a stale mod edit.

  Warn #2 — in `apply_cell_climate_override`, on the "climate resolves, weather
  doesn't" branch, which returns `false` **without** updating
  `applied_climate`, by design:
  ```rust
  log::warn!(
      "Cell ({},{}) climate override {:08X?} resolves no default weather — keeping the \
       current sky", …);
  return false;
  ```
  Because `applied_climate` is deliberately left stale so the case is
  re-evaluated, and the caller re-evaluates every frame, this is an unconditional
  per-frame warn.
- **Impact**: 60–144 warn lines per second to the log sink, plus the `format!`
  allocation each time, for as long as the player stands in an affected exterior
  cell. Drowns every other diagnostic — the same failure shape as closed #2394
  and #900. No rendering effect; the fallback behaviour itself is correct.
- **Related**: #2451 (closed), #2394 (closed, per-frame retry + unbounded
  `log::` spam), #900 (closed, per-frame WARN spam)
- **Suggested Fix**: Move both warns behind a de-dup gate keyed on the cell grid
  (or on `(player_grid, effective)`), so each distinct broken cell warns once. A
  `HashSet<(i32,i32)>` on `WorldStreamingState` next to `applied_climate_form` is
  enough; the `resolve_cell_climate` warn should be lifted out of the pure
  function into the caller so the pure decision stays side-effect-free.

---

### INC-2026-08-13-03: New `failed_skin_blas` set is not pruned on cell unload, unlike the `failed_skin_slots` sibling its doc claims parity with

- **Severity**: MEDIUM
- **Dimension**: Renderer — skinned BLAS lifecycle / host-side cache hygiene
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1314-1333` (declaration); `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:508,731`; gap at `byroredux/src/cell_loader/unload.rs:204-209`
- **Changed in**: `crates/renderer/src/vulkan/context/{mod,skinned_blas_refit}.rs` (commit `a6015730`, #2802)
- **Status**: NEW — same defect class as **closed #1004**, which the new sibling
  set does not inherit the fix for
- **Description**: #2802 adds `failed_skin_blas` as the "BLAS sibling of
  `failed_skin_slots`" and documents *"Drop contract: same as
  `failed_skin_slots`"*. That parity does not hold. `failed_skin_slots` is pruned
  on cell unload by `queue_skin_unload_victims` — a fix landed specifically under
  #1003/#1004 because the LRU-eviction clear alone was not enough (a cell unload
  with no subsequent render tick retained entries forever). The new set has no
  such prune: `unload.rs` still passes only `&mut ctx.failed_skin_slots`.
- **Evidence**:
  ```rust
  // byroredux/src/cell_loader/unload.rs:204
  queue_skin_unload_victims(
      &victims,
      |eid| ctx.skin_slots.contains_key(&eid),
      &mut ctx.pending_skin_unload_victims,
      &mut ctx.failed_skin_slots,          // ← no failed_skin_blas counterpart
  );
  ```
  ```rust
  // queue_skin_unload_victims, unload.rs:511
  let victim_set: HashSet<EntityId> = victims.iter().copied().collect();
  failed.retain(|eid| !victim_set.contains(eid));
  ```
  Entity IDs are not normally recycled (`World::spawn` monotonically increments
  `next_entity`, `crates/core/src/ecs/world.rs:85-88`, doc: *"Entity IDs are NOT
  reclaimed"*) — **except** on save load, where `crates/save/src/driver.rs:123`
  calls `world.set_next_entity(snapshot.next_entity)` and can rewind it. The
  M45.1 live load-apply reloads the cell, which prunes `failed_skin_slots` for
  exactly the ids about to be reissued, and leaves `failed_skin_blas` holding
  them.
- **Impact**: Two effects. (a) Unbounded host-side growth of the set across cell
  crossings — small (one `u32` per entity that ever failed a BLAS build), but it
  is precisely what #1004 was filed for. (b) After a save load that rewinds
  `next_entity`, a freshly-spawned entity can inherit a stale suppression and
  lose skinned ray-traced shadows/reflections for the rest of the session, since
  the only clear is an LRU eviction. Reachability of (b) is narrow — it needs a
  prior BLAS-build failure under VRAM pressure — but the failure is silent and
  sticky when it happens.
- **Related**: #1004 (closed — the identical finding for `failed_skin_slots`),
  #1003 (closed), #2802 (closed)
- **Suggested Fix**: Widen `queue_skin_unload_victims` to take both sets (or a
  `&mut [&mut HashSet<EntityId>]`) and `retain` over each, and extend the
  existing `queue_skin_unload_victims` unit tests to cover the new set — the
  helper was explicitly extracted so this transformation is testable without a
  Vulkan device.

---

### INC-2026-08-13-04: `shrink_blas_scratch_to_fit`'s realloc-failure arm reproduces the exact "scratch absent → every skinned refit fails forever" hole #2460 just closed for the `peak == 0` arm

- **Severity**: MEDIUM
- **Dimension**: Renderer — acceleration-structure memory
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:132-137`
- **Changed in**: `crates/renderer/src/vulkan/acceleration/memory.rs` (commit `a919fcd7`, #2460) — the arm itself is untouched, but the fix's own reasoning applies to it verbatim
- **Status**: NEW
- **Description**: #2460 fixed the shrink peak walk to union the static and
  skinned BLAS maps, and documented precisely why: with a static-only walk the
  `peak == 0` arm dropped the shared scratch while skinned entities were still
  resident, *"and every one of their refits then failed the `blas_scratch_buffer
  absent` context until a first-sight rebuild."* Two blocks below, the
  realloc-failure arm leaves `blas_scratch_buffer` at `None` with the identical
  consequence, and the comment there still asserts *"This is a degraded but
  correct state."* It is not self-healing: the first-sight retry is gated on
  `needs_blas = accel.skinned_blas_entry(entity_id).is_none()`, which is `false`
  because the entry survives; and the refit-chain rebuild
  (`should_rebuild_skinned_blas`) never fires because `refit_count` is
  incremented at `blas_skinned.rs:618`, *after* the scratch lookup that returns
  `Err` at `:513`.
- **Evidence**:
  ```rust
  // memory.rs:132 — the buffer was already take()n above
  Err(e) => {
      log::warn!("BLAS scratch shrink realloc failed: {e}; next build will re-allocate");
  }
  ```
  ```rust
  // blas_skinned.rs:513 — early Err, before any counter moves
  let scratch_buffer = self.blas_scratch_buffer.as_ref().context(
      "blas_scratch_buffer absent — must be allocated by build_skinned_blas_batched_on_cmd first",
  )?;
  ```
  ```rust
  // skinned_blas_refit.rs:253
  let needs_blas = accel.skinned_blas_entry(entity_id).is_none();   // false → no rebuild
  ```
  #2802's new refit guard does not cover it either: `if !accel.has_skinned_blas(entity_id) { continue; }`
  passes, because the entry exists — only the *scratch* is gone.
- **Impact**: On a scratch realloc failure at a cell-unload or resize boundary,
  every resident skinned NPC's BLAS refit fails permanently: one WARN per entity
  per frame, and stale (bind-pose / last-successful-pose) skinned geometry in the
  TLAS, so RT shadows and reflections of NPCs freeze while raster stays correct.
  Requires an allocation failure to trigger — rare, but the state it leaves is
  unrecoverable without a cell reload.
- **Related**: #2460 (closed), #2802 (closed), #1782 (closed), #2774 (open, the
  TLAS analogue of a suspect shrink arm)
- **Suggested Fix**: On realloc failure, keep the *old* buffer instead of
  retiring it — the shrink is an optimisation, and the pre-shrink buffer is by
  construction large enough. Failing that, drop every `skinned_blas` entry (and
  the static entries) so `needs_blas` flips true and the first-sight path
  rebuilds against a freshly allocated scratch.

---

### INC-2026-08-13-05: `material_flag::MSN_HAS_AUTHORED_Z`'s doc cites a `texture_registry.rs::classify_msn_z_source` that does not exist

- **Severity**: LOW
- **Dimension**: Tech debt — doc/symbol drift at the NIFAL boundary
- **Location**: `crates/renderer/src/vulkan/material.rs:594-599`
- **Changed in**: `crates/renderer/src/vulkan/material.rs` (commit `6f5beb1f`, #2826)
- **Status**: NEW
- **Description**: The new flag's rationale block tells the reader the bit is set
  in `texture_registry.rs` by a function called `classify_msn_z_source`. Neither
  exists: `git grep classify_msn_z_source` returns only this doc comment. The
  real mechanism is `MaterialTextureHandles::normal_has_alpha` (itself
  `dds::format_has_alpha` on the bound normal map's Vulkan format), consumed by
  `byroredux::material_translate::{msn_has_authored_z, resolve_msn_z_source}`.
- **Evidence**:
  ```
  /// content heuristic — this bit is set in `texture_registry.rs`
  /// (`classify_msn_z_source`) and folded into `Material.effect_shader_flags`
  ```
  vs. the only definition site, `byroredux/src/material_translate.rs:375`:
  ```rust
  fn msn_has_authored_z(model_space_normals: bool, normal_has_alpha: bool) -> bool {
      model_space_normals && normal_has_alpha
  }
  ```
  This is exactly the backticked-symbol class the project's own
  `_audit-validate.sh` advisory exists to catch (`_audit-common.md`
  § Path-Reference Convention).
- **Impact**: Documentation only. A reader chasing the MSN Z-source decision is
  sent to the wrong file, and the reuse of `normal_has_alpha` for *two* distinct
  semantics (spec-mask provenance and MSN Z-authorship) is exactly the coupling a
  future maintainer needs pointed at accurately.
- **Related**: #2826 (closed); `_audit-common.md` § Path-Reference Convention
- **Suggested Fix**: Replace the citation with
  `byroredux::material_translate::resolve_msn_z_source` /
  `msn_has_authored_z`, and name `MaterialTextureHandles::normal_has_alpha` +
  `dds::format_has_alpha` as the actual signal — the `resolve_msn_z_source`
  docstring already states this correctly, so copy from there.

---

### INC-2026-08-13-06: #2332's fix landed with a pinning test and a doc citation, but the commit's `Fix` keywords omit it — the issue is still OPEN

- **Severity**: LOW
- **Dimension**: Tech debt — tracker/code sync
- **Location**: `crates/nif/src/blocks/dispatch_tests/havok.rs:169-213`; `crates/nif/src/import/collision/mod.rs:12`
- **Changed in**: commit `c41e87d8`, branch `fix/2331-2332-2333-2334`
- **Status**: Existing: **#2332** (open in tracker; work appears landed)
- **Description**: The branch name names four issues; the commit subject is
  `Fix #2331 Fix #2333 Fix #2334` — #2332 is absent. The range nonetheless
  contains #2332's deliverables: a new regression test
  `bhk_sp_collision_object_classifies_as_phantom_authoring` asserting
  `examine_collision_kind` returns `CollisionAuthoring::Phantom`, and a module
  docstring row that cites "(#2332)". Verified against the current code:
  `classify_collision_block` (`import/collision/mod.rs:111`) matches on the exact
  `Any` type, so a `bhkSPCollisionObject` dispatched into `BhkPCollisionObject`
  already classifies as `Phantom` — i.e. the issue's premise ("classified
  `CollisionAuthoring::Classic`") is now stale and the new test pins that.
- **Evidence**: `git log --oneline HEAD~10..HEAD` → `c41e87d8 Fix #2331 Fix #2333
  Fix #2334: …`; `/tmp/audit/issues.json` → `2332 OPEN FO3-D5-02:
  bhkSPCollisionObject classified CollisionAuthoring::Classic although it's a
  phantom wrapper`.
- **Impact**: Tracker drift. #2332 will be re-triaged, re-investigated, and its
  stale premise re-discovered — the exact cost the `feedback_audit_findings`
  memory records. (The sibling `#2738` is in the same shape: `f8e59b4e` is a
  `feat(esm)` commit referencing it with no closing keyword, but there the
  consumer half of EX-16b is genuinely still outstanding, so leaving it open is
  defensible.)
- **Related**: #2332 (open), #2333/#2334 (closed), `feedback_multi_issue_commit_close`
- **Suggested Fix**: Close #2332 manually with a pointer to
  `bhk_sp_collision_object_classifies_as_phantom_authoring`, noting the premise
  was already stale at fix time.

---

### INC-2026-08-13-07: `open_all_mesh_archives` is all-or-nothing, so the FO3/FNV/Skyrim NIF baseline gates now go fully dark on any install missing one DLC archive

- **Severity**: LOW
- **Dimension**: Regression-gate coverage
- **Location**: `crates/nif/tests/common/mod.rs:295-336`; consumers `crates/nif/tests/per_block_baselines.rs:101` and `block_coverage_baselines.rs:195`
- **Changed in**: `crates/nif/tests/common/mod.rs` (commit `c41e87d8`, #2334)
- **Status**: NEW
- **Description**: #2334 widened the baseline corpus to every vanilla mesh
  archive — the right call, and the DLC-only collision types it unblocked are
  real. The all-or-nothing rule is also correct in isolation: a partial corpus
  would report absent DLC as `PARSED shrank`. The unintended consequence is that
  the gate's *floor* dropped. Previously a base-game FNV install still gated
  492,796 blocks from `Fallout - Meshes.bsa`; now the harness requires all
  **eleven** listed FNV archives — including the four pre-order-pack `.bsa`s
  which ship only with the Ultimate Edition — and on any shortfall returns
  `None`, skipping the whole game with only an `eprintln!`.
- **Evidence**:
  ```rust
  for name in game.mesh_archives() {
      let path = data.join(name);
      if !path.is_file() {
          eprintln!("[{}] skipping: {:?} not found — the baseline corpus is all {} archive(s) or nothing (#2334)", …);
          return None;                       // ← whole game skipped
      }
      …
  }
  ```
  FNV list: `Fallout - Meshes.bsa`, `Update.bsa`, `DeadMoney`, `HonestHearts`,
  `OldWorldBlues`, `LonesomeRoad`, `GunRunnersArsenal`, `CaravanPack`,
  `ClassicPack`, `MercenaryPack`, `TribalPack`. FO3 requires all five GOTY DLC;
  Skyrim SE requires both `Skyrim - Meshes0/1.bsa`. Oblivion was deliberately
  left single-archive for exactly this reason — the reasoning is in the code but
  was not applied to FO3/FNV.
- **Impact**: A contributor on a non-GOTY FO3 or non-Ultimate FNV install loses
  the strongest per-block NIF regression gate entirely, silently, where it
  previously ran at partial-but-useful coverage. Affects developer machines only
  (these harnesses already skip on hosts with no game data, so CI is unchanged).
  The regenerated TSVs are otherwise clean: `unknown_blocks` stays 0 and totals
  rose consistently (FO3 287,331→526,109, FNV 492,796→662,102, SkyrimSE
  665,846→758,733).
- **Related**: #2334 (closed), #1883 (closed — the false-green blind spot this
  continues)
- **Suggested Fix**: Fall back to the primary archive with a loud
  `eprintln!("degraded corpus")` and compare against a *second*, primary-only
  baseline TSV, so a partial install still gates something. Alternatively, split
  the baselines per archive so a missing DLC skips only its own rows.

---

## 4. Verified clean (checked, no finding)

Recorded so a later sweep does not re-derive them:

- **GPU-struct lockstep** — `GpuMaterial` (Rust) vs `include/bindings.glsl`: this
  range changed only doc comments and trailing `// unsampled` markers. Field
  order, types and the 300→344 offsets are byte-identical; the 348 B total is
  unchanged. `MAT_FLAG_MSN_HAS_AUTHORED_Z` takes bit 12, the only free bit below
  the bits-16-23 `EFFECT_LI` byte-field (bit 10 = `BGSM_AUTHORED`, Rust-only by
  design; bit 11 = `THIN_GLASS`), and is mirrored in all four required places
  (`shader_constants_data.rs`, `build.rs`, generated `shader_constants.glsl`,
  plus the `shader_constants.rs` equality assertion).
- **SPIR-V freshness** — `water.frag` and `triangle.frag` were recompiled with
  `glslangValidator -V` and byte-compared against the checked-in `.spv`:
  **both match exactly**. `volumetrics_inject.comp`, `pbr.glsl` and
  `bindings.glsl` changes were comment-only, so no recompile was owed and none
  was made.
- **NIFAL two-path parity (#2826)** — `resolve_msn_z_source` is wired into
  **both** material load paths, immediately after `MaterialTextureHandles` is
  inserted and beside the existing `resolve_normal_alpha_spec_roughness`:
  `cell_loader/spawn/mesh_instance.rs:557` (REFR spawn) and
  `scene/nif_loader.rs:995` (loose NIF). No divergence — this is the classic
  NIFAL leak and it did not happen.
- **Contract break — `classify_glass_into_material` gained a 7th parameter** —
  exactly one production call site (`material_translate.rs:225`), updated; all 13
  test call sites updated. `from_bgsm` is set only by the BGSM
  (`asset_provider/material.rs:979`) and BGEM (`:1319`) arms and explicitly *not*
  by the Starfield `.mat` arm (`:881`), so the Skyrim inline-effect case
  (`InnerHaze` sharing `plainglasstile01.dds`) correctly stays `false`.
- **Contract break — `merge_external_material` `bool` → `MergeOutcome`** — all
  four production call sites use `let _ =`, satisfying the new `#[must_use]`;
  test sites use `.merged()`.
- **Contract break — `WorldspaceRecord::cell_offsets` removed (#2454)** — zero
  remaining references workspace-wide.
- **Contract break — `TemplateCache::resolve_depth` → `(Arc, bool)`** — sole
  caller `resolve` updated.
- **NVNM decoder parse safety (#2738)** — `NvnmCursor::counted` uses
  `checked_mul` + `checked_add` + slice `get`, so a 4-billion count yields `None`
  before any allocation; `NVNM_MAX_DIVISOR = 64` caps the segment walk at 4,096
  iterations; acceptance is gated on `cur.pos == data.len()`, rejecting both
  short and trailing-byte bodies; the header/body split keeps FO4 tiles locatable
  while retaining their blob. No parse-safety or unbounded-work defect found.
  Note the decoded `EsmIndex.navmeshes` still has no engine-side consumer — that
  is #2738/#2372's own remaining scope, not a regression.
- **`unsafe` delta** — no new `unsafe` blocks. Six `unsafe fn` *gained*
  `# Safety` sections (#2684). `debug_assert_flush_range_bounded` is debug-only,
  `flush_range_within` is pure and unit-tested, and the rewritten `// SAFETY:`
  comments in `buffer.rs` correct a previously false containment claim — a
  documentation fix, not a behaviour change.
- **Lock / query delta** — no new multi-component queries and no changed RwLock
  scope. `resolve_msn_z_source` uses sequential single-component
  `world.get`/`get_mut`, the same shape as its sibling.
- **Drop ordering** — `failed_skin_blas` holds `EntityId` (`u32`) only, no Vulkan
  handles; `VulkanContext::Drop` is unaffected (see finding 03 for its *other*
  lifecycle gap).
- **`bhkSPCollisionObject` classification** — `classify_collision_block` matches
  on the exact `Any` type, so the alias landing in `BhkPCollisionObject` already
  yields `Phantom`; the new test pins it (see finding 06 for the tracker gap).
- **Baseline TSV regen honesty** — `unknown_blocks` remains 0 on all three
  regenerated coverage baselines; no type's `unknown` was raised to absorb a
  regression.

---

## 5. Missing tests

Changed code paths with no corresponding test update:

| Path | Gap |
|---|---|
| `byroredux/src/scene/world_setup.rs::apply_cell_climate_override` | The pure halves (`resolve_cell_climate`, `resolve_default_weather`) are well tested; the **wiring** is not. No test covers the per-frame invocation, the `applied_climate_form` bootstrap seeding in `WorldStreamingState::new`, the "climate resolves / weather doesn't" early-return, or the warn paths. Finding 02 lives entirely in the untested half. |
| `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` / `byroredux/src/cell_loader/unload.rs` | `queue_skin_unload_victims`'s unit tests still cover only `failed_skin_slots`. No test asserts `failed_skin_blas` is pruned on unload — which is why finding 03 is invisible to the suite. |
| `crates/renderer/src/vulkan/acceleration/memory.rs` | `shared_blas_scratch_peak` and `scratch_should_shrink` are pinned at the predicate level, but no test covers `shrink_blas_scratch_to_fit`'s **realloc-failure** arm (finding 04) or the `target = peak + scratch_alignment_padding` sizing. |
| `crates/bgsm/src/template.rs` | `cyclic_chains_reparse_while_clean_chains_still_cache` pins that cyclic chains re-parse, but nothing pins the **cost model** — e.g. a resolver call-count assertion showing how many archive reads a wet-template descendant costs per resolve (finding 01). |
| `byroredux/src/material_translate.rs::resolve_msn_z_source` | `msn_has_authored_z` (the pure predicate) is tested; the ECS-level resolver — component absent, `MaterialTextureHandles` missing, idempotency across a re-run — is not. |
| `crates/plugin/src/esm/records/misc/world.rs` | Good synthetic coverage for the packed form. Not covered: a `NAVM` carrying **both** typed sub-records and an `NVNM` (`decode_nvnm` would overwrite the typed geometry), and `NVNM` header decode when `worldspace != 0` but the body is FO4-shaped *and* the grid words are the last bytes present. |

---

## 6. Next step

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-08-13.md
```
