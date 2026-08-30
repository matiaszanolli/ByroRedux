# #3671 — PERF-D7-2026-08-30-02: the interior cell load still runs its whole REFR + NPC spawn on an unlimited budget, though the resumable cursor #1798 deferred on now exists

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D7-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,esm-plugin,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3671

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/references/mod.rs:227-267` (`load_references`, `FrameTimeBudget::unlimited()` at `:247`), called from `byroredux/src/cell_loader/load.rs:485` (`load_cell_with_masters`), reached from `byroredux/src/cell_loader/transition.rs:437-460` (`load_interior_cell`)
- **Status**: NEW — supersedes #1798 (CLOSED), which was closed by *measuring* the stall, not bounding it
- **Description**: Every exterior path now yields against a real deadline:
  `ExteriorCellApplyJob::advance` and `PersistentCellApplyJob::advance` both
  thread the `FrameTimeBudget` seeded from `STREAMING_APPLY_BUDGET`
  (`app_step.rs:33` = 16 ms, `:196-197`) into `load_references_budgeted`. The
  interior path calls the thin `load_references` wrapper, which constructs
  `FrameTimeBudget::unlimited()` and then asserts the job *cannot* yield
  (`unreachable!("an unlimited reference-load budget cannot yield")`,
  `references/mod.rs:266`). One door walk-in therefore spawns every `PlacedRef`,
  every SCOL/PKIN expansion and every NPC — each NPC being a multi-NIF
  `NpcSpawnJob` — inside a single frame, followed by the forced
  `flush_pending_cell_textures` fence wait.
- **Evidence**: `references/mod.rs:247`:
  ```rust
  let mut budget = FrameTimeBudget::unlimited();
  match load_references_budgeted(..., &mut budget) {
      ReferenceLoadProgress::Complete(result) => result,
      ReferenceLoadProgress::Pending(_) => {
          unreachable!("an unlimited reference-load budget cannot yield")
      }
  }
  ```
  #1798's closing comment is explicit that the fix shipped was measurement
  only: *"This is the minimal step the issue itself calls out — making the cost
  visible — rather than the larger chunked-spawn-budget rewrite … which needs
  real per-cell numbers to size correctly and is a substantially bigger change
  (a resumable cursor across frames)."* **That premise no longer holds.** The
  resumable cursor exists (`ReferenceLoadJob` with `next_ref` / `next_synth` /
  `current_ref_synth` / `active_npc`), it is the same function the interior
  already calls, and the per-frame allowance is already chosen and justified
  (`STREAMING_APPLY_BUDGET`). The remaining work is a caller-side loop plus a
  `stamp_cell_root_range` on each yield — the shape `ExteriorCellApplyJob`
  already implements.
- **Impact**: A multi-hundred-millisecond-to-multi-second freeze on every
  interior transition into a dense cell — door walk-in, `coc`-style debug load,
  and the M45.1 save-load cell reload — on a machine where nothing else in the
  streaming stack blocks a frame. It is also the one path where the shipped
  `npc_spawn_wall` number has no lever attached to it: the log tells the user
  how long the freeze was and nothing can act on it.
- **Related**: #1798 (CLOSED, measurement-only); #2275 (the identical gap for
  the worldspace persistent CELL, since fixed by `PersistentCellApplyJob` —
  the template for this fix); #881 (`flush_pending_uploads`, the fence wait
  that compounds it); #1698 (the *post*-load settle storm — adjacent, distinct).
- **Suggested Fix**: Drive `load_cell_with_masters`'s reference phase through
  `load_references_budgeted` behind an `InteriorCellApplyJob` (or reuse the
  `ExteriorCellApplyJob` shape), stepped from `App::step_cell_transition` under
  the same `STREAMING_APPLY_BUDGET` deadline, stamping `stamp_cell_root_range`
  on each yield so a cancelled transition stays reclaimable. Keep
  `load_references` as the synchronous wrapper for the remaining test /
  console callers.
> **Cross-reference**: `#3540` (Starfield `citycydoniamainlevel` never renders a frame — 10-minute single-threaded stall) is a *different* root cause (M28.5 static-collider AABB build in `byroredux/src/systems/character.rs` and BLAS construction in `crates/renderer/src/vulkan/acceleration/blas_static.rs`), but it is reached through the same unbudgeted interior-load frame this finding describes.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
