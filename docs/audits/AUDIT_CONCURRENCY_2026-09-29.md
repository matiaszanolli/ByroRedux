**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (@ `9e6f08870`, 20 commits ago) · **Audited**: Dims 1–7 (every dimension's `Paths:` had commits: D1 2, D2 4, D3 4, D4 3, D5 1, D6 1, D7 3 — mostly the #4986–#5000 fix wave plus the dialogue / gear-import / player-animation features) · **Unchanged since baseline (skimmed)**: none at the dimension level. Sub-paths with zero commits, guard spot-checked only: `acceleration/`, `context/{init,draw,sync_and_acquire_frame,resize,teardown,skinned_blas_refit}.rs`, `crates/core/src/ecs/{scheduler,access,world,lock_tracker}.rs`, `crates/physics/src/{world,components,config}.rs`, `cell_loader/unload.rs`, `systems/character.rs`, `ragdoll.rs`, `extensions/`, debug-server, `crates/ui`, `crates/audio`.

# Concurrency & Synchronization Audit — 2026-09-29

**Command**: `/audit-concurrency` (all dimensions, depth `deep`), run inside `/audit-suite --preset comprehensive`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Delta scope.** `git log 9e6f08870..HEAD -- <Paths>` per dimension. All seven dimensions were analysed synchronously
  in this session (no sub-agents); per-dimension notes are in `/tmp/audit/concurrency/dim_{1..7}.md` and this report
  was reconciled against each of them.
- **No engine launch, no GPU process** (suite constraint). Vulkan evidence comes from the CI `vulkan-validation` job
  log, which for the first time since #4596 actually reached a device (see Headline).
- **Dedup.** Open issues (`/tmp/audit/issues.json`, 163), closed-issue searches per finding, today's sibling reports
  `AUDIT_ECS_2026-09-29.md` and `AUDIT_RENDERER_2026-09-29.md`, and the 09-28 concurrency report.

### Guard runs at HEAD (local, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --no-fail-fast` | 2517 passed, **5 failed** — the five `systems::walk_anim::tests::*`, each a lock-order-cycle panic at `lock_tracker.rs:476`. Same as ECS-2026-09-29-D1-01 (cycle B). |
| `cargo test -p byroredux -- scheduler_access system_access_declaration` | 29 passed, 0 ignored |
| `cargo test -p byroredux-physics sync` | 31 passed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics` | 192 passed |
| `cargo test -p byroredux-renderer --lib -- frames_in_flight one_time_lock_scope partial_delta_promotion skin_publish_barrier taa_resolves render_finished rgba_updates only_the_submitted gpu_timers dependency_chain` | 33 passed |
| `cargo test -p byroredux-bsa --lib -- concurrent threaded parallel` | 3 passed (BSA / BA2 / CSG threaded extract, #5000) |
| CI run 36609043348 (`9fcfdc3fc`) | "ABBA lock-order detector": **failure** (the ECS D1-01 cycles). "Vulkan validation layers (lavapipe)": **failure — but now on real validation output** (below). |

## Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 2 | 0 | 0 |
| MEDIUM | 0 | 0 | 2 (#4780, #4987) |
| LOW | 3 | 0 | 1 (#4989) |

Cross-referenced, not re-filed (owned by today's ECS report): ECS-2026-09-29-D1-01 (HIGH, ABBA lane red from `ab31cfefe`),
ECS-2026-09-29-D1-02 (LOW, test-body closing edges), ECS-2026-09-29-D5-01 (LOW, `interaction_system` /
`npc_dialogue_selection` Access rows).

**Headline.**
- **The live validation lane works again, and it has something to say.** `6d05c2bc0`'s lavapipe ICD glob (#4987) got
  the `vulkan-validation` job to a device for the first time since #4596: CI job 109545444434 logs
  `lavapipe ICD: /usr/share/vulkan/icd.d/lvp_icd.json`, runs the 5-frame bench to `bench exit status: 0` with
  `rt_supported=1 tlas_build=1`, under `BYRO_LOCK_ORDER_CHECK: 1`. Two consequences:
  - **First dynamic evidence for the parallel batch**: the real rayon-dispatched schedule ran ≥7 frames against a real
    world under the detector with **no `panicked at` / `lock-order cycle`**. The static proof (9/9 declared, 0
    conflicts) now has a live confirmation, bounded by what a content-free demo scene exercises.
  - **Two validation error classes, every frame**, which are this report's two HIGH findings:
    `SYNC-HAZARD-READ-AFTER-WRITE` on the ReSTIR current-reservoir buffer (CONC-D2-2026-09-29-01) and
    `VUID-vkCmdDispatch-None-08114` on a compute set's bindings 9/10 (CONC-D2-2026-09-29-02, attributed to caustics).
    The same two appear on the previous main run (36574364470 @ `8b334c102`), so they predate today's feature commits.
- **#4987 status**: its premise ("never reached a device") no longer holds. The lane's remaining red is genuine
  validation output; #4987 can close once the two HIGHs are fixed and the lane runs clean.
- **The ABBA lane is red** on `ab31cfefe`'s dialogue nests — reproduced locally, owned by ECS-2026-09-29-D1-01. The same
  dialogue code has one more guard-lifetime slip that report does not name (CONC-D3-2026-09-29-01, LOW).
- **Fix wave verified**: #4986, #4988, #4990–#4999 and #5000 are in place and pinned; #4997's scan is not
  self-satisfying (production slice only).

| ID | Sev | Dim | Status | Title |
|---|---|---|---|---|
| CONC-D2-2026-09-29-01 | HIGH | D2 | NEW | ReSTIR reservoir clear publishes to FRAGMENT `SHADER_WRITE` only; sync validation reports READ_AFTER_WRITE at every draw |
| CONC-D2-2026-09-29-02 | HIGH | D2 | NEW | Caustic set bindings 9/10 are never written when there is no global geometry SSBO, but the dispatch still runs (`VUID-vkCmdDispatch-None-08114`) |
| CONC-D3-2026-09-29-01 | LOW | D3 | NEW | `npc_dialogue` shadows its `LoadedCellIndex` guard instead of dropping it, so the read guard spans the whole system including the write pass |
| CONC-D4-2026-09-29-01 | LOW | D4 | NEW | `equipment_appearance_system`'s Access row does not declare the mid-life gear-import surface `0182fc5e8` added |
| CONC-D6-2026-09-29-01 | LOW | D6 | NEW | A swapchain-format-change rebuild of `EguiPass` drops the egui font atlas, and egui never re-sends it |
| — | MEDIUM | D3 | Existing: #4987 | `vulkan-validation` lane — now reaches a device; red only on the two HIGHs above (status update) |
| — | MEDIUM | D2 | Existing: #4780 | #3685 skip-clear latch also skips the temporal reset (code unchanged) |
| — | LOW | D2 | Existing: #4989 | Narrowed palette dispatches write one SSBO back to back with no barrier (code unchanged) |

---

## Findings

### CONC-D2-2026-09-29-01: ReSTIR reservoir clear publishes to FRAGMENT `SHADER_WRITE` only; sync validation reports READ_AFTER_WRITE at every draw
- **Severity**: HIGH — "Vulkan validation layer errors in normal operation" (`_audit-severity.md` HIGH list). It fires on
  every frame of every scene. The underlying data hazard is probably nil (the shader only stores to this buffer, see
  below), so the practical harm is that the only live validation gate stays red and masks any real hazard behind it.
- **Dimension**: Compute → AS → Fragment Chains (a TRANSFER → FRAGMENT chain)
- **Location**: `crates/renderer/src/vulkan/restir.rs:131-169` (`ReservoirBuffers::begin_frame`, the `after`
  barrier at `:143-145` and its `cmd_pipeline_barrier` at `:160-168`); called from
  `crates/renderer/src/vulkan/context/begin_frame_recording.rs:99`. Shader side:
  `crates/renderer/shaders/include/bindings.glsl:571-573` (`ReservoirCurrBuffer`, set 1 binding 16, no `writeonly`),
  written at `crates/renderer/shaders/triangle.frag:3856`.
- **Status**: NEW. No issue or audit mentions it (searched "SYNC-HAZARD-READ-AFTER-WRITE", "reservoir barrier",
  "ReSTIR fill", "begin_frame reservoir"; #2152 is the older first-use-initialisation fix). The code arrived in
  `186234944` (2026-09-26, ReSTIR light identity) while the lane could not reach a device.
- **Verification Path**: validation layer — already captured.
- **Description**: `begin_frame` clears this frame's reservoir slot with `vkCmdFillBuffer`, then publishes the clear
  with `TRANSFER_WRITE → SHADER_WRITE`, `TRANSFER → FRAGMENT_SHADER`. The buffer is declared read-write in GLSL, and
  Synchronization Validation classifies the main pass's use of binding 16 as `FRAGMENT_SHADER_SHADER_STORAGE_READ`,
  which the barrier's dst access does not cover. The fill → store WAW ordering is correct as written; the fragment
  "read" is what the validation model sees for a non-`writeonly` storage block.
- **Evidence**: CI job 109545444434, 20 errors across both FIF slots (`VkBuffer 0x1b9…` / `0x1bb…`):
  ```
  [ SYNC-HAZARD-READ-AFTER-WRITE ] … vkCmdDrawIndexed(): Hazard READ_AFTER_WRITE for VkBuffer 0x1b900000001b9[] …
  type: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, binding #16 index 0. Access info (usage:
  SYNC_FRAGMENT_SHADER_SHADER_STORAGE_READ, prior_usage: SYNC_COPY_TRANSFER_WRITE, write_barriers:
  SYNC_FRAGMENT_SHADER_SHADER_STORAGE_WRITE|…, command: vkCmdFillBuffer, seq_no: 2, reset_no: 2).
  ```
  ```rust
  // restir.rs:143-145
  let after = [buffer_barrier(self.curr_buffer(frame))
      .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
      .dst_access_mask(vk::AccessFlags::SHADER_WRITE)];
  ```
- **Trigger Conditions**: every frame; any scene (the CI bench is the content-free demo).
- **Impact**: The `vulkan-validation` lane is permanently red, so a real hazard introduced later is invisible in it.
  If any fragment path ever loads from `reservoirsCurr`, for example a future in-frame spatial reuse, that load would
  be a real RAW with no memory dependency.
- **Related**: CONC-D2-2026-09-29-02 (the other red), #4987, #2152.
- **Suggested Fix**: Add `vk::AccessFlags::SHADER_READ` to the `after` barrier's dst access. Also declare the block
  `writeonly` if nothing reads it; that states the intent and lets the validator and driver see it. Confirm with the
  next lane run: binding #16 goes quiet.

### CONC-D2-2026-09-29-02: Caustic set bindings 9/10 are never written when there is no global geometry SSBO, but the dispatch still runs
- **Severity**: HIGH — a Vulkan spec violation (`VUID-vkCmdDispatch-None-08114`: a statically used descriptor that was
  never written) reported by the validation layer in normal operation. It is reachable only when the mesh registry has
  no global geometry buffers (the content-free demo scene, i.e. exactly what CI runs). Real cells always build them.
- **Dimension**: Compute → AS → Fragment Chains
- **Location**:
  - The conditional write: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:273-295`
    (`caustic.write_geometry_buffers` inside `if let (Some(vb), Some(ib)) = (global_vertex_buffer, global_index_buffer)`).
  - The layout: `crates/renderer/src/vulkan/caustic.rs:461-474` (bindings 9/10, no `PARTIALLY_BOUND`) and `:710-738`.
  - The dispatch gate, TLAS only: `crates/renderer/src/vulkan/context/post_passes.rs:444-467`.
- **Status**: NEW (searched "08114", "caustic descriptor binding 9"; no match).
- **Verification Path**: validation layer — VUID already captured. The *pipeline attribution* is by elimination and
  needs one confirmation: a local `BYRO_VALIDATION=1` run of the bare demo, or debug-utils object names on the sets.
- **Description**: The CI error names a two-set (per-FIF) compute descriptor set whose bindings 9 and 10 were never
  updated. Four compute layouts have set-0 bindings 9 and 10:
  - SVGF temporal writes both unconditionally at creation (`svgf.rs:839-840`).
  - Volumetrics inject writes both unconditionally at creation (`volumetrics/init.rs:668-669`).
  - Ground-cover models writes all 12 bindings immediately before a dispatch that is gated off without ground cover
    (`groundcover_models.rs:650-712`; the bench reports `groundcover-models: demanded=0`).
  - **Caustics** writes 9/10 only when both global geometry buffers exist.

  In the demo scene they do not exist:
  - `spawn_demo_primitives` uploads through `MeshRegistry::upload` (`crates/renderer/src/mesh.rs:552`), which never
    calls `accumulate_global_geometry` (`mesh.rs:681`).
  - So `build_geometry_ssbo` early-returns on empty `pending_vertices` (`mesh/geometry_ssbo.rs:312`).

  The scene set's bindings 8/9 handle this same `None` case by being `PARTIALLY_BOUND`; the comment at
  `sync_and_acquire_frame.rs:271-272` calls that "validly unbound". The caustic layout has no such flag, and the
  caustic dispatch is gated on a TLAS only.
- **Evidence**: CI job 109545444434, 10 errors on `VkDescriptorSet 0x1cb…` / `0x1cc…`:
  ```
  [ VUID-vkCmdDispatch-None-08114 ] … vkCmdDispatch(): the descriptor (VkDescriptorSet 0x1cb00000001cb[], binding 9,
  index 0) is being used in draw but has never been updated via vkUpdateDescriptorSets() or a similar call.
  ```
  (and the same for binding 10, on both sets).
- **Trigger Conditions**: RT on, a TLAS built, and no mesh has gone through `upload_scene_mesh*`. That covers the bare
  demo and any scene before its first global-geometry build.
- **Impact**: Undefined descriptor contents in a live compute dispatch. `caustic_splat.comp` dereferences
  `GlobalVertices` / `GlobalIndices` for committed-hit reconstruction, so any dynamic access is UB (a device fault on
  strict drivers). The demo likely never reaches that access (`lights_submitted=0`). The certain cost is that it keeps
  the only validation gate red.
- **Related**: CONC-D2-2026-09-29-01, #4987.
- **Suggested Fix**: Skip the caustic dispatch until the geometry bindings have been written for that slot, for
  example with a per-FIF latch set in `write_geometry_buffers`. Alternatively, bind a small placeholder buffer at
  creation, or mark 9/10 `PARTIALLY_BOUND` as the scene set does. Confirm on the next lane run.

### CONC-D3-2026-09-29-01: `npc_dialogue` shadows its `LoadedCellIndex` guard instead of dropping it, so the read guard spans the whole system including the write pass
- **Severity**: LOW. No cycle closes today (the npc_dialogue tests pass under the detector), but it is a
  guard-lifetime slip of the #4982 class that seeds a large edge fan-out.
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/npc_dialogue.rs:187-190` (`npc_dialogue_selection_system_inner`) and
  `:270-273` (`select_topic_by_form_id`).
- **Status**: NEW. ECS-2026-09-29-D1-01 covers `populate_candidates` and `running_quests_binding_entity`, and D5-01
  covers this system's Access row, but neither covers this guard's lifetime.
- **Description**: `let Some(index) = world.try_resource::<LoadedCellIndex>() else { … }; let index = index.0.clone();`
  shadows the `ResourceRead` but does not drop it, so it lives to the end of the function. In the selection system it
  is held across all of the following:
  - `world.get::<Dead>` and `world.get::<SceneAliasCandidate>`;
  - `running_quests_binding_entity` (three resource locks);
  - `select_first_info` (the CTDA evaluator's whole read set);
  - Pass 2's `apply_selection` writes to `DialogueRegistry`, `NpcDialogueTopic` and `DialogueSurfaceState`.

  The function's own comment (`:164-165`) says the selection is "applied after all reads drop". The two other
  `LoadedCellIndex` sites copy the `Arc` and `drop` the guard (`player_body.rs:134-139`,
  `npc_spawn/loot_appearance.rs:314-318`).
- **Evidence**:
  ```rust
  let Some(index) = world.try_resource::<LoadedCellIndex>() else {
      return;
  };
  let index = index.0.clone();   // the guard is shadowed, not dropped
  ```
- **Trigger Conditions**: every player activation of an alias-bound NPC, and every dialogue-UI topic click
  (`select_topic_by_form_id` runs under `&mut World`: it cannot deadlock, but it records the same edges).
- **Impact**: It records `LoadedCellIndex → {every evaluator-read type, three dialogue writes}` in the detector graph.
  Any future site that holds one of those and then reads `LoadedCellIndex` closes a cycle rooted here, while the ABBA
  lane is already red.
- **Related**: ECS-2026-09-29-D1-01, #4982.
- **Suggested Fix**: `let index = { let Some(r) = world.try_resource::<LoadedCellIndex>() else { return; }; r.0.clone() };`
  (or an explicit `drop`) at both sites.

### CONC-D4-2026-09-29-01: `equipment_appearance_system`'s Access row does not declare the mid-life gear-import surface `0182fc5e8` added
- **Severity**: LOW. This is an exclusive system, and the analyzer never pairs exclusives.
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/late.rs:409-419` (the row, last touched `24ccc8f74`) vs
  `byroredux/src/npc_spawn/loot_appearance.rs:275` → `:286-360` (`queue_midlife_imports`).
- **Status**: NEW. It is the same class as #4821, #4996 and ECS-2026-09-29-D5-01, but a third row that report does
  not name.
- **Description**: `equipment_appearance_system` now calls `queue_midlife_imports` unconditionally. That function takes:
  - `world.get::<PendingGearImport>` (read) and `world.get::<ActorBodyClass>` (read);
  - `try_resource::<LoadedCellIndex>` (resource read);
  - `query_mut::<PendingGearImport>` (write).

  None of these is declared. The row still lists only `EquipmentEventBatch`, `NpcEquipmentPart`, `Dead`,
  `NpcAppearanceHidden` (write), `Children` and `MeshHandle`.
- **Evidence**: `git show 0182fc5e8 -- byroredux/src/boot/schedule/` is empty; the gear-import commit touched no
  schedule file.
- **Trigger Conditions**: none at runtime today. The row matters on promotion to parallel and in `sys.accesses`.
- **Impact**: The row is the promotion baseline and the operator view. The mechanical guard cannot see it: the system
  is not in `PARALLEL_SYSTEMS` and is not one of the three scanned exclusives.
- **Related**: ECS-2026-09-29-D5-01, #4821, #4996.
- **Suggested Fix**: Add `.reads::<crate::npc_spawn::ActorBodyClass>()`,
  `.writes::<crate::npc_spawn::PendingGearImport>()` and `.reads_resource::<crate::cell_loader::LoadedCellIndex>()`.

### CONC-D6-2026-09-29-01: A swapchain-format-change rebuild of `EguiPass` drops the egui font atlas, and egui never re-sends it
- **Severity**: LOW. It only triggers on a surface-format change (an HDR toggle or a display move), and it only
  affects the overlay.
- **Dimension**: Resource Lifecycle (swapchain recreate)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:1106-1134` (the #2475 format-change arm: `pass.destroy` +
  `EguiPass::new`); `crates/renderer/src/vulkan/egui_pass.rs` (`promote_partial_deltas`, `image_mirrors`).
- **Status**: NEW. It dates from the #2475 full rebuild (`fd8f67e2a`, 2026-08-08) and is not caused by #4986. Searched
  "egui format change rebuild": #2475 and #2685 cover the render-pass lifetime, not texture state.
- **Verification Path**: not visible to `cargo test`. It needs a live format flip, or a unit test that the rebuild
  re-seeds managed textures.
- **Description**: The rebuilt pass has a fresh `egui_ash_renderer::Renderer` (empty `managed_textures` / `textures`)
  and an empty `image_mirrors`. The app's `egui::Context` is not reset, so egui believes the font atlas is resident and
  only sends partial deltas for new glyphs. The sequence is:
  1. A partial delta arrives. With no mirror entry, `promote_partial_deltas` passes it through unchanged.
  2. The crate's `set_textures` returns `BadTexture` for the unknown id
     (`egui-ash-renderer-0.11.0/src/renderer/mod.rs:351`).
  3. Every `cmd_draw` that samples the atlas also returns `BadTexture` (`:589`).

  The overlay is dead for the session, with an error every frame.
- **Evidence**: The resize arm constructs `EguiPass::new(...)` with no hand-over of textures or mirrors. `EguiPass::new`
  initialises `image_mirrors: FxHashMap::default()`.
- **Trigger Conditions**: `recreate_swapchain` with a surface format different from the pass's build format.
- **Impact**: Debug overlay, native pause/inventory/dialogue pages (all egui) render nothing after the flip.
- **Related**: #2475, #4986 (whose mirror now holds exactly the data a re-seed needs), REN-D5-2026-09-29-01.
- **Suggested Fix**: Take `image_mirrors` out of the old pass before `destroy`, then replay each mirror as a full delta
  into the rebuilt pass. Alternatively, have egui re-send its textures after a rebuild.

---

## Existing issues re-checked

- **#4987 (MEDIUM, open) — status update, not a new finding.** The lane now reaches a device. The evidence is CI job
  109545444434: the lavapipe ICD resolved, `bench exit status: 0`, `rt-integrity: frame=7 … rt_supported=1
  tlas_build=1`, `BYRO_LOCK_ORDER_CHECK: 1` in the env, and no engine panic. It is red only on CONC-D2-2026-09-29-01 and
  -02. Suggest closing #4987 when those are fixed and a clean lane run exists.
- **#4780 (MEDIUM, open)** and **#4989 (LOW, open)**: code unchanged since filing.
- **Verified fixed** since the 09-28 report:
  - #4986: partial egui deltas are promoted against a CPU mirror. The copy-on-write `Arc::make_mut` is correct. The
    crate allocates the new set before freeing the old one, from a `FREE_DESCRIPTOR_SET` pool.
  - #4988: rider 14 is in `sync.rs` and in `frames_in_flight_contract_names_every_dependent_resource`.
  - #4990: comment only.
  - #4991: render-skip sink test.
  - #4992: `about_to_wait` early return before `scheduler.run`, pinned.
  - #4993: `PIPESTATUS[0]`.
  - #4994: cross-file hops and `world.get` forms.
  - #4995: facing moved to an Update exclusive, pinned.
  - #4996: `container_loot` row.
  - #4997: production-slice scan.
  - #4998: partial TIMESTAMP pools released, pinned.
  - #4999: doc.
  - #5000: BSA and CSG threaded tests.

## Dimension notes (clean areas)

- **D1 — Vulkan Queue & AS Sync.**
  - No behavioural commit in `acceleration/` or the queue and acquire paths.
  - The `MAX_FRAMES_IN_FLIGHT == 2` const-assert is intact, and the FIF-contract and one-time-lock-scope guards are
    green.
  - The new `GearImportLoader::step` reuses the corpse loader's upload path on the main thread.
- **D2 — Compute → AS → Fragment Chains.**
  - `554ef5c44` split the volumetrics u16 assert in two, with the same bounds.
  - It also turned the groundcover-models spacing guard NaN-explicit, which is semantically identical.
  - `546e7fbc7` makes `RendererConfig::default()` auto-exposure. That was already the CLI default; the exposure-meter
    barriers are unchanged.
- **D3 — ECS Lock Ordering.**
  - `424aad268` `disengage_lost_contact` snapshots, then acquires; the physics LOS guard is taken per cast.
  - `queue_midlife_imports` and `equipment_appearance_system` drop every read guard before writing.
  - The `app_frame.rs` dialogue-surface guards are statement-scoped.
  - `extensions/` has no commits.
- **D4 — Scheduler Proof.** 9 `add_to_with_access` = `PARALLEL_SYSTEMS.len()` and 37 exclusives-with-access. The new
  single-writer state is handed off in writer-before-reader order:
  - `DialogueSurfaceState`: Late → `app_frame`, same frame.
  - `PendingGearImport`: Late → `about_to_wait` loader, a one-frame hand-off by design.
- **D5 — Physics RwLock.** All named snapshot-then-acquire guards are present, not ignored, and green. The new
  `faction_hostility` `PhysicsWorld` reader holds no storage guard.
- **D6 — Lifecycle.**
  - Teardown and resize are unchanged apart from the finding above.
  - The egui mirror is host-only.
  - Player-gear GPU release belongs to REN-D5-2026-09-29-02.
- **D7 — Worker Threads.**
  - No new thread, rayon section or channel.
  - The new `Archive::warned` `Mutex<HashSet>` (`ec63d2636`) is on the error path only, statement-scoped and
    poison-recovered.

## Routed (not concurrency)

- `/audit-scripting`: the doc on `running_quests_binding_entity` (`crates/scripting/src/scene/quest_alias.rs:914-915`)
  says a missing `QuestStageState` means the quest "never appears". The code
  (`running.as_ref().is_none_or(...)`) instead treats every installed quest as running when the resource is absent.

## Skill drift (fold into the next `/audit-concurrency` sync)

- **Dim 3 known-open line is stale.** It reads "#4987: that lane has not yet been seen to reach a device". As of
  36574364470 / 36609043348 it does; the next run should read the lane's validation output, not just its exit.
- **`crates/renderer/src/vulkan/restir.rs` is in no dimension's `Paths:`.** Its per-frame fill + barrier
  (CONC-D2-2026-09-29-01) is a TRANSFER → FRAGMENT chain; add it to Dim 2.
- Dim 3's First step runs `-p byroredux` only; the scripting-crate cycle needs `-p byroredux-scripting`, or the CI
  workspace form. This matches ECS-2026-09-29-SK-01.

---

Publish with: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-09-29.md` (domain label **sync** for
CONC-D2-2026-09-29-01/-02 and D6-01; **concurrency** for D3-01 and D4-01).
