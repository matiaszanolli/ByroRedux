# Concurrency & Synchronization Audit — 2026-08-30

**Repo**: `/mnt/data/src/gamebyro-redux` · **HEAD**: `64f64480` · **Depth**: `deep` · **Dimensions**: all 7
**Suite**: `comprehensive` · **Dedup baseline**: `gh issue list --limit 200` (2026-08-30), plus every
`docs/audits/AUDIT_CONCURRENCY_*.md` back to 2026-08-12 and the three same-day siblings
(`AUDIT_ECS_2026-08-30.md`, `AUDIT_SAFETY_2026-08-30.md`, `AUDIT_RENDERER_2026-08-30.md`).

---

## Executive Summary

**20 findings: 0 CRITICAL · 2 HIGH · 10 MEDIUM · 8 LOW.**
19 are NEW; 1 (CONC-D3-2026-08-30-05) is an **amendment to open issue #3445**, not a separate issue.
**All 7 dimensions produced at least one finding** — none came back empty this sweep.

**64 candidate findings were dropped because the premise no longer held at HEAD** (per-dimension:
D1 5, D2 7, D3 12, D4 4, D5 13, D6 10, D7 13). That is ~76% of everything the dimension agents
surfaced, against the ~1-in-6 historical stale rate — the difference is that this sweep required
every candidate to be re-read at `64f64480` and explicitly disproved before inclusion. The dropped
list is preserved per dimension so the same false premises are not re-derived next sweep.

### The two HIGH findings

**CONC-D5-2026-08-30-01 is empirically confirmed, not inferred — the project's own ABBA detector
fails on a clean tree at HEAD**, and the dedicated `lock-order-check` CI job is red:

```
$ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins
test result: FAILED. 1642 passed; 5 failed; 17 ignored     (all ragdoll::tests::*)
  "lock-order cycle: PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld"

$ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins -- --skip combat_approach_line_of_sight
test result: ok. 1645 passed; 0 failed
```

Reproduced independently by the orchestrator. One site — `combat_approach_line_of_sight_reaches`,
added by `5c8a1581` (#3423) — binds the `PhysicsWorld` guard and acquires two storages under it.
It is the *only* `PhysicsWorld → storage` inversion in the tree, and it opens every one of the
many safe `storage → PhysicsWorld` edges at once.

**CONC-D2-2026-08-30-01** is a barrier finding and is filed under the speculative-fix guardrail
(see below): `caustic_splat.comp` grew its own inline `SkinnedVertexRef` deref in `9bf7d024`
(2026-08-15) — *one day after* `AUDIT_CONCURRENCY_2026-08-14.md` cleared the missing `COMPUTE`
dst bit on the skin publish barrier by tracing the include graph. An include-graph trace cannot
see an inline deref, so the clearance went stale the next day.

### Speculative-fix guardrail compliance

No Vulkan barrier / stage-mask / render-pass restructure is proposed on reasoning alone.
Every sync finding (D1-01, D2-01, D2-02, D2-03, D6-01) names the concrete confirming signal —
the exact `SYNC-HAZARD-*` string and the resource it must name under a `BYRO_VALIDATION=1`
**release** build, or the specific `VUID-*`. **No GPU run was performed in this sweep**, so each
is a source-level conclusion pending that run. Where a fix is suggested it is an *additive* mask
widening (which can only add dependencies, never remove one), and it is still gated on the
before/after validation signal.

### Severity roll-up

| Dimension | CRIT | HIGH | MED | LOW | Dropped as stale |
|---|---|---|---|---|---|
| D1 · Vulkan Queue & AS Sync | 0 | 0 | 1 | 2 | 5 |
| D2 · Compute → AS → Fragment Chains | 0 | **1** | 1 | 1 | 7 |
| D3 · ECS Lock Ordering & Deadlock | 0 | 0 | 4 | 1 | 12 |
| D4 · Scheduler Access Declarations | 0 | 0 | 1 | 2 | 4 |
| D5 · RwLock — Resource↔Storage & Physics | 0 | **1** | 1 | 1 | 13 |
| D6 · Resource Lifecycle | 0 | 0 | 1 | 1 | 10 |
| D7 · Worker Threads & Thread-Safety | 0 | 0 | 1 | 0 | 13 |
| **Total** | **0** | **2** | **10** | **8** | **64** |

### Finding index

| ID | Sev | Status | Title |
|---|---|---|---|
| CONC-D5-2026-08-30-01 | HIGH | NEW | `combat_approach_line_of_sight_reaches` holds `PhysicsWorld` across `RapierHandles` — live `PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld` cycle; CI red |
| CONC-D2-2026-08-30-01 | HIGH | NEW | `caustic_splat.comp` reads the skinned-vertex SSBO from COMPUTE; the skin chain publishes only to `AS_BUILD \| FRAGMENT` |
| CONC-D1-2026-08-30-01 | MED | NEW | `MAX_FRAMES_IN_FLIGHT`'s const-assert names only the depth image; four more resources rely on the same both-slots-wait identity |
| CONC-D2-2026-08-30-02 | MED | NEW | `CausticPipeline::clear_for_skip`'s `TRANSFER_WRITE` is never in the next dispatch's decay-read source scope |
| CONC-D3-2026-08-30-01 | MED | NEW | `skin.dump` holds `SkinnedMesh` across `format_skin_dump`'s `GlobalTransform` read — the console half of #2388 was never fixed |
| CONC-D3-2026-08-30-02 | MED | NEW | `save::validate_animation` takes `AnimationPlayer → AnimationClipRegistry`, inverting `animation_system_inner`'s #2400 order |
| CONC-D3-2026-08-30-03 | MED | NEW | `scene.show` takes `SceneRegistry → ScenePlayer`; `actor_quest_trigger_is_in_sequence` takes the reverse |
| CONC-D3-2026-08-30-05 | MED | **Existing: #3445** | #3445's blast radius is wider than filed — `mat.list` and `eval_walk_entity` supply two more opposing `StringPool` edges |
| CONC-D4-2026-08-30-01 | MED | NEW | `make_billboard_system` (PostUpdate) reads the camera pose `camera_follow_system` (Late) authors — one frame of lag, invisible to both KPIs |
| CONC-D5-2026-08-30-02 | MED | NEW | `ragdoll_writeback_system` acquires `LocalBound`/`WorldBound` under the `PhysicsWorld` guard — a second `PhysicsWorld → storage` edge |
| CONC-D6-2026-08-30-01 | MED | NEW | `skin_slots` teardown nested under `skin_compute.is_some()` — the exact shape #3374 un-nested for `morph_slots` |
| CONC-D7-2026-08-30-01 | MED | NEW | `Ba2Archive::extract` holds its `Mutex<File>` across zlib/LZ4 inflate; the BSA sibling drops it first |
| CONC-D1-2026-08-30-02 | LOW | NEW | `with_one_time_commands`' doc header still describes the pre-#1713 lock scope |
| CONC-D1-2026-08-30-03 | LOW | NEW | `FrameSync::images_in_flight`'s deadlock rationale describes the pre-#952 `reset_fences` position |
| CONC-D2-2026-08-30-03 | LOW | NEW | `VolumetricsPipeline::record_neutral_frame`'s clear is not in the next dispatch's `pre_int_write` source scope (WAW) |
| CONC-D3-2026-08-30-04 | LOW | NEW | The canonical order table covers 3 clusters; the animation, scene/quest and physics-shape clusters have no documented direction |
| CONC-D4-2026-08-30-02 | LOW | NEW | `submersion_system` (Late) sets `ParticleEmitter.rate`; its only consumer runs in PostUpdate |
| CONC-D4-2026-08-30-03 | LOW | NEW | `player_controller_system`'s doc points its access declaration at `main.rs`, which has held none since #1858/#1670 |
| CONC-D5-2026-08-30-03 | LOW | NEW | The canonical order table omits `PhysicsWorld` entirely, so the rule D5-01 breaks is unwritten |
| CONC-D6-2026-08-30-02 | LOW | NEW | `destroy_allocator_owned_resources` documents "reverse-creation order"; the block is neither reverse nor forward |

### Cross-cutting pattern: the canonical order table is the common root cause

Three independent LOW findings (D3-04, D5-03, and by implication D3-02/D3-03/D5-01) reduce to
one fact: `docs/engine/ecs.md`'s canonical acquisition-order table documents the
hierarchy/skinning/bounds cluster and the CHARAL pair, and **nothing else**. It has no entry for
`PhysicsWorld` — the single most co-acquired resource in the engine and now the hub of the live
cycle — nor for the animation, scene/quest or physics-shape clusters. Those directions exist only
inside individual function comments, one of which (`byroredux/src/commands/view.rs:184-185`)
*states the rule while doing the opposite*. Every one of this sweep's new lock-order findings is a
second consumer re-deriving an order from scratch and getting the opposite one. Fixing the table
is cheaper than fixing them one at a time.

### Known-real prior hazards re-verified, NOT re-reported

- **#3441 `ActorValues` / `CharacterRuleset` cycle — NOT regressed.** Confirmed independently by
  both D3 and D5: `crates/scripting/src/condition.rs:470-500` still snapshots `ActorValues` into
  an owned clone and reads `CharacterLevel` before `try_resource::<CharacterRuleset>()`, and both
  pinning tests are present.
- **#3303 `pull_dynamic` `Transform`/`GlobalTransform` split** — intact, and its guard test is
  CI-executed.
- **#1167 streaming Drop ordering, #1782 deferred scratch, #1790 `WRITE|READ` scratch mask,
  #a476b256/#1449 deferred AS destroy, #507945d8/#1436 AS build-input `SHADER_READ`, #2384/#2385/#2386
  lock-tracker guards, #1105 volumetrics latch, #931 bloom RAW chain** — all re-verified intact.

### Already-filed, encountered and skipped

#3090, #3247, #3249, #3429, #3442, #3444, #3445, #3446, #3449, #3473, #3493.


---

# Findings by Dimension

## Dimension 1 — Vulkan Queue & Acceleration-Structure Sync


Audit: ByroRedux concurrency sweep, 2026-08-30. HEAD `64f64480`. Depth: **deep**.
Scope files re-read at HEAD: `crates/renderer/src/vulkan/context/{draw,resize,teardown,init,mod,post_passes,skinned_blas_refit,depth_capture,screenshot}.rs`,
`crates/renderer/src/vulkan/{sync,texture,instance,egui_pass,presentation,skin_compute,upscaling}.rs`,
`crates/renderer/src/vulkan/acceleration/{mod,blas_static,blas_skinned,tlas,memory,predicates,types,constants}.rs`,
`crates/renderer/src/deferred_destroy.rs`, `crates/renderer/src/vulkan/scene_buffer/upload.rs`.

Delta basis: the prior full sweep (`docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md`,
committed `0262f716`) reported **0 findings** for this dimension against the same
checklist. I re-verified every one of its nine claims against HEAD rather than
inheriting them, then diffed `0262f716..HEAD` over the renderer to find what the
prior sweep could not have seen (`git diff --stat` → 21 files, +1432/-254; the
sync-relevant deltas are #3469 cached skinned device address, #3308 depth capture,
#3426 presentation-pass UI overlay, #3443/#3467 geometry-rebuild gate + timing).

---

### Findings

#### CONC-D1-2026-08-30-01: `MAX_FRAMES_IN_FLIGHT`'s const-assert names only the depth image, but at least four non-per-FIF resources rely on the same "both slots == device idle" identity

- **Severity**: MEDIUM
- **Dimension**: Vulkan Queue & Acceleration-Structure Sync
- **Location**: `crates/renderer/src/vulkan/sync.rs:8-49` (the `#870` block + `const _: () = assert!`), `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:216-231`
- **Status**: NEW
- **Trigger Conditions**: Latent — **unreachable at HEAD** (`MAX_FRAMES_IN_FLIGHT == 2`, enforced by the const-assert). Reachable the moment someone follows remediation option **(a)** in that block ("making the depth image per-frame-in-flight") and bumps the constant to 3+ *without* also doing option (b). At 3 slots, `draw_frame`'s wait covers `in_flight[frame]` and `in_flight[(frame+1) % 3]`; slot `(frame+2) % 3` is still executing. `build_skinned_blas_batched_on_cmd` then immediately `destroy()`s a `blas_scratch_buffer` whose device address that slot's recorded `cmd_build_acceleration_structures` still holds → AS build against freed memory.
- **Verification Path**: `cargo test` observable only as the const-assert (which is what currently blocks the bad state). The failure mode itself would be **validation layer** (`VUID-vkDestroyBuffer-buffer-00922`, sync-validation use-after-free on the scratch allocation under `BYRO_VALIDATION=1` release) or a raw device fault. **Not reproducible today** — this is a guard-completeness finding, not a live bug.
- **Description**: `sync.rs` correctly identifies that the both-slots `wait_for_fences` is equivalent to device-idle *only* at `MAX_FRAMES_IN_FLIGHT == 2`, and lists two remediations. But it attributes the whole constraint to one resource (the shared depth image) and offers option (a) — make *that* image per-FIF — as sufficient. It is not. The same identity is load-bearing for at least four other non-per-FIF resources, none of which is named there and only one of which (`images_in_flight`, `sync.rs:105-110`) carries its own warning:
  1. `blas_scratch_buffer`'s **immediate** (deliberately non-deferred, correct today) destroy at `blas_skinned.rs:229-231`;
  2. `depth_capture_staging` — destroyed and reallocated *during frame recording* (`depth_capture.rs:137` → `:240-245`), with a SAFETY comment asserting "no command buffer can still reference `buffer`";
  3. `terrain_tile_buffer` — one shared DEVICE_LOCAL buffer overwritten by a blocking staged copy mid-`draw_frame` (`draw.rs:3361-3372` → `scene_buffer/upload.rs:840`);
  4. `screenshot_staging` / `depth_capture_pending_readback`, single-slot host readbacks gated purely on the top-of-frame wait.
  Separately, `blas_skinned.rs`'s own SAFETY comment states a *narrower* premise than the one that actually holds: it justifies the immediate free by "this same frame-in-flight slot's previous recording ... has therefore already retired". That argument alone is insufficient — the other slot's recording also captures the scratch address. What makes the site safe is the both-slots wait in `draw.rs:1624-1637`, which the comment never mentions. So at 3 slots the comment would still *read* correct while the code became a use-after-free.
- **Evidence**:
  ```
  sync.rs:36-42
  // Bumping this constant requires either:
  //   (a) making the depth image per-frame-in-flight
  //       (`Vec<vk::Image>` indexed by frame_index, mirroring the
  //       G-buffer pattern at `gbuffer.rs:52`), OR
  //   (b) extending the fence wait to cover all in-flight slots
  //       (currently 2; would become MAX_FRAMES_IN_FLIGHT - 1 fences).

  sync.rs:46-49
  const _: () = assert!(
      MAX_FRAMES_IN_FLIGHT == 2,
      "shared depth image at context/mod.rs:580 requires \
       MAX_FRAMES_IN_FLIGHT == 2; see #870 for the safety contract"
  );

  blas_skinned.rs:216-231
  // SAFETY / not a #1782 sibling: ... this call site
  // runs from `record_skinned_blas_refit`, itself called from
  // `draw_frame` AFTER that frame's own `wait_for_fences`. Any
  // command buffer that could reference the *old* scratch
  // buffer's device address (this same frame-in-flight slot's
  // previous recording) has therefore already retired. Do NOT
  // "fix" this site by copying the deferred-destroy pattern —
  // ...
  if let Some(mut old) = self.blas_scratch_buffer.take() {
      old.destroy(device, allocator);
  }
  ```
- **Impact**: The const-assert is the project's designated tripwire for this class. Its message offers a remediation that would satisfy the assert-removal while silently breaking four other invariants — the "short list read as exhaustive" hazard the same comment block warns about for the depth consumers, applied one level up. No impact at HEAD.
- **Related**: #870 (the const-assert), #1782 (deferred scratch destroy), #3442 (the source-scan pin that cannot see `(f + 1) % MAX_FRAMES_IN_FLIGHT`), #418 (deferred-destroy tick placement).
- **Suggested Fix**: Amend the `#870` block to state that option (a) alone is **not** sufficient — the both-slots wait is depended on by the immediate scratch free, the depth-capture/screenshot staging destroys, and the terrain-tile buffer — so option (b) (or per-FIF-ing all of them) is mandatory on any bump; and add one line to `blas_skinned.rs:216-228` naming `draw.rs`'s *both*-slots wait as the actual guarantee rather than the slot-local one. Documentation only; no code change, no barrier change.

---

#### CONC-D1-2026-08-30-02: `with_one_time_commands`' doc header still describes the pre-#1713 lock scope, contradicting the #1713 regression test 220 lines below it

- **Severity**: LOW
- **Dimension**: Vulkan Queue & Acceleration-Structure Sync
- **Location**: `crates/renderer/src/vulkan/texture.rs:644-647`
- **Status**: NEW
- **Trigger Conditions**: None — documentation defect. Surfaces when a maintainer reads the public helper's contract instead of the private `_inner`'s comment.
- **Verification Path**: `cargo test` — `vulkan::texture::one_time_lock_scope_tests::queue_guard_released_before_one_time_fence_wait` passes at HEAD (verified: 7/7 green in `cargo test -p byroredux-renderer --lib -- one_time_lock_scope_tests vulkan::sync::tests`), which is precisely what makes the doc line false.
- **Description**: Two doc paragraphs are stacked on one function, and the first is the pre-#1713 one. `with_one_time_commands` is documented as holding the queue `Mutex` "for the submit+wait", which is the exact behaviour CONC-D1-01 / #1713 removed and which the in-file regression test at `:863-903` now *asserts against* (it requires a scope-closing `}` between the submit and the wait). The live `_inner` comment at `:801-814` states the correct rule. This is the same defect class as #3527 / #3493 (a fix orphaning its predecessor's rationale), on the one invariant this dimension's checklist item 1 is built around.
- **Evidence**:
  ```
  texture.rs:644-647
  /// Execute a one-time-submit command buffer: allocate, record, submit, wait, free.
  ///
  /// The queue `Mutex` is locked only for the submit+wait, not during recording.
  /// Run a closure in a one-time-submit command buffer, then wait for completion.
  ```
  versus the code it documents:
  ```
  texture.rs:812-816
  let submit_result = {
      let q = queue.lock().expect("graphics queue lock poisoned");
      device.queue_submit(*q, &[submit_info], fence)
  };
  ```
  (guard scope closes before `wait_for_fences` at `:825`).
- **Impact**: A maintainer optimising the one-time path could "restore" the documented behaviour and re-serialise every future second graphics-queue thread across a GPU-execution wait — the regression test would catch it, but only after the change. Also note the duplicated summary sentence (`:644` and `:647`) reads as an unresolved merge.
- **Related**: #1713 (CONC-D1-01), audit 2026-05-16 CONC-D2-NEW-01.
- **Suggested Fix**: Delete line 646 (or rewrite it as "the queue `Mutex` is locked for the submit **only** — released before the fence wait, see #1713") and drop the duplicated summary line so the header has one summary.

---

#### CONC-D1-2026-08-30-03: `FrameSync`'s `images_in_flight` invariant doc still describes the pre-#952 `reset_fences` position, so its stated deadlock rationale is false at HEAD

- **Severity**: LOW
- **Dimension**: Vulkan Queue & Acceleration-Structure Sync
- **Location**: `crates/renderer/src/vulkan/sync.rs:9`, `:28`, `:47`, `:93-110`
- **Status**: NEW
- **Trigger Conditions**: None — documentation defect on the frame-in-flight contract.
- **Verification Path**: `cargo test` / source inspection. `grep -n "reset_fences" context/draw.rs` → `1763` (a comment recording the move), `3801`, `3811`. There is no `reset_fences` near the image-fence guard.
- **Description**: `FrameSync::images_in_flight`'s doc explains why the `image_fence != in_flight[frame]` aliasing guard exists: *"Reusing the slot's own fence would block on an UNSIGNALED handle (it's reset at draw.rs:191) and deadlock."* #952 moved `reset_fences` to immediately before `queue_submit` (`draw.rs:3801-3812`), roughly 2,060 lines *after* the guard at `draw.rs:1745-1746`. At the guard, `in_flight[frame]` is still SIGNALED (it was waited on at `:1624-1637` and has not been reset), so waiting on it would return immediately — the guard is a fast-path skip, not a deadlock preventer. The doc's stated hazard no longer exists, and the same block's five line citations are all stale by ~1,500-3,600 lines: `context/mod.rs:580-582` for the shared depth image (actually `context/mod.rs:1460-1461` — line 580 is inside `DrawCommand::material_hash`), `draw.rs:108-120` and `:144-156` for the both-slots wait (actually `:1624-1637`), `draw.rs:179-186` / `:180` for the image-fence read (actually `:1745-1746`).
- **Evidence**:
  ```
  sync.rs:95-106
  /// by the time `draw_frame` next reads it at `context/draw.rs:179-186`.
  /// This is upheld upstream by the *both-slots* `wait_for_fences` at
  /// `context/draw.rs:144-156`, ...
  /// The aliasing guard `image_fence != in_flight[frame]` at draw.rs:180
  /// then prevents waiting on the just-reset fence belonging to the
  /// current frame slot. Reusing the slot's own fence would block on
  /// an UNSIGNALED handle (it's reset at draw.rs:191) and deadlock.
  ```
  ```
  sync.rs:47   "shared depth image at context/mod.rs:580 requires \
  context/mod.rs:577   pub fn material_hash(&self) -> u64 {     <-- what is actually at :580
  context/mod.rs:1461  depth_image: vk::Image,                  <-- the real site
  ```
- **Impact**: The const-assert's failure message (the tripwire a future `MAX_FRAMES_IN_FLIGHT` bump lands on) points at the wrong file location for the resource it names. And the aliasing-guard rationale, if believed, would let someone conclude the guard is what makes an early `reset_fences` safe — the exact inversion of what #952 established. Doc rot on a sync contract, no runtime effect.
- **Related**: #952 (REN-D1-NEW-04), #953 (REN-D1-NEW-05), #870, #282, #2794 (the same stale-line-number class already fixed once in `deferred_destroy.rs`).
- **Suggested Fix**: Replace the five hard line citations with symbol names (`draw_frame`'s both-slots `wait_for_fences`, `VulkanContext::depth_image`, `draw_frame`'s pre-submit `reset_fences`) — the `deferred_destroy.rs:38-46` pattern after #2794 — and restate the guard's purpose as "skip a redundant wait on a fence this frame already waited on", not "prevent a deadlock on an unsignaled fence".

---

### Dropped (stale premise)

**5 candidate findings dropped after checking the premise against HEAD.**

1. **"`build_skinned_blas_batched_on_cmd`'s immediate scratch destroy is a missed #1782 instance."** Dropped as a *use-after-free* claim: `draw_frame` waits on **both** `in_flight[frame]` and `in_flight[(frame+1) % MAX_FRAMES_IN_FLIGHT]` (`draw.rs:1624-1637`), which at `MAX_FRAMES_IN_FLIGHT == 2` covers every slot, so no in-flight command buffer can hold the old scratch address. The task brief also pre-excluded it. Only the *documentation/latent-coupling* half survived, as CONC-D1-2026-08-30-01.
2. **"#3469's cached `SkinSlot::output_address` can go stale."** Dropped: `output_buffer` is allocated exactly once in `SkinComputePipeline::create_slot` (`skin_compute.rs:499-551`) and only ever released by `destroy_slot` (`:585-592`) — there is no realloc-in-place path, so the address is stable for the slot's lifetime, which is the Vulkan guarantee the SAFETY comment cites. A source pin exists at `skin_compute.rs:1655-1680`, and the `#2402` "filter before the address read" ordering is preserved at `draw.rs:3018-3028`.
3. **"`record_presentation_pass`'s new UI overlay draw (#3426) needs a UI-texture barrier it does not have."** Dropped: the Scaleform texture upload completes on a blocking fenced submit *before* `draw_frame` is entered (the #3429 path), and the overlay's SSBO reads are covered by the earlier in-`cmd` HOST→SHADER barriers, which apply to all later commands in the same submission. The overlay is also in the same subpass as the tone-map draw, so rasterization order sequences the blend.
4. **"`draw_frame`'s acquire→submit window has `?`-propagating calls that leak the `image_available[frame]` signal."** Dropped: the window `draw.rs:1744-3800` contains **zero** `?` operators (verified by scan); all five error exits are explicit `return Err` arms each preceded by `recreate_image_available_for_frame`. The comment block at `:1735-1743` describes the real code.
5. **"`recreate_in_flight_for_frame` leaves `images_in_flight` slots pointing at the fence it destroys."** Dropped: fixed under #1188 — `invalidate_images_in_flight_for_fence` runs before `destroy_fence` (`sync.rs:356-357`), with three unit tests.

Additionally **not re-reported** (prior-report overlap, per CONTEXT dedup):
- `upload_terrain_tiles`' blocking submit+fence-wait inside `draw_frame` (`draw.rs:3361-3372`) — change-gated on a terrain-dirty cell transition; explicitly ACCEPTED in `AUDIT_CONCURRENCY_2026-08-14.md` row 10.
- `EguiPass::dispatch`'s `set_textures` holding the graphics-queue guard across an internal submit+wait (`egui_pass.rs:251-254`) — same 2026-08-14 row; gated on a non-empty `textures_delta.set` (font-atlas change), and the pool it uses is `transfer_pool`, not the frame pool.
- The `mesh.rs` chunked geometry rebuild's per-frame one-time submit — traced and deliberately not filed by `AUDIT_CONCURRENCY_2026-08-27b.md`; #3443 has since added a `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` gate on top.
- The animating-menu per-frame `VkImage` + fence — **#3429**, already filed.

---

### Verified clean

All nine checklist items confirmed to still hold at HEAD.

1. **Single-Mutex queue submission — CLEAN.** `graphics_queue` / `present_queue` are both `Arc<Mutex<vk::Queue>>` (`context/mod.rs:1864`, `:1870`); `present_queue` is `Arc::clone(&graphics_queue)` when families match (`context/init.rs:125-129`). All three submit/present sites **bind** the guard rather than `*queue.lock()`: `draw.rs:3827-3862` (submit; explicit `drop(queue)` on both the error and success arms, before the recovery calls), `draw.rs:3940-3958` (present; guard scoped to the `unsafe` block), `texture.rs:812-816` (one-time; scope closes before `wait_for_fences` at `:825`). No `wait_for_fences` executes under any queue guard. `one_time_lock_scope_tests` exists at `texture.rs:862-903` and does assert lock→submit→wait *and* that a scope-closing brace sits between the submit and the wait — **run and passing**.
2. **Frame-in-flight discipline — CLEAN.** Dual-fence wait at `draw.rs:1624-1637` precedes every per-frame resource reuse (deferred-destroy tick, timestamp read, image-health harvest, screenshot + depth readbacks, morph-weight flush). `reset_fences` sits immediately before `queue_submit` (`:3801-3823`). `image_available[frame]` is never left signal-pending: the pre-acquire bail is at `:1596`, and the acquire→submit window is `?`-free with five explicit recovery arms. The ordering comment at `:1735-1743` matches the code.
3. **Acquire → render → present chain — CLEAN.** Wait `image_available[frame]` @ `COLOR_ATTACHMENT_OUTPUT` (`draw.rs:3778-3779`); signal `render_finished[img]` — **per swapchain image**, not per frame slot (`:3793`); present waits the same handle (`:3939`). `recreate_for_swapchain` (`sync.rs:220-272`) rebuilds `render_finished` and `images_in_flight` from `swapchain_image_count` and `in_flight` as SIGNALED, destroy-then-null before the fallible recreate. Pinned by `render_finished_is_sized_and_indexed_per_swapchain_image` (passing).
4. **AS build → read barrier — CLEAN.** Static BLAS `AS_WRITE → AS_READ` at `blas_static.rs:604-611`. `record_scratch_serialize_barrier`'s dst mask is still `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR` (`blas_skinned.rs:697-703`) — the #1790 guard **intact**; self-emitted before the first batched build (`:294-300`) and at refit entry (`:446`). Refit→TLAS handoff at `skinned_blas_refit.rs:671-679`. The frame's single `AS_BUILD/WRITE → (FRAGMENT|COMPUTE)/AS_READ` publish runs on **both** the success and failure arms of `build_tlas` (`draw.rs:2657-2698`), the #2931 fix.
5. **Deferred BLAS-scratch destruction (#1782) — CLEAN.** `blas_static.rs:517-519` and `memory.rs:87-92`, `:112-113` all route the retired `blas_scratch_buffer` through `pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN)`. Ticked in lockstep with `pending_destroy_blas` (`blas_static.rs:104-115`) and drained at shutdown (`:143-158`). The `build_skinned_blas_batched_on_cmd` immediate free is the documented deliberate exception and was not flagged as a missed instance.
6. **AS build INPUT access flag (#507945d8 / #1436) — CLEAN.** TLAS instance copy: `TRANSFER_WRITE → SHADER_READ` at `TRANSFER → ACCELERATION_STRUCTURE_BUILD_KHR` (`tlas.rs:237-248`). Skinned-vertex compute output: `SHADER_WRITE → SHADER_READ` at `COMPUTE_SHADER → ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER` (`skinned_blas_refit.rs:480-488`, the #2403 widening). Neither uses `ACCELERATION_STRUCTURE_READ_KHR`. Rationale intact at `skinned_blas_refit.rs:44-52`.
7. **Deferred AS destruction vs in-flight reads (#a476b256 / #1449) — CLEAN.** Eviction pushes to `pending_destroy_blas` (`blas_static.rs:1094`); `drop_blas` at `:60`; skinned drop at `blas_skinned.rs:733`; the handle-overwrite guard `self.drop_blas(mesh_handle)` before slot reuse at `blas_static.rs:910` (#2481). `DEFAULT_COUNTDOWN == MAX_FRAMES_IN_FLIGHT` (`deferred_destroy.rs:46`) and the tick runs after the fence wait (#418). Shutdown drains via `AccelerationManager::destroy()` → `drain_pending_destroys` and `VulkanContext::flush_pending_destroys` (`context/mod.rs:1913-1940`, `device_wait_idle` first). **No** immediate `destroy_acceleration_structure` at any eviction site; the 8 immediate calls in `blas_static.rs` / `blas_skinned.rs:248` are all rollback of just-created, never-registered, never-submitted structures, and the two in `tlas.rs` are the slot-resize path guarded by its own `device_wait_idle` (`:985`).
8. **Swapchain recreate sync — CLEAN.** `recreate_swapchain_core` opens with `device_wait_idle` (`resize.rs:34-38`), and every destroy/rebuild phase downstream cites it. The new `PresentationPipeline` (#3426) is destroyed and rebuilt unconditionally (`resize.rs:1005-1050`) — including the UI overlay pipeline that moved into it, whose old rebuild site was correctly removed (`resize.rs:326-331`). `frame_sync.recreate_for_swapchain` is called with the new image count at `resize.rs:1082`. Teardown order still destroys `presentation` (`teardown.rs:185-186`) before the shared `pipeline_layout` (`:313`).
9. **One-time blocking submits in the frame path — CLEAN (no new instances).** Full census of `with_one_time_commands` / `with_one_time_commands_reuse_fence` / `submit_one_time` call sites: all are load-time (`init.rs`, `ssao.rs`, `bloom.rs`, `svgf.rs`, `taa.rs`, `caustic.rs`, `water_caustic.rs`, `volumetrics.rs`, `placeholder.rs`, `exposure.rs`, `buffer.rs`, `blas_static.rs`'s cell-load batched build) or previously-accepted change-gated cases (`upload_terrain_tiles`, egui `set_textures`, the `mesh.rs` chunked rebuild). The new `#3308` depth capture is **not** one: it records into the frame's own `cmd` and reads back at the top of the next frame after the fence wait (`depth_capture.rs:34-45`, `draw.rs:1707-1708`) — the screenshot pattern. The new `#3426` presentation pass records only.

Also verified independently (not on the checklist):
- **TLAS scratch is per-frame-in-flight** (`tlas.rs:283-297`, `scratch_buffers[frame_index]`), so it never needs the shared-scratch serialise barrier; `shrink_tlas_scratch_to_fit` is explicitly excluded from `shrink_blas_scratch_to_fit` for that reason (`memory.rs:33-39`).
- **#2674's post-record commit** of `last_blas_addresses` / `needs_full_rebuild` / `last_blas_map_gen` still lands after `cmd_build_acceleration_structures`, not before (`tlas.rs:131-149`, `:371-405`).
- **#2460's union peak walk** (`blas_entries` ∪ `skinned_blas`) is intact in `shrink_blas_scratch_to_fit` (`memory.rs:70-76`), and the shrink target still carries `scratch_alignment_padding` (`:108`).
- **egui's upload pool is `transfer_pool`**, not the frame command pool (`draw.rs:3741`), so its mid-recording one-time submit cannot alias the frame's recording buffer.

---

### Coverage gaps

1. **No validation-layer or RenderDoc run was performed.** Every barrier/stage-mask conclusion above is source-level. Per the CONTEXT guardrail I proposed no barrier changes and no finding rests on "this barrier looks wrong". Confirming a *positive* clean bill for items 4 and 6 needs a `BYRO_VALIDATION=1` release build with sync-validation and a zero RAW/WAR hazard count on a skinned-actor cell (e.g. `skyrim_se-WhiterunDragonsreach`, where `skin_pool_live = 83` per the #3469 note).
2. **Carried forward, still unconfirmed: CONC-D1-H1 (audit 2026-08-12).** `recreate_image_available_for_frame` (`sync.rs:298-323`) destroys a binary semaphore that has a **pending acquire signal**. Whether VVL treats that as in-use at `vkDestroySemaphore` (`VUID-vkDestroySemaphore-semaphore-05149`) could not be settled from source then and cannot now — the spec text is about *submitted batches* and the acquire signal sits outside that wording. I found **no open GitHub issue** covering it in `/tmp/audit/concurrency/issues.json`, so the hypothesis appears to have been raised and dropped without being filed. I am not re-filing it as a finding (prior-report overlap) but flag it as un-tracked. Confirming experiment unchanged: `BYRO_VALIDATION=1` release build, fault-inject `begin_command_buffer`, look for that VUID at the recreate. **Do not change that code before the run.**
3. **Multi-threaded queue/pool access is untested, not unaudited.** The `Mutex<vk::Queue>` discipline is correct for a second submitter, but `transfer_pool` itself is a bare `vk::CommandPool` with no external-synchronisation wrapper, and `vkAllocateCommandBuffers` / `vkFreeCommandBuffers` require it. Today every caller is on the main thread so nothing races; if a streaming-upload thread is ever introduced, the pool — not the queue — is the next thing that needs a lock. Out of scope for a finding at HEAD (no concurrent caller exists), noted so it is not rediscovered as a surprise.
4. **`draw.rs` is 4,900+ lines and was read in ranges**, not end to end. I covered the fence/acquire/AS/submit/present spine (`:1590-1800`, `:2550-2760`, `:3000-3060`, `:3320-3420`, `:3640-3990`) plus every `?`/`return Err` in the acquire→submit window. Passes with no queue or AS interaction (material interning, blend-pipeline warm-up, draw sorting) were not read.
5. **`presentation.rs` render-pass attachment layouts** (`initialLayout` / `finalLayout` on the swapchain attachment, and its subpass dependencies) were not verified — that is a render-pass correctness question for the renderer dimension, not queue/AS sync, and #3426 is three days old.

---

## Dimension 2 — Compute → AS → Fragment Chains


Audit: ByroRedux concurrency sweep, 2026-08-30. HEAD `64f64480`. Depth: **deep**.
Repo root: `/mnt/data/src/gamebyro-redux`.

Method: enumerated **every** `cmd_pipeline_barrier` / `memory_barrier` /
`*MemoryBarrier` site reachable from `draw_frame` in command-record order, then
cross-checked each producer/consumer pair against the actual GLSL that
dereferences the resource (not just the include graph). Each candidate below
survived an explicit disproof attempt; 7 candidates did not (see
`## Dropped (stale premise)`).

### Findings

#### CONC-D2-2026-08-30-01: `caustic_splat.comp` reads the skinned-vertex output SSBO from COMPUTE, but the skin chain only publishes it to AS_BUILD | FRAGMENT
- **Severity**: HIGH
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:480-488` (the publish barrier) × `crates/renderer/shaders/caustic_splat.comp:158-160, 205-216` (the unpublished COMPUTE consumer)
- **Status**: NEW — *regression of the #2403 class*, introduced by `9bf7d024` (2026-08-15), one day after `docs/audits/AUDIT_CONCURRENCY_2026-08-14.md` verified this exact barrier clean.
- **Trigger Conditions**: Any frame in which (a) at least one skinned entity dispatched `skin_vertices.comp` (`pose_dirty` non-empty or a first-sight slot), (b) `accel.tlas_handle(frame)` is `Some` so `record_caustic_splat_pass` actually dispatches, and (c) a caustic ray-query committed hit lands on that skinned actor's geometry (`hit.boneOffset != 0 && hit.skinnedVertexAddress != 0`). Concretely: an NPC standing behind glass or near water, in a cell where the caustic pass is live. No CPU-side timing window — it is a pure device-side RAW inside one command buffer.
- **Verification Path**: validation layer. `BYRO_VALIDATION=1` **release** build with `VK_LAYER_KHRONOS_validation` synchronization validation enabled, on a cell with a skinned actor + a caustic source (e.g. FO4 `DmndDugoutInn01`, or any Skyrim interior with water). Expected concrete signal: **`SYNC-HAZARD-READ-AFTER-WRITE`** naming the `SkinSlot::output_buffer` `VkBuffer` at the `vkCmdDispatch` inside `CausticPipeline::dispatch` (`caustic.rs:1058`), with `prior_access = SYNC_COMPUTE_SHADER_SHADER_STORAGE_WRITE` from the `skin_vertices.comp` dispatch. RenderDoc alternative: the same pixel's caustic contribution computed from a previous pose's triangle positions (visible as caustic ghosting trailing a moving NPC). `cargo test` cannot see this.
- **Description**:
  `record_skinned_blas_refit` publishes the `skin_vertices.comp` output with a single global `memory_barrier` whose destination scope is
  `ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER` / `SHADER_READ`.
  `#2403` widened that mask to `FRAGMENT_SHADER` precisely because
  `include/ray_hit.glsl` (reached from `triangle.frag` / `water.frag`)
  dereferences `GpuInstance.skinnedVertexAddress`. The 2026-08-14 audit
  re-traced the include graph and explicitly cleared the missing
  `COMPUTE_SHADER` bit:

  > "`caustic_splat.comp` and `volumetrics_inject.comp` include
  > `include/shadow_common.glsl`, which touches no geometry buffer, so the
  > missing `COMPUTE_SHADER` dst bit on that barrier is **not** a gap today."
  > — `docs/audits/AUDIT_CONCURRENCY_2026-08-14.md:496-501`

  That is no longer true. `caustic_splat.comp` does **not** go through
  `ray_hit.glsl` — commit `9bf7d024` gave it its own inline
  `SkinnedVertexRef` block and its own
  `getCausticHitTriWorldPositions()` that dereferences
  `hit.skinnedVertexAddress` directly. So an include-graph trace still comes
  back clean while the deref exists.

  I walked every barrier between the two dispatches in command order and none
  makes the write visible to `COMPUTE_SHADER` / `SHADER_READ`:
  - `skinned_blas_refit.rs:480` — src `COMPUTE/SHADER_WRITE`, dst `AS_BUILD|FRAGMENT / SHADER_READ`. Correct availability, wrong visibility scope.
  - `skinned_blas_refit.rs:672` and `blas_skinned.rs:695` — `AS_WRITE → AS_WRITE|AS_READ`; src access does not include the compute `SHADER_WRITE`.
  - `draw.rs:2688` — `AS_BUILD/AS_WRITE → FRAGMENT|COMPUTE / AS_READ`. Has the `COMPUTE` dst bit but its **src access is `ACCELERATION_STRUCTURE_WRITE_KHR`**, which does not cover the skin compute write.
  - `draw.rs:2757` / `draw.rs:3612` — src is `HOST`.
  - `draw.rs:2775` (cluster-cull trailing) — src `COMPUTE/SHADER_WRITE` (does cover it), dst `FRAGMENT_SHADER` only. This is the same accidental-cover barrier `#2403` called out, and it still has no `COMPUTE` dst bit.
  - `context/geometry_pass.rs` — contains **zero** barriers; the main render pass's outgoing subpass dependency (`context/helpers.rs:274-312`) has src `COLOR_ATTACHMENT_OUTPUT|EARLY/LATE_FRAGMENT_TESTS`, which cannot cover a compute SSBO write.
  - `caustic.rs:889` (inside `CausticPipeline::dispatch`) — src `HOST/HOST_WRITE`; all other barriers in that function are `VkImageMemoryBarrier`s on the accumulator image.
  - `post_passes.rs:593` is a real global `COMPUTE/SHADER_WRITE → COMPUTE/SHADER_READ` barrier and *would* cover it — but it lives in `record_volumetrics_pass`, which `record_post_passes` calls **after** `record_caustic_splat_pass` (`post_passes.rs:242-243`), and it is gated behind the volumetrics TLAS/cluster/geometry triple.

  Execution ordering *is* established by chaining (`skin compute → AS_BUILD` via `:480`, `AS_BUILD → COMPUTE` via `draw.rs:2688`), so this is a pure memory-visibility gap, not an execution-order gap — the same shape as `#2403`.
- **Evidence**:
  `crates/renderer/shaders/caustic_splat.comp` (the new COMPUTE consumer):
  ```glsl
  158  layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer SkinnedVertexRef {
  159      float data[];
  160  };
  ...
  205      if (hit.boneOffset != 0u && hit.skinnedVertexAddress != 0ul) {
  206          SkinnedVertexRef skinned = SkinnedVertexRef(hit.skinnedVertexAddress);
  207          uint p0 = i0 * SKIN_OUTPUT_STRIDE_FLOATS;
  ...
  211          w0 = vec3(skinned.data[p0], skinned.data[p0 + 1u], skinned.data[p0 + 2u]);
  ```
  `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (the publish barrier, unchanged since `#2403`):
  ```rust
  480                              memory_barrier(
  481                                  &self.device,
  482                                  cmd,
  483                                  vk::PipelineStageFlags::COMPUTE_SHADER,
  484                                  vk::AccessFlags::SHADER_WRITE,
  485                                  vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR
  486                                      | vk::PipelineStageFlags::FRAGMENT_SHADER,
  487                                  vk::AccessFlags::SHADER_READ,
  488                              );
  ```
  Provenance of the new deref:
  ```
  $ git log -S "SkinnedVertexRef" --oneline -- crates/renderer/shaders/caustic_splat.comp
  9bf7d024 refactor: enhance glass IOR handling and ray budget telemetry   (2026-08-15)
  ```
- **Impact**: The caustic splat can compute refracted-light deposits from a skinned actor's *previous-frame* triangle positions (or partially-written positions) on drivers with incoherent compute L1. Visible class: caustic pools that lag / ghost behind a moving NPC near water or glass. Blast radius is bounded — the caustic accumulator is additive and screen-space; it cannot corrupt the AS or cause device loss. The upgrade path is what makes this HIGH rather than MEDIUM: the same barrier is the *only* publish for a buffer that already has three consumers, and each new consumer added without re-auditing this mask silently re-opens the hole.
- **Related**: `#2403` / CHAIN2-D2-01 (the FRAGMENT half of this exact fix); `docs/audits/AUDIT_CONCURRENCY_2026-08-14.md:496-501` (the now-stale clearance); `docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md:278` ("Skin chain … Intact end to end" — that sweep did not re-check `caustic_splat.comp`'s own deref).
- **Suggested Fix**: Add `vk::PipelineStageFlags::COMPUTE_SHADER` to the dst stage mask at `skinned_blas_refit.rs:485-486`, exactly as `#2403` added `FRAGMENT_SHADER`. Widening a dst stage mask is purely additive — it can only add execution/memory dependencies, never remove one — so it is the lowest-risk class of change, but confirm with the sync-val signal above before and after. Consider also adding a source-assert test (mirroring `skin_dispatch_ran_ordering_tests`) that fails if any `.comp` under `shaders/` mentions `skinnedVertexAddress` while the barrier's dst mask lacks `COMPUTE_SHADER` — the include-graph trace demonstrably does not catch an inline deref.

---

#### CONC-D2-2026-08-30-02: `CausticPipeline::clear_for_skip`'s `TRANSFER_WRITE` is never in the source scope of the next dispatch's decay read
- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/caustic.rs:1144-1161` (publish) × `crates/renderer/src/vulkan/caustic.rs:957-972` (the next visit's `pre_decay`)
- **Status**: NEW
- **Trigger Conditions**: For one frame-in-flight slot `f`: frame N skips the caustic dispatch (no TLAS for `f`, or `caustic_failed` latched) so `caustic_skip_clear_decision` fires `clear_for_skip(f)`; frame N+`MAX_FRAMES_IN_FLIGHT` reaches the slot again with `history_valid == true` (camera parked, scene static) and takes the `if history_valid` decay branch, whose shader `imageLoad`s the accumulator. Reachable at cell-load settle: the TLAS is absent for the first frames while the camera has not yet moved.
- **Verification Path**: validation layer. `BYRO_VALIDATION=1` release build; force the window by launching with `--cell <interior with water>` and holding the camera still through load. Expected concrete signal: **`SYNC-HAZARD-READ-AFTER-WRITE`** on the caustic accumulator `VkImage` at the decay `vkCmdDispatch` (`caustic.rs:981`) with `prior_access = SYNC_COPY_TRANSFER_WRITE` (the `vkCmdClearColorImage` in `clear_for_skip`). Not observable via `cargo test`; the visible artifact class (a one-frame stale caustic pool after a load) is too subtle to be a reliable signal on its own.
- **Description**: `clear_for_skip` deliberately publishes only to `FRAGMENT_SHADER` — its own comment says "TRANSFER → FRAGMENT directly (no compute dispatch follows this clear)". That is true *within the frame*, but the slot's **next** visit is `dispatch`, whose `pre_decay` barrier names `COMPUTE_SHADER | FRAGMENT_SHADER` / `SHADER_READ | SHADER_WRITE` as the source scope. `TRANSFER` / `TRANSFER_WRITE` appears in neither the earlier publish's dst nor the later barrier's src, so no dependency chain carries the clear's write to the decay pass's `imageLoad`. The `pre_decay` doc comment enumerates the prior uses it expects ("prior splat compute-write + composite fragment-read") and the `clear_for_skip` path is simply not in that list. Note this is *not* covered by the both-slots fence wait under the codebase's own stated doctrine — `skinned_blas_refit.rs:568-580` explicitly says "the host fence-wait is a host-side dependency only and does NOT establish device-side memory ordering for the next submission".
- **Evidence**:
  ```rust
  // caustic.rs — clear_for_skip publishes to FRAGMENT only
  1144        // TRANSFER → FRAGMENT directly (no compute dispatch follows this
  1145        // clear, unlike `dispatch`'s TRANSFER → COMPUTE mid-barrier).
  1146        let post_clear_barrier = vk::ImageMemoryBarrier::default()
  1147            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
  1148            .dst_access_mask(vk::AccessFlags::SHADER_READ)
  ...
  1155            vk::PipelineStageFlags::TRANSFER,
  1156            vk::PipelineStageFlags::FRAGMENT_SHADER,
  ```
  ```rust
  // caustic.rs — the next visit's pre_decay src scope omits TRANSFER
  957            let pre_decay = vk::ImageMemoryBarrier::default()
  958                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
  959                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
  ...
  966                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
  967                vk::PipelineStageFlags::COMPUTE_SHADER,
  ```
  ```glsl
  // caustic_splat.comp:241-243 — the decay pass reads the accumulator
  241  if (pc.decayOnly == 1u) {
  242      for (int channel = 0; channel < 3; ++channel) {
  243          uint v = imageLoad(causticAccum, ivec3(pixel, channel)).r;
  ```
- **Impact**: The decay pass can scale the pre-clear accumulator contents instead of zeros, so a caustic pool that `#2507` intended to clear survives one extra slot visit. Purely cosmetic and self-correcting (the EMA converges), which is why this is MEDIUM and not HIGH — but it is the kind of gap that becomes load-bearing the moment `MAX_FRAMES_IN_FLIGHT` is raised or the both-slots fence wait is relaxed (the relaxation `#653` and `svgf.rs:1256-1268` both anticipate).
- **Related**: `#2507` (the skip-clear this weakens); `#653` (the "fence currently serialises this, but the mask must still be right" precedent, already applied in `taa.rs:781` and `svgf.rs:1315`).
- **Suggested Fix**: Add `vk::PipelineStageFlags::TRANSFER` / `vk::AccessFlags::TRANSFER_WRITE` to the source scope of `dispatch`'s `pre_decay` and `pre_clear_barrier` (`caustic.rs:957-972`, `:1002-1015`) — or, equivalently and more locally, widen `clear_for_skip`'s `post_clear_barrier` dst to `FRAGMENT_SHADER | COMPUTE_SHADER`. Both are additive-only mask widenings; confirm with the sync-val signal before/after.

---

#### CONC-D2-2026-08-30-03: `VolumetricsPipeline::record_neutral_frame`'s clear is not in the source scope of the next dispatch's `pre_int_write`
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2594-2609` (publish) × `crates/renderer/src/vulkan/volumetrics.rs:2314-2329` (`pre_int_write`)
- **Status**: NEW
- **Trigger Conditions**: Slot `f` takes `record_neutral_frame` on frame N (no TLAS / no `cluster_cull` / no global geometry / `!requires_dispatch`), then the same slot takes the full `dispatch` path on frame N+`MAX_FRAMES_IN_FLIGHT`. Always hit at scene load, since the first frames have no TLAS.
- **Verification Path**: validation layer. Same harness as `-02`. Expected concrete signal: **`SYNC-HAZARD-WRITE-AFTER-WRITE`** on the `integrated_volumes[f]` `VkImage` at the integration `vkCmdDispatch` (`volumetrics.rs:2349`) with `prior_access = SYNC_COPY_TRANSFER_WRITE`.
- **Description**: Exact structural sibling of `-02` on the volumetrics side. `record_neutral_frame` clears `integrated_volumes[frame]` and publishes `TRANSFER_WRITE → FRAGMENT_SHADER/SHADER_READ` (composite's sampler3D). The next full `dispatch` at that slot guards the integrated volume with `pre_int_write`, whose source scope is `COMPUTE_SHADER | FRAGMENT_SHADER` / `SHADER_READ` — it names the composite *read* but not the neutral *clear write*. A repeat `record_neutral_frame` on the same slot has the same hole in its own `to_clear` barrier (`:2569-2584`, src `COMPUTE|FRAGMENT / SHADER_READ|SHADER_WRITE`).
- **Evidence**:
  ```rust
  // record_neutral_frame — publishes only to FRAGMENT
  2594        let to_sample = vk::ImageMemoryBarrier::default()
  2595            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
  2596            .dst_access_mask(vk::AccessFlags::SHADER_READ)
  ...
  2603            vk::PipelineStageFlags::TRANSFER,
  2604            vk::PipelineStageFlags::FRAGMENT_SHADER,
  ```
  ```rust
  // dispatch — pre_int_write src scope omits TRANSFER_WRITE
  2314        let pre_int_write = vk::ImageMemoryBarrier::default()
  2315            .src_access_mask(vk::AccessFlags::SHADER_READ)
  2316            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
  ...
  2323            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
  2324            vk::PipelineStageFlags::COMPUTE_SHADER,
  ```
- **Impact**: LOW, not MEDIUM, because unlike `-02` this is a **write-after-write where the later write fully covers the image** — `volumetrics_integrate.comp` is dispatched one thread per `(x, y)` column and Z-marches every slice, so every froxel is overwritten. There is no read of stale data; only the *order* of two writes to the same memory is formally undefined. Reported for symmetry with `-02` and because sync-val will flag it in the same session.
- **Related**: CONC-D2-2026-08-30-02 (same pattern, RAW instead of WAW).
- **Suggested Fix**: Add `TRANSFER` / `TRANSFER_WRITE` to the source scope of `pre_int_write` and `record_neutral_frame`'s own `to_clear` — additive-only. Lower priority than `-02`; fix both in one pass if `-02` is taken.

---

### Dropped (stale premise)

**7 candidates dropped.**

1. **`MorphSlot::weight_buffer` host write has no `HOST_WRITE → COMPUTE_SHADER/SHADER_READ` barrier before `skin_vertices.comp` reads it.** True as stated (`flush_pending_morph_weights` at `draw.rs:1640`; the first `HOST →` barrier in the command buffer is `draw.rs:2757`, *after* the skin dispatch). Dropped: every one of those writes happens before `vkQueueSubmit`, and the Vulkan submit carries an implicit host-write availability/visibility operation covering all device accesses in the batch. The `HOST →` barriers elsewhere in `draw.rs` are belt-and-braces, not load-bearing. No hazard.
2. **Volumetrics `tlas_written` latch asymmetry on the gated-off / early-return paths.** Re-read `post_passes.rs:480-733`: `write_tlas` → `write_boundary_geometry` → barrier → `write_lights_and_clusters` → `dispatch` is one straight-line block with no early return and no `?`; both `record_neutral_frame` arms call neither. Inside `dispatch`, all three `debug_assert!` + reset pairs run before any `?` (`volumetrics.rs:2060-2081`, first `?` at `:2159`), so a failed dispatch cannot leave a latch set. Set/reset is symmetric.
3. **Bloom's final up-mip (`up_mips[0]`) has no publish barrier.** `bloom.rs:699-712` emits a post barrier for **every** `i` in the up chain including `i == 0`; `#2796` moved its consumer from composite's fragment read to `apply_to_scene`'s compute dispatch, and the `dst_stage` was correspondingly narrowed to `COMPUTE_SHADER` — which matches the actual consumer. Down chain likewise emits a post barrier per mip (`:658-677`). No missing publish. (Barriers *around `scene_color`* on the FSR color-input path are `#3247`, already filed — not re-reported.)
4. **MaterialBuffer SSBO upload has moved into a mid-frame compute path (R1).** `material.rs` contains **zero** `vk::CommandBuffer`, `cmd_dispatch`, `cmd_copy_buffer` or `cmd_pipeline_barrier` references — it is a pure CPU table. `upload_materials` (`scene_buffer/upload.rs:640`) is a mapped host write called at `draw.rs:3328`, before the `HOST → VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT` barrier at `draw.rs:3612` and before any draw recording. Premise does not hold.
5. **A raster-from-skinned-SSBO path was added without a `VERTEX_SHADER` / `VERTEX_INPUT` barrier.** `triangle.vert:58`, `water.vert:68` and `ui.vert` all annotate `skinnedVertexAddress` as *"offset 112 — #2219, unused here"*; `triangle.vert:211-229` inline-skins from the palette SSBO (binding 3) and `bones_prev` (binding 12). The checklist's standing note still holds.
6. **Cross-frame ping-pong uses the `(f + 1) % N` form that aliases at `N > 2`.** All four sites use the general previous-slot form: `svgf.rs:827`, `taa.rs:548`, `restir.rs:111`, `volumetrics.rs:2204`. SVGF carries a `const _: () = assert!(MAX_FRAMES_IN_FLIGHT >= 2)` gate (`svgf.rs:80-84`) and ReSTIR a runtime `assert_ne!` test (`restir.rs:405-407`). The one surviving `(f + 1) % N` is the *fence-wait* index at `draw.rs:1625`, which is `#3442` and already filed.
7. **SSAO write-after-read: the main render pass reads `ao[frame]` (FRAGMENT) and the SSAO dispatch overwrites the same slot later in the same command buffer.** The WAR is real but guarded — `ssao.rs:536-546` emits exactly `FRAGMENT_SHADER/SHADER_READ → COMPUTE_SHADER/SHADER_WRITE` before the dispatch, and `:571-582` closes with `COMPUTE_SHADER/SHADER_WRITE → FRAGMENT_SHADER/SHADER_READ`.

### Verified clean

1. **Skin chain (M29), palette half** — `record_bone_world_copy` (`scene_buffer/upload.rs:296-312`) and `record_pending_bind_inverse_copies` (`:428-443`) each carry their own `TRANSFER_WRITE → SHADER_READ` / `TRANSFER → COMPUTE_SHADER` barrier; the palette dispatch's trailing `SHADER_WRITE → SHADER_READ` buffer barrier (`draw.rs:2578-2597`) has `dst_stage = COMPUTE_SHADER | VERTEX_SHADER`, which matches the *complete* consumer set — `skin_vertices.comp:193-209` (COMPUTE) and `triangle.vert:221-229` + `bones_prev` (VERTEX). No FRAGMENT or COMPUTE consumer of the palette exists.
2. **Skin chain, skin → AS → TLAS → ray-query half** — intact end to end *except* the COMPUTE consumer in finding `-01`. `COMPUTE/SHADER_WRITE → AS_BUILD|FRAGMENT/SHADER_READ` (`skinned_blas_refit.rs:480`) → shared-scratch `AS_WRITE → AS_WRITE|AS_READ` self-emitted per build/refit (`blas_skinned.rs:695-703`, the `#1790` guard) → `AS_WRITE → AS_READ` refit→TLAS handoff (`skinned_blas_refit.rs:672-679`) → `AS_WRITE → FRAGMENT|COMPUTE / AS_READ` on **both** the success and failure arms (`draw.rs:2688-2696`, the `#2931` guard, comment still in place). AS build *inputs* still correctly use `SHADER_READ` rather than `AS_READ` (`#1436`).
3. **Volumetrics gate (#1105)** — `tlas_written` / `lights_written` / `boundary_geometry_written` set/reset symmetric on every path (see Dropped #2). Injection → integration → composite chain fully barriered: pre-inject 10-image WAR/RAW batch (`volumetrics.rs:2268-2287`), `inj_to_int` + `pre_int_write` (`:2321-2329`), `post_int` `COMPUTE → FRAGMENT` (`:2355-2363`), plus a `COMPUTE_WRITE → HOST_READ` publish for binding 18 (`:2368`). The `COMPUTE → COMPUTE` barrier for cluster-grid reuse (`post_passes.rs:593-601`) is present with the correct rationale comment — the codebase already knows this pattern, which is what makes `-01` an oversight rather than a design gap.
4. **Bloom within-frame RAW chain (#931)** — see Dropped #3. Post-barrier-on-just-written-mip accounting is complete for both chains; `apply_to_scene`'s `SHADER_READ_ONLY_OPTIMAL → GENERAL → SHADER_READ_ONLY_OPTIMAL` round trip names all three producing stages in the first transition's source mask. Deduped against `#3247`.
5. **Caustic CLEAR → COMPUTE → FRAGMENT (within a frame)** — the moving-camera path is `COMPUTE|FRAGMENT → TRANSFER` pre-clear, `cmd_clear_color_image`, `TRANSFER → COMPUTE` post-clear, splat dispatch, `COMPUTE → FRAGMENT` publish (`caustic.rs:1002-1080`). The parked path substitutes `pre_decay` → decay dispatch → `mid` (`COMPUTE_WRITE → COMPUTE_READ|WRITE` for the `atomicAdd` RMW) → splat. Intra-frame ordering is complete; only the *cross-frame* `clear_for_skip` seam is finding `-02`.
6. **MaterialBuffer SSBO (R1)** — see Dropped #4.
7. **Master ordering in `draw.rs` / `record_post_passes`** — walked `svgf → caustic → volumetrics → taa → ssao → composite → bloom → upscale → presentation` (`post_passes.rs:241-283`) against every pass's inputs. Each consumer's producer runs earlier: composite samples SVGF/TAA/caustic/volumetrics/water-caustic outputs, all published before it; bloom runs after composite by design (`#2796`) and the composite render pass's outgoing dependency already names `COMPUTE_SHADER | TRANSFER` in `dst_stage_mask` (`composite.rs:639-647`), so bloom's compute sample of `scene_color` is covered. The main render pass's outgoing dependency names `FRAGMENT_SHADER | COMPUTE_SHADER` (`context/helpers.rs:309-312`), covering SSAO/SVGF/TAA/caustic reads of the G-buffer. Water-caustic accumulator: `clear_pre_render_pass` before `vkCmdBeginRenderPass` (`draw.rs:3638`), `barrier_post_render_pass` as the first statement of `record_svgf_pass` (`post_passes.rs:314-316`), consumer is `composite.frag` only (FRAGMENT), so the `FRAGMENT → FRAGMENT` mask is correct.
8. **Per-frame descriptor freshness for the RT geometry bindings** — `draw.rs:1824-1845` re-points *both* `scene_buffers` bindings 8/9 and `caustic`'s bindings 9/10 at the live global vertex/index SSBO every frame, so a `rebuild_geometry_ssbo` cannot leave the caustic pass dereferencing a deferred-destroy buffer.
9. **SVGF à-trous ping-pong** — `atrous_dst_pp(k) = k % 2` / `atrous_src_pp(k) = (k-1) % 2` with a `const _: () = assert!(ATROUS_ITERATIONS % 2 == 1)` gate; each iteration's post barrier is `COMPUTE → COMPUTE` except the last, which adds `FRAGMENT` (`svgf.rs:1367-1387`). WAR between iterations is covered by the execution dependency, as the comment claims.
10. **TAA** — `pre` WAR barrier on the output slot, `post` publish with `dst = FRAGMENT | COMPUTE` (the `#653` mask that survives a fence relaxation) (`taa.rs:722-786`).
11. **Exposure** — a persistent 1×1 `R32_SFLOAT` written once in `initialize()` via `with_one_time_commands` and published to `COMPUTE_SHADER | FRAGMENT_SHADER` (`exposure.rs:204-217`). No per-frame write, no hazard.
12. **`morph_compute.rs`** — no GPU dispatch of its own; `MorphSlot` holds no descriptor sets, both buffers are reached via `buffer_reference`. The `#3244` staging fix is still in place (`flush_pending_morph_weights` at `draw.rs:1640`, after the dual-fence wait at `:1624-1636`).

### Coverage gaps

- **No GPU was available.** Every finding here is a static-analysis result. Per the audit's no-speculative-Vulkan rule, none of `-01`/`-02`/`-03` should be shipped on this reasoning alone — each names the exact sync-validation hazard string that must be observed first. `-01` in particular needs a scene that actually satisfies all three trigger conditions simultaneously (skinned actor + live TLAS + caustic-source hit); if the sync-val run does not produce the hazard, the most likely reason is that the caustic ray-query never commits a hit on skinned geometry in the test cell, not that the barrier is adequate.
- **FSR / `frame_upscaler.rs` / `presentation.rs` internals not audited.** `record_upscale_pass` hands `scene_color` to the FFX SDK, whose internal barriers are opaque to source reading, and the surrounding `scene_color` barriers are already `#3247`. Deferred rather than duplicated.
- **`egui_pass.rs` and `screenshot.rs`** were only checked for barrier *presence* in the master-ordering walk, not audited as chains — they are outside this dimension's entry points.
- ~~`.spv` / `.glsl` drift for `caustic_splat.comp`~~ — **closed, not a gap.** The committed `caustic_splat.comp.spv` (mtime one minute after the `.glsl`) contains the `SkinnedVertexRef` and `skinnedVertexAddress` `OpName`s, so the deref in finding `-01` is live in the binary the engine actually loads via `include_bytes!`, not GLSL-only.

---

## Dimension 3 — ECS Lock Ordering & Deadlock


Audit: ByroRedux concurrency sweep, 2026-08-30. HEAD `64f64480`. Depth: **deep**.

Method: read `world.rs` / `lock_tracker.rs` / `query.rs` / `resource.rs` /
`storage.rs` at HEAD against checklist items 1–5 and 7; then a scripted
lock-order-graph extraction over `byroredux/src`, `crates/core`,
`crates/scripting`, `crates/physics`, `crates/debug-server`, `crates/save`,
`crates/ui`, `crates/plugin` — per-function ordered acquisition sequences
(`query`/`query_mut`/`query_2*`/`resource*`/`try_resource*`/`get`/`has`/`count`),
guard-liveness modelling (block scoping for `if let`/`match` arms, explicit
`drop(g)`, statement-temporary detection), plus a transitive
acquires-closure over the intra-crate call graph so a guard held *across a
call* records edges for everything the callee locks. Every surviving
bidirectional pair was then read by hand at HEAD before being reported.

---

### Findings

#### CONC-D3-2026-08-30-01: `skin.dump` holds `SkinnedMesh` across `format_skin_dump`'s `GlobalTransform` read — the console half of #2388 was never fixed
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/commands/assets.rs:712-715` (callee at `byroredux/src/commands/shared.rs:90-138`)
- **Status**: NEW
- **Trigger Conditions**: Debug build with `BYRO_LOCK_ORDER_CHECK=1`. Run any
  frame (so `make_world_bound_propagation_system` records
  `GlobalTransform → SkinnedMesh`), then `skin.dump <id>` in `byro-dbg`. The
  console command records the reverse edge and `global_order::record_and_check`
  panics on whichever observation lands second. Order of the two is
  irrelevant — the cycle closes either way.
- **Verification Path**: `cargo test` cannot see this (the detector is opt-in).
  Concrete confirming signal: a debug run with `BYRO_LOCK_ORDER_CHECK=1` and
  the panic message `ECS cross-thread deadlock risk (lock-order cycle):
  attempted acquisition of ... while holding ...` naming `SkinnedMesh` and
  `GlobalTransform`.
- **Description**: `docs/engine/ecs.md:600-604` fixes the process-wide order
  `… → GlobalTransform → SkinnedMesh → MeshHandle → …`. `SkinDumpCommand::execute`
  binds the `ComponentRef<SkinnedMesh>` returned by `world.get::<SkinnedMesh>`
  through a `let … else`, so the read guard lives to the end of the function,
  and then calls `format_skin_dump`, which acquires `GlobalTransform` (and
  `Name`, `StringPool`) per bone with that guard still live. That is
  `SkinnedMesh → GlobalTransform`, the inverse of the canonical order.
  `#2388` fixed exactly this inversion in the debug-server sibling
  `eval_inspect_skinned_mesh` — whose comment at
  `crates/debug-server/src/evaluator.rs:255-262` states the fix — but the
  console-command sibling that goes through `format_skin_dump` was missed.
- **Evidence**:
```rust
// byroredux/src/commands/assets.rs:712-715 — SkinnedMesh guard lives past the call
let Some(skin) = world.get::<SkinnedMesh>(entity) else {
    return CommandOutput::line(format!("Entity {} has no SkinnedMesh component", entity));
};
let lines = format_skin_dump(world, entity, &skin);
```
```rust
// byroredux/src/commands/shared.rs:136-138 — inside that call, with `skin` still held
let world_mat = world
    .get::<GlobalTransform>(*bone_e)
    .map(|gt| gt.to_matrix());
```
```rust
// crates/debug-server/src/evaluator.rs:262-264 — the canonical direction, post-#2388
let gt_q = world.query::<GlobalTransform>();
let Some(skin_q) = world.query::<SkinnedMesh>() else { … };
```
  Opposing `GlobalTransform → SkinnedMesh` edges exist at
  `byroredux/src/systems/bounds.rs:135-138`
  (`make_world_bound_propagation_system`, a `Stage::PostUpdate`
  `add_exclusive_with_access` system that runs every frame),
  `crates/debug-server/src/evaluator.rs:262-263` and `:349-350`, and
  `build_skinned_palettes` (`byroredux/src/render/skinned.rs`, named as the
  canonical establisher in `docs/engine/ecs.md:608-610`).
- **Impact**: No live deadlock today — every opposing site is either an
  exclusive system or main-thread render collection, so the hold periods
  cannot overlap. The concrete cost is the one `docs/engine/ecs.md:643-649`
  names: "an inverted pair that is *safe* still aborts a debug build once both
  sites run". `skin.dump` and `walk` both dispatch from the same
  `DebugDrainSystem`, so this is the literal #2388 reproduction with one
  command name changed. It also erodes the invariant that would make a future
  promotion of `world_bound_propagation` to a parallel lane safe by
  construction.
- **Related**: #2388 (fixed the debug-server half), #3445, #3446, ECS-D1-01 in
  `docs/audits/AUDIT_ECS_2026-08-30.md` (same class).
- **Suggested Fix**: Snapshot before acquiring — clone the `SkinnedMesh` (or
  just its `bones` / `bind_inverses` / `skeleton_root` / `global_skin_transform`)
  into an owned local and drop the guard before calling `format_skin_dump`.
  `format_skin_dump` already takes `&SkinnedMesh`, so passing `&owned` is a
  two-line change with no behaviour difference.

#### CONC-D3-2026-08-30-02: `save::validate_animation` takes `AnimationPlayer → AnimationClipRegistry`, inverting `animation_system_inner`'s documented outermost-lock order
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `crates/save/src/validate.rs:336-344` (opposing edge at `byroredux/src/systems/animation.rs:530-532` + `:604` / `:619`)
- **Status**: NEW
- **Trigger Conditions**: Debug build with `BYRO_LOCK_ORDER_CHECK=1`: any frame
  runs `animation_system_inner` and records
  `AnimationClipRegistry → AnimationPlayer`; a subsequent `save` (or any load
  through `restore_world`, `crates/save/src/driver.rs:168`) records the
  reverse and closes the cycle.
- **Verification Path**: opt-in detector panic naming `AnimationClipRegistry`
  and `AnimationPlayer`; not reachable by `cargo test`.
- **Description**: `animation_system_inner` documents `AnimationClipRegistry`
  and `NameIndex` as "the two outermost locks" (#2400) and holds the registry
  read guard for the whole function, acquiring `AnimationPlayer` **for write**
  twice underneath it. `validate_animation` does the opposite: it takes the
  `AnimationPlayer` read guard through a `let … else` (so it lives across the
  whole loop) and then acquires `AnimationClipRegistry`, using both together in
  the loop body. `docs/engine/ecs.md`'s canonical table has no entry for this
  pair, so nothing but the local `#2400` comment records the intended
  direction — and the save crate never saw it.
- **Evidence**:
```rust
// byroredux/src/systems/animation.rs:530-532 — registry first, held to end of fn
let Some(registry) = world.try_resource::<AnimationClipRegistry>() else {
    return;
};
// :604 and :619 — AnimationPlayer (WRITE) acquired underneath it
let Some(player_query) = world.query_mut::<AnimationPlayer>() else { return; };
let mut player_query = world.query_mut::<AnimationPlayer>().unwrap();
```
```rust
// crates/save/src/validate.rs:337-343 — AnimationPlayer first, registry second
let Some(q) = world.query::<AnimationPlayer>() else {
    return;
};
let registry = world.try_resource::<AnimationClipRegistry>();

for (entity, player) in q.iter() {
    if let Some(reg) = registry.as_ref() {
```
- **Impact**: No live deadlock today: `make_animation_system` is the parallel
  side (`boot.rs:1009` `add_to_with_access`), but every `validate_world` caller
  is constrained to a quiescent lane — the `save` command's own comment
  (`byroredux/src/save_io.rs:775-786`, #3113/#2154) documents this. The costs
  are (a) a debug-build detector abort once both sites run, and (b) a latent
  edge that becomes a genuine ABBA the moment any validation path moves onto a
  live scheduler lane — and the edge is write-vs-read, so it is a hard blocking
  edge, not a reader-reader one.
- **Related**: #2400, #3113, #2154; ECS-D1-01 (`docs/audits/AUDIT_ECS_2026-08-30.md`).
- **Suggested Fix**: In `validate_animation`, acquire `AnimationClipRegistry`
  before `AnimationPlayer` (one statement moved above the `let … else`),
  matching `animation_system_inner`'s documented outermost-lock order, and add
  the pair to the canonical table (see CONC-D3-2026-08-30-04).

#### CONC-D3-2026-08-30-03: `scene.show` takes `SceneRegistry → ScenePlayer`; `actor_quest_trigger_is_in_sequence` takes `ScenePlayer → SceneRegistry`
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/commands/quest.rs:474-491` and `crates/scripting/src/trigger.rs:341-346`
- **Status**: NEW
- **Trigger Conditions**: Debug build with `BYRO_LOCK_ORDER_CHECK=1`: any frame
  in which a trigger volume is entered runs
  `actor_quest_trigger_is_in_sequence` (`trigger.rs:296`) and records
  `ScenePlayer → SceneRegistry`; a `scene.show <formid>` from `byro-dbg` then
  records the reverse and panics.
- **Verification Path**: opt-in detector panic naming `ScenePlayer` and
  `SceneRegistry`; not reachable by `cargo test`.
- **Description**: Both sites bind through `let … else` / `world.get`, so both
  guards stay live while the pair is used together. The two orders are exact
  opposites and neither pair member appears in `docs/engine/ecs.md`'s canonical
  order table.
- **Evidence**:
```rust
// crates/scripting/src/trigger.rs:341-346 — ScenePlayer → SceneRegistry
let Some(players) = world.query::<crate::ScenePlayer>() else { return true; };
let Some(registry) = world.try_resource::<crate::SceneRegistry>() else { return true; };
```
```rust
// byroredux/src/commands/quest.rs:474-491 — SceneRegistry → ScenePlayer
let Some(registry) = world.try_resource::<SceneRegistry>() else { … };
let Some(definition) = registry.definition(form_id) else { … };
let Some(entity) = registry.scene_entity(form_id) else { … };
let Some(player) = world.get::<ScenePlayer>(entity) else { … };
…
one_line_text(&definition.editor_id, 80)   // `registry` still borrowed here
```
- **Impact**: No live deadlock — `trigger_detection_dispatch` is
  `add_exclusive(Stage::Update)` (`boot.rs:877`) and `scene.show` runs in the
  exclusive `DebugDrainSystem`, so the holds cannot overlap. The concrete cost
  is the debug-build detector abort, plus the safety being entirely
  circumstantial: promoting `trigger_detection_system` to
  `add_to_with_access` is a one-line change with no compile-time or test-time
  signal (`docs/engine/ecs.md:639-643`).
- **Related**: #2388, #3445; CONC-D3-2026-08-30-04.
- **Suggested Fix**: Snapshot in `scene.show` — clone the `SceneDefinition`
  fields (or just the `editor_id`) and the `scene_entity` id, `drop(registry)`,
  then `world.get::<ScenePlayer>`. That makes the command's order
  `SceneRegistry`-only, leaving `trigger.rs`'s
  `ScenePlayer → SceneRegistry` as the single recorded direction.

#### CONC-D3-2026-08-30-04: the canonical order table covers only the hierarchy/physics/CHARAL clusters — three other multi-lock clusters have no documented direction
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering & Deadlock (doc gap / root cause)
- **Location**: `docs/engine/ecs.md:598-636`
- **Status**: NEW
- **Trigger Conditions**: n/a (documentation).
- **Verification Path**: `cargo test` — none; read-only doc check.
- **Description**: The table gives one total order for
  `CharacterController → … → Name → StringPool` plus the `CharacterRuleset →
  ActorValues` pair (#3441). It has no entry for three clusters that live
  systems demonstrably hold together:
  (a) `AnimationClipRegistry` / `NameIndex` / `AnimationPlayer` — the #2400
  "two outermost locks" invariant exists only as a comment at
  `byroredux/src/systems/animation.rs:527-529` and `:598-600`;
  (b) `ScenePlayer` / `SceneRegistry` / `QuestStageState` /
  `QuestAdvanceOnActivate` — no documented direction anywhere;
  (c) `CollisionShape` / `RigidBodyData` / `ActorBoneCollider` /
  `PhysicsWorld` — documented only as a local comment at
  `crates/physics/src/sync.rs:840-842`.
  Findings 02 and 03 above are direct consequences of (a) and (b): a second
  crate reconstructed an order from scratch and got the opposite one. This is
  the same shape as the 2026-08-24 finding that the table omitted
  `CharacterController`/`RapierHandles`, which was fixed by adding them.
- **Evidence**:
```
docs/engine/ecs.md:600-604
CharacterController → RapierHandles → Transform → Parent → Children
                    → GlobalTransform → SkinnedMesh → MeshHandle
                    → LocalBound → WorldBound → Name → StringPool
```
  No `AnimationClipRegistry`, `NameIndex`, `AnimationPlayer`, `ScenePlayer`,
  `SceneRegistry`, `CollisionShape`, `RigidBodyData` or `ActorBoneCollider`
  appears anywhere in that section.
- **Impact**: The knowledge exists in three local comments; the arbiter the
  audit checklist points at does not carry it. Every new consumer of those
  types re-derives an order by guesswork.
- **Related**: #2400, #2388, CONC-D3-2026-08-30-02, CONC-D3-2026-08-30-03.
- **Suggested Fix**: Add the three clusters to the canonical table with the
  direction the per-frame system already establishes
  (`AnimationClipRegistry → NameIndex → AnimationPlayer`;
  `ScenePlayer → SceneRegistry`;
  `RapierHandles → CollisionShape → RigidBodyData → GlobalTransform →
  ActorBoneCollider`), citing the establishing site for each as the existing
  entries do.

#### CONC-D3-2026-08-30-05: #3445's blast radius is wider than filed — `mat.list` supplies a second opposing edge, `Material ↔ StringPool`
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/studio_host.rs:12-25` (filed site) and `byroredux/src/commands/scene.rs:526-540`, `:583-598` (newly identified opposing sites)
- **Status**: **Existing: #3445** — not a separate issue; recorded so the fix
  is verified against both pairs, not just `Name ↔ StringPool`.
- **Trigger Conditions**: Debug build with `BYRO_LOCK_ORDER_CHECK=1`;
  `mat.list` (or `mat.get`) followed by any SDK studio snapshot, in either
  order.
- **Verification Path**: opt-in detector panic naming `Material` and
  `StringPool`.
- **Description**: #3445 reports that `studio_host::snapshot` holds
  `StringPool` (line 13) across `world.get::<Transform>` / `Name` / `Material`,
  inverting the canonical `Name → StringPool` tail. The same hold also records
  `StringPool → Material` and `StringPool → Transform`. `mat.list`
  (`commands/scene.rs:527`) holds `QueryRead<Material>` across
  `resolve_entity_name` (`commands/shared.rs:36-39`, `Name → StringPool`),
  producing `Material → StringPool` — a second closed 2-cycle beyond the one
  named in the issue. `eval_walk_entity` (`evaluator.rs:346-353`) supplies
  `Transform → StringPool`, closing a third against `StringPool → Transform`.
- **Evidence**:
```rust
// byroredux/src/studio_host.rs:12-25 — pool held across Transform/Name/Material
let pool = world.try_resource::<StringPool>();
…
let transform = world.get::<Transform>(entity)?;
let name = world.get::<byroredux_core::ecs::Name>(entity) …
let material = world.get::<Material>(entity).map(|material| MaterialValue { …
```
```rust
// byroredux/src/commands/scene.rs:527-540 — Material held across Name→StringPool
let Some(q) = world.query::<Material>() else { … };
…
for (e, m) in rows {
    let name = resolve_entity_name(world, e).unwrap_or_else(|| "-".to_string());
```
- **Impact**: The one-site fix proposed on #3445 (snapshot/drop `pool` before
  the per-entity `world.get` calls) resolves all three cycles at once — but the
  issue text only names one, so a partial fix that reorders `Name`/`StringPool`
  without dropping the pool guard would leave `Material ↔ StringPool` and
  `Transform ↔ StringPool` live.
- **Related**: #3445, #3446, CONC-D3-2026-08-30-01.
- **Suggested Fix**: Amend #3445 to require the guard be *dropped* before the
  per-entity loop (not merely reordered), and to name all three pairs in its
  acceptance criteria.

---

### Overlap with the same-day sibling audit (not re-filed)

`docs/audits/AUDIT_ECS_2026-08-30.md` (dated today, findings not yet issues)
already carries three candidates this dimension independently reproduced.
They are **not** re-reported above:

- **ECS-D1-01** — `scene_centroid_distance` (`byroredux/src/app_step.rs:410-411`)
  takes `MeshHandle → GlobalTransform`, inverting `static_meshes.rs:99-100`.
  Independently confirmed at HEAD; both guards live, no `drop` between them.
- **ECS-D1-02** — `track_read`'s recursive-read fast path returns before
  `global_order::record_and_check` (`crates/core/src/ecs/lock_tracker.rs:100-109`),
  so an order edge established while the incoming type is already read-held is
  never recorded. Independently confirmed at HEAD.
- **ECS-P2-01 / ECS-P2-02** — `combat_input_system`'s `CombatState` write guard
  across `attack_cooldown_seconds`, and `collect_candidates`' scratch write
  guard across `populate_candidates`. Confirmed; note that the *cycle* reading
  of `combat.rs` is false (see Dropped #1) — the finding as filed (single
  hold-across-call, no reverse edge, MEDIUM) is the accurate one.

---

### Dropped (stale premise)

**12 candidate findings dropped** after re-reading the file at HEAD.

1. `combat_input_system` `CombatState ↔ RapierHandles` "2-cycle"
   (`byroredux/src/combat.rs:120/159/237`) — both guards are block-scoped
   (`if let Some(mut state) = …` arm; `match world.query::<RapierHandles>()`
   arm). The file's own comment at `:156-158` states the non-overlap
   intentionally. No cycle.
2. `character_controller_system` holding `PhysicsWorld` across
   `query_mut::<Transform>` / `set_kinematic_translation`
   (`byroredux/src/systems/character.rs:334 → 450/498`) — `drop(pw)` at
   `:386`.
3. `register_newcomers` holding `PhysicsWorld` across `query_mut::<RapierHandles>`
   (`crates/physics/src/sync.rs:890 → 1035`) — `drop(pw)` at `:1019`.
4. `activate_ragdoll` `RapierHandles → PhysicsWorld`
   (`byroredux/src/ragdoll.rs:414 → 424`) — explicit two-phase collect; the
   `match world.query::<RapierHandles>()` arm closes before the
   `resource_mut::<PhysicsWorld>()` block opens.
5. `World::find_by_name` `StringPool → Name` (`crates/core/src/ecs/world.rs:428-432`)
   — explicit `drop(pool)` at `:430`.
6. `animation_system_inner` `Name → NameIndex` / `Name → Transform`
   (`byroredux/src/systems/animation.rs:550`) — `world.query::<Name>().map(|q| q.len())`
   is a statement temporary; the binding is a `usize`. The file's `:540-549`
   comment documents the deliberate NameIndex-before-Name discipline.
7. `camera_ray` `ActiveCamera → Transform/GlobalTransform`
   (`byroredux/src/interaction.rs:843-849`) — `try_resource::<ActiveCamera>()?.0`
   copies the id out; the guard dies at the end of that statement.
8. `mesh.info` (`byroredux/src/commands/assets.rs:188-438`) apparent 14-type
   hold chain inverting `GlobalTransform → Parent` and `MeshHandle → SkinnedMesh`
   — every `world.get::<T>` is the scrutinee of an `if let`, dropped at the end
   of its own block. Fully sequential.
9. `refresh_scene_actor_bindings` holding `SceneActorBindings`
   (`crates/scripting/src/scene/quest_alias.rs:527`) — the expression is
   `world.resource::<SceneActorBindings>().actors.clone()`; the guard is a
   statement temporary and `resolved` is an owned map.
10. `assemble_camera` / `snap_character_body_to_camera` holding `ActiveCamera`
    across `Transform` (`byroredux/src/render/camera.rs:136-141`,
    `byroredux/src/systems/character.rs:671-679`) — explicit `drop(active)` /
    id copied out of the match arm.
11. `entities` command `MeshHandle → Transform`
    (`byroredux/src/commands/world_info.rs:88 → 107`) — `world.count::<T>()`
    returns a `usize`; no guard survives the statement. (The real edge there,
    `CollisionShape → MeshHandle` at `:94-96`, has no opposing site.)
12. **`ActorValues` / `CharacterRuleset` cycle (#3441) — NOT regressed.**
    `crates/scripting/src/condition.rs:470-500` still snapshots `ActorValues`
    into an owned clone and drops the guard (and reads `CharacterLevel`) before
    `try_resource::<CharacterRuleset>()` at `:500`; the pinning tests at
    `:1487-1520` (source-order assertion) and `:1527-1556` (detector-driven)
    are both present. Nothing to report.

---

### Verified clean

- **Checklist 1 — TypeId-sorted acquisition.** All four multi-lock accessors in
  `crates/core/src/ecs/world.rs` branch on `id_a < id_b` and set up the
  `lock_tracker` scopes in the *same* TypeId-ascending order as the real
  `read()`/`write()` calls: `query_2_mut` (`:528-580`), `query_2_mut_mut`
  (`:598-645`), `resource_2_mut` (`:764-820`). `try_resource_2_mut`
  (`:888-905`) does both existence checks before delegating to
  `resource_2_mut`, so it inherits the ordering (#465). Each has its
  `assert_ne!(TypeId::of::<A>(), TypeId::of::<B>(), …)` same-type guard. The
  #2149 "construct both wrappers, then defuse both" discipline is intact in all
  four. There are no `query_3*` / `resource_3*` accessors in the workspace, so
  nothing was missed.
- **Checklist 2 — static proof + tracker coverage.** `install_runtime_registries`
  (`byroredux/src/boot.rs:1540-1563`) asserts `undeclared_parallel_count() == 0`,
  `known_conflict_count() == 0` and `unknown_pair_count() == 0` as plain
  `assert_eq!` (release-live, per the comment at `:1530-1539`).
  `build_scheduler_reports_zero_access_conflicts`
  (`byroredux/src/boot.rs:1939-1962`, #3111) pins the same three in `cargo test`.
  Both still exist and still assert.
- **Checklist 3 — check-before-insert (#2384).** `track_read`
  (`lock_tracker.rs:110-129`) and `track_write` (`:155-172`) both run the
  `#[cfg(debug_assertions)] global_order::record_and_check(...)` block *before*
  `locks.borrow_mut().insert(...)`. The `is_clean()`-after-`catch_unwind`
  assertions are present in scenario 1 (`:706-710`), scenario 4 (`:765-769`)
  and the cross-thread real-`World` scenario 5 (`:773-812`, #2387), which
  asserts exactly one worker is rejected and both worker-local maps are clean.
- **Checklist 4 — `GRAPH` poison recovery (#2385).** All four accesses use
  `unwrap_or_else(|poison| poison.into_inner())`: `:364` (fast-path read),
  `:392` (slow-path write), `:447` (`reset`), `:456` (`poison_for_tests`).
  No `.expect("GRAPH poisoned")` / `.unwrap()` on `GRAPH` anywhere. The
  cycle panic still `drop(graph)`s before unwinding (`:404-407`). Scenario 6
  (`:806-814`) drives a real poison then a recovery.
- **Checklist 5 — recursive read stays a warning (#2386).**
  `lock_tracker.rs:88-104`: one `log::warn!` on the `read_count == 1` → 2
  transition, then `saturating_add(1)` and continue. No panic.
  `recursive_read_warns_once_and_continues` (`:572-584`) pins it. #3249 already
  covers the "unbounded / no call-site info" complaint — not re-filed.
- **Checklist 6 — no structural mutation during systems.** `World::insert`,
  `spawn`, `despawn`, `register` and `storage_write` all take `&mut self`;
  systems receive `&World`, so this is compiler-enforced and needs no runtime
  guard. Verified no `&mut World` reaches a scheduler system signature.
- **Checklist 7 — poisoning.** All 19 `RwLock::read()`/`write()` acquisitions in
  `world.rs` resolve `PoisonError` through `storage_lock_poisoned::<T>()` /
  `resource_lock_poisoned::<R>()`; the 7 `get_mut()` paths (`:157`, `:183`,
  `:286`, `:310`, `:336`, `:384`, `:924`) do the same. Zero bare `.unwrap()` on
  a lock result. The remaining `.expect(...)` calls in `query.rs` (`:47`,
  `:119`, `:265`) and `resource.rs` (`:52`, `:93`, `:101`) are downcast
  assertions ("storage/resource type mismatch (bug in World)"), not poison
  swallowing.
- `CommandRegistry` held read across `ConsoleCommand::execute`
  (`byroredux/src/boot.rs:342-346`, `crates/debug-server/src/evaluator.rs:428-434`)
  is deliberate and documented (#1786); no site acquires `CommandRegistry`
  while holding anything else, so it contributes no cycle.
- `SaveRegistry` held across `save_world`/`validate_world`
  (`byroredux/src/save_io.rs:775-788`) is likewise deliberate, with the
  quiescent-lane argument written out (#3113/#2154); no reverse edge exists.

---

### Coverage gaps

- **The detector was never run.** `BYRO_LOCK_ORDER_CHECK=1` needs a debug build
  plus a Vulkan device and on-disk game data, and the project rule forbids
  launching a parallel engine instance. Every "would abort a debug build" claim
  above is derived from reading the two acquisition sites, not from an observed
  panic. The named confirming signal is in each finding's Verification Path.
- **Static analysis limits.** Guards moved into closures or iterator adapters
  and then dropped elsewhere, and guards crossing a trait-object boundary
  (`ConsoleCommand::execute`, `SaveRegistry`'s `SaveFn`/`ValidateFn` pointers,
  `Box<dyn System>`), are not followed precisely — the closure over the call
  graph stops at function-pointer dispatch. `crates/save`'s per-column
  `save`/`validate` function pointers were therefore inspected by hand only for
  `validate_animation`; the other six `validate_*` helpers in
  `crates/save/src/validate.rs` were read but produced no bidirectional pair.
- **Cross-crate call closure is intra-workspace only.** Calls into `rapier`,
  `ruffle`, `ash` and other third-party crates cannot re-enter the ECS, so this
  is not believed to hide anything, but it is untested.
- **Overlap-feasibility claims rest on the schedule as written.** Every
  "cannot overlap" argument above traces the site to an `add_exclusive*`
  registration in `boot.rs` or to the between-frames `step_*` lanes. It does
  not account for future re-registration, which is exactly the fragility
  `docs/engine/ecs.md:639-643` warns about.
- Dimension 3 covers ECS `RwLock`s only. `Mutex`/`RwLock` inside
  `crates/renderer` (allocator, texture registry, acceleration manager) and the
  debug-server's per-client thread queue are other dimensions' scope.

---

## Dimension 4 — Scheduler Access Declarations (regression guard)


Audit: ByroRedux concurrency, 2026-08-30. HEAD `64f64480`. Depth: **deep**.
Scope: `crates/core/src/ecs/{scheduler.rs,access.rs,system.rs}`,
`byroredux/src/boot.rs` (`build_scheduler` / `install_runtime_registries`),
`byroredux/src/scheduler_access_tests.rs`, `byroredux/src/commands/world_info.rs`.

### Method

Parsed the whole `build_scheduler` body (`boot.rs:706-1523`) into a registration
table: **66 registrations — 9 parallel, 57 exclusive**. All 9 parallel entries use
`add_to_with_access`; 10 of the 57 exclusives use `add_exclusive_with_access`
(19 declared, 47 undeclared exclusives). Ran the live suite (15/15 green, below).
Then computed the cross-stage producer/consumer analysis (checklist 5b) over every
declared read/write, plus hand-checks of the highest-value undeclared exclusives.

Registration table (declared entries only; execution order = `Stage` discriminant,
then parallel phase, then exclusive phase in registration order):

| # | Stage | Phase | System | Line |
|---|---|---|---|---|
| 0 | Early | par | `player_controller_system` | 723 |
| 1 | Early | par | `timer_tick_system` | 780 |
| 2 | Early | **excl** | `weather_system` | 764 |
| 3 | Update | par | `make_animation_system()` | 1009 |
| 4 | Update | excl | `pool_regen_tick_system` | 993 |
| 5 | Update | excl | `cinematic_animation_event_system` | 1057 |
| 6 | PostUpdate | par | `make_transform_propagation_system()` | 1091 |
| 7 | PostUpdate | excl | `make_billboard_system()` | 1245 |
| 8 | PostUpdate | excl | `make_world_bound_propagation_system()` | 1262 |
| 9 | Physics | par | `physics_sync_system` | 1273 |
| 10 | Late | par | `camera_follow_system` | 1330 |
| 11 | Late | par | `reverb_zone_system` | 1411 |
| 12 | Late | par | `log_stats_system` | 1482 |
| 13 | Late | par | `metrics_sample_system` | 1501 |
| 14 | Late | excl | `submersion_system` | 1392 |
| 15 | Late | excl | `water_damage_system` | 1422 |
| 16 | Late | excl | `reconcile_pending_dead_actors_system` | 1437 |
| 17 | Late | excl | `make_water_interaction_system()` | 1444 |
| 18 | Late | excl | `water_audio_system` | 1453 |

Analyzable pairs: Early 1 + Late 6 = **7** (Update / PostUpdate / Physics each hold
exactly one parallel system and analyze nothing) — matches the `>= 7` floor pinned by
`scheduler_access_invariants_hold_on_the_real_schedule`.

---

### Findings

#### CONC-D4-2026-08-30-01: `make_billboard_system` (PostUpdate) reads the camera pose that `camera_follow_system` (Late) authors — billboards orient to the previous frame's camera
- **Severity**: MEDIUM
- **Dimension**: Scheduler Access Declarations (cross-stage sequencing, checklist 5b)
- **Location**: `byroredux/src/boot.rs:1243-1253` (billboard registration) + `byroredux/src/boot.rs:1328-1348` (camera_follow registration); read site `byroredux/src/systems/billboard.rs:63-84`
- **Status**: NEW (same shape as #3180, which fixed the inversion for `submersion_system` only)
- **Trigger Conditions**: `PlayerMode::Character` (the gameplay camera) + any frame in
  which the camera pose changes. Not reachable in `PlayerMode::FlyCam`, where
  `fly_camera_system` writes the camera `Transform` in `Stage::Early` and
  `transform_propagation` composes its `GlobalTransform` in the same `PostUpdate`
  parallel phase that precedes the billboard exclusive.
- **Verification Path**: `cargo test` cannot see it (the analyzer only reasons within a
  stage, so both counters stay 0). Confirm by hand from the stage table above, or
  visually: fast mouse-yaw in an exterior in player mode — billboard/impostor quads
  shear or show a sliver edge that snaps back when the camera stops.
- **Description**: `make_billboard_system` is a `Stage::PostUpdate` **exclusive**; its
  first act is to read the active camera's `GlobalTransform` and derive `cam_pos` /
  `cam_forward`, which is the entire input to every billboard rotation it writes. In
  `PlayerMode::Character` the sole author of that camera pose is
  `camera_follow_system`, registered `add_to_with_access(Stage::Late, …)` and declaring
  `.writes::<GlobalTransform>()` + `.writes::<Transform>()` (`fly_camera_system`
  early-returns in Character mode, `systems/camera.rs:20-26`). `Stage::PostUpdate`
  (discriminant 2) executes strictly before `Stage::Late` (discriminant 4), so within
  frame N the billboard system reads the pose authored in Late of frame N-1 — and the
  `transform_propagation` pass that runs immediately before it in PostUpdate recomposes
  the camera `GlobalTransform` from that same frame-N-1 `Transform`, so there is no
  second path to a fresh value. The renderer then draws frame N from the frame-N camera
  pose (`build_render_data` runs after the whole schedule), against billboards oriented
  to frame N-1.
  This is exactly the defect #3180 found and fixed for `submersion_system` — the commit
  (`5ce2b1c5`) moved that one system from `PostUpdate` to `Late` and left the sibling
  PostUpdate consumer of the same camera pose in place. The `#1375` invariant comment
  directly above the billboard registration (`boot.rs:1220-1236`) reasons only about
  Late-stage *writes* of `GlobalTransform` versus `WorldBound` propagation; it never
  considers a PostUpdate *read* of a Late-authored pose.
- **Evidence**:
  ```rust
  // byroredux/src/boot.rs:1243-1253
  scheduler.add_exclusive_with_access(
      Stage::PostUpdate,
      make_billboard_system(),
      Access::new()
          .reads_resource::<ActiveCamera>()
          ...
          .writes::<byroredux_core::ecs::GlobalTransform>(),
  );

  // byroredux/src/systems/billboard.rs:77-85
  let Some(mut gq) = world.query_mut::<GlobalTransform>() else { return; };
  let Some(cam_global) = gq.get(cam_entity).copied() else { return; };
  let cam_pos = cam_global.translation;
  // Camera forward = rotation * -Z (see Camera::view_matrix).
  let cam_forward = cam_global.rotation * -Vec3::Z;

  // byroredux/src/boot.rs:1328-1332  (the sole Character-mode author of that pose)
  scheduler.add_to_with_access(
      Stage::Late,
      crate::systems::camera_follow_system,
  ```
  Stage order that makes it structural — `crates/core/src/ecs/scheduler.rs:27-38`
  (`Early=0 … PostUpdate=2 … Late=4`, `BTreeMap` ascending) and `:497-515`
  (per stage: whole parallel phase, then exclusives).
- **Impact**: One full frame of camera lag on every billboard rotation in gameplay
  (player) mode. At 60 fps and a 400 °/s flick that is ~6.7° of facing error, which for
  a camera-facing quad is visible as shear/sliver on grass, tree impostors and
  SpeedTree billboards during fast turns, resolving as soon as the camera stops (the
  `camera_changed` gate at `billboard.rs:93-96` means the steady state is correct).
  No race, no unsoundness — a pure ordering defect, and invisible to
  `known_conflict_count()` / `unknown_pair_count()` because `analyze_pair` never
  compares systems in different stages.
- **Related**: #3180 (`5ce2b1c5`, the identical inversion for `submersion_system`);
  #1374 / #1375 (billboard camera-motion gate + the PostUpdate ordering contract);
  #217 (bounds propagation must run after billboard rotations).
- **Suggested Fix**: Move `camera_follow_system` so the pose is authored before its
  PostUpdate consumer — it only needs the player body's *propagated* `GlobalTransform`,
  so a `Stage::PostUpdate` **exclusive** registered between `transform_propagation` and
  `make_billboard_system` satisfies every existing contract (billboards see the current
  pose; bounds propagation, still last, sees the final camera GT; the Late water/audio
  consumers still sequence after it). Note this contradicts
  `submersion_runs_after_camera_follow_and_before_water_audio`, which asserts
  `!late.systems[camera_follow].is_exclusive` — that pin has to be rewritten in the same
  commit, and the #3180 orderings (camera_follow before submersion before water_audio
  before audio_system) re-expressed across stages rather than within Late.

#### CONC-D4-2026-08-30-02: `submersion_system` (Late) sets `ParticleEmitter.rate`, but its only consumer `particle_system` runs in PostUpdate — water-disturbance emission lags a frame
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations (cross-stage sequencing, checklist 5b)
- **Location**: `byroredux/src/boot.rs:1118` (`particle_system`, PostUpdate exclusive) + `byroredux/src/boot.rs:1390-1400` (`submersion_system`, Late exclusive); write site `byroredux/src/systems/water.rs:262-277`, read site `byroredux/src/systems/particle.rs:367-372`
- **Status**: NEW
- **Trigger Conditions**: Any frame in which the camera enters, leaves, or moves within
  a `WaterVolume` that carries a `ParticleEmitter` — i.e. every ripple/splash
  disturbance emitter.
- **Verification Path**: Hand-check of the stage table only; the KPIs are blind to it
  (`analyze_pair` is intra-stage) and `submersion_system`'s declaration is the only one
  of the pair that names `ParticleEmitter` at all (`particle_system` is a bare
  `add_exclusive`, so its side is an undeclared row).
- **Description**: `submersion_system` writes `emitter.rate` for every water volume the
  camera disturbs; `particle_system` is the sole consumer of `rate` (it integrates the
  spawn accumulator from it). `Stage::PostUpdate` (2) executes before `Stage::Late` (4),
  so the rate `particle_system` spawns against in frame N is the one
  `submersion_system` computed in frame N-1.
- **Evidence**:
  ```rust
  // byroredux/src/systems/water.rs:262-268   (submersion_system, Stage::Late exclusive)
  if let Some((volume_q, mut emitter_q)) = world.query_2_mut::<WaterVolume, ParticleEmitter>() {
      for (entity, volume) in volume_q.iter() {
          if let Some(emitter) = emitter_q.get_mut(entity) {
              let previous = emitter.rate;
              let rate = disturbance_rate(cam_pos, volume);
              emitter.rate = rate;

  // byroredux/src/boot.rs:1118              (particle_system, Stage::PostUpdate exclusive)
  scheduler.add_exclusive(Stage::PostUpdate, particle_system);
  ```
- **Impact**: One frame of latency on the water-disturbance spawn rate at the moment the
  player enters/leaves water. Cosmetic and sub-perceptual in steady state; no race and
  no correctness hazard. Pre-existing rather than a #3180 regression — before that
  commit `submersion_system` was a PostUpdate exclusive registered *after*
  `particle_system` (`git show 5ce2b1c5^:byroredux/src/boot.rs`, lines 1057 vs 1221),
  so the inversion already existed within PostUpdate.
- **Related**: #3180 (moved `submersion_system` PostUpdate → Late); CONC-D4-2026-08-30-01
  (same class, larger blast radius).
- **Suggested Fix**: Either accept and document the one-frame lag on the
  `submersion_system` registration comment, or move the disturbance-rate write out of
  `submersion_system` into a PostUpdate step that precedes `particle_system` — the write
  needs only `ActiveCamera` + `WaterVolume`, none of the Late-authored camera pose that
  forced #3180's move.

#### CONC-D4-2026-08-30-03: `player_controller_system`'s doc comment points its access declaration at `main.rs`, which has not held it since #1858/#1670
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations (doc rot)
- **Location**: `byroredux/src/systems/character.rs:76-78`
- **Status**: NEW
- **Trigger Conditions**: n/a (documentation).
- **Verification Path**: `cargo test` — none; grep. `byroredux/src/main.rs` contains no
  `Access::new()`; `build_scheduler` (all 19 declarations) lives in `boot.rs:706-1523`,
  and `main.rs:472/505` only calls `boot::build_scheduler()` and
  `boot::install_runtime_registries`.
- **Description**: The M27-Phase-3 merge comment on `player_controller_system` — the
  system whose whole reason for existing is that its declaration is the union of
  `fly_camera_system` + `character_controller_system` — sends a maintainer to the wrong
  file to find or amend that union.
- **Evidence**:
  ```rust
  // byroredux/src/systems/character.rs:76-78
  /// Access (declared at registration in `byroredux/src/main.rs`) is the
  /// union of the two inner systems' accesses. The `PlayerMode` read
  /// here is itself part of that union.
  ```
- **Impact**: A maintainer widening `character_controller_system`'s or
  `fly_camera_system`'s access surface is pointed at a file with no declarations in it;
  the most likely outcome is a silently incomplete union on the engine's only Early
  parallel pair, which is precisely what makes `known_conflict_count() == 0` unsound
  (the #2676 / #2389 failure mode).
- **Related**: #1858 / #1670 (`main.rs` → `boot.rs` split); #2676; #2389.
- **Suggested Fix**: Change the path to `byroredux/src/boot.rs` (`build_scheduler`).

---

### Dropped (stale premise)

**4 candidates dropped.**

1. **`weather_system` → `player_controller_system` WindField one-frame read**
   (checklist 5's worked example). Real — `player_controller_system` is Early *parallel*
   and `weather_system` is Early *exclusive*, so the reader genuinely precedes the writer
   — but it is the documented intent of #3111 (`boot.rs:760-763`, "the controller sees
   one stable snapshot") and was already traced and explicitly cleared by
   `AUDIT_CONCURRENCY_2026-08-24.md:346` and `AUDIT_CONCURRENCY_2026-08-27b.md:298`.
   Re-filing it would be a duplicate of ground already walked, so it is recorded under
   *Verified clean* instead.
2. **`player_controller_system` (Early) reads `RapierHandles`, sole declared writer
   `physics_sync_system` (Physics)** — reader is three stages ahead of the writer, so it
   flags on the mechanical 5b scan. Dropped: `RapierHandles` is stable per-entity
   identity, the read site (`character.rs:212-215`) only uses it to exclude the player's
   own collider from a shape-cast, and it is behind
   `.and_then(|q| q.get(...))` so an absent handle degrades to "exclude nothing" for the
   single frame between body creation and the first `physics_sync` pass. No stale-value
   hazard.
3. **`analyze_pair` cannot see a component-bag/resource-bag mis-declaration** —
   `collect_overlap` only compares `components_*` against `components_*` and
   `resources_*` against `resources_*` (`access.rs:176-219`), so a type declared
   `.reads::<T>()` on one system and `.writes_resource::<T>()` on another would report
   `None` despite a real `TypeId` collision. Dropped as unreachable: a workspace-wide
   scan for `impl Component for X` ∩ `impl Resource for X` returns the empty set, so no
   type can currently be declared in both bags. (Types that *look* duplicated in the
   table — `components::WaterPlane` vs `components::water::WaterPlane`,
   `components::SubmersionState` vs `components::water::SubmersionState` — are
   re-export aliases of one type, `components/mod.rs:97-99`, so the `TypeId` comparison
   is correct.)
4. **Bare `add_exclusive` on undeclared gameplay exclusives** — 47 of 57 exclusives
   carry no declaration. Not re-filed: **Existing: #3473** already covers this
   (checklist item 6's dedup instruction).

### Verified clean

1. **Conflict model matches the enum (checklist 1).** `AccessConflict` has exactly the
   three variants `None` / `Unknown { left_undeclared, right_undeclared }` /
   `Conflict { pairs }` (`access.rs:139-152`); there is no `Parallel` variant.
   `analyze_pair` (`:162-171`) returns `Unknown` on the *first* match arm whenever either
   side is `None` — the pessimistic fallback is intact — and `Conflict` only on a
   Write/Read, Read/Write or Write/Write overlap of the same `TypeId` in the same bag
   class (`:176-219`). `ConflictKind` still has the three directions. Ten unit tests in
   `access.rs:248-402` pin every arm including both undeclared cases.
2. **KPIs and construction assertions (checklist 2).**
   `undeclared_count() = undeclared_parallel_count() + undeclared_exclusive_count()`
   (`scheduler.rs:651-683`, #1237) — the three predicates partition on `is_exclusive`
   exactly. `install_runtime_registries` (`boot.rs:1540-1563`) still holds all three as
   **release** `assert_eq!` (not `debug_assert!`), on
   `undeclared_parallel_count()`, `known_conflict_count()` and `unknown_pair_count()`,
   and still runs *after* `debug_server::start` appends `DebugDrainSystem`
   (`main.rs:496` before `main.rs:505`).
3. **The pin still exists and still passes (checklist 2).**
   `boot::scheduler_access_report_tests::build_scheduler_reports_zero_access_conflicts`
   (`boot.rs:1940-1963`) asserts the same three counts. **Ran it**:
   `cargo test -p byroredux --bin byroredux scheduler_access` → **15 passed, 0 failed**,
   including that test plus `scheduler_access_invariants_hold_on_the_real_schedule`
   (which additionally floors `parallel_system_count() >= 9` and
   `analyzed_pair_count() >= 7` against monotone-demotion erosion, #2393),
   `player_wind_read_is_declared_and_weather_writer_is_exclusive`,
   `contract_bearing_exclusives_declare_their_access`, and
   `submersion_runs_after_camera_follow_and_before_water_audio`.
   `parallel-scheduler` is confirmed default-on (`crates/core/Cargo.toml:7`) with no
   `default-features = false` on `byroredux-core` anywhere in the workspace.
4. **Exclusive phase (checklist 3).** `Scheduler::run` (`scheduler.rs:497-515`) runs a
   stage's whole `data.parallel` batch (rayon `par_iter_mut`, or a plain `for` without
   the feature) and only then walks `data.exclusive` serially; `access_report`
   (`:556-598`) lists exclusives but pairs only `data.parallel[i] × data.parallel[j]`.
   Both M27 Phase-3 re-stagings hold: `audio_system` is
   `add_exclusive(Stage::Late, …)` (`boot.rs:1481`) and `spin_system` is
   `add_exclusive(Stage::Update, …)` (`boot.rs:1083`). The Phase-3 **merge** also holds:
   `player_controller_system` is a `Stage::Early` parallel `add_to_with_access`
   (`boot.rs:723-759`) that dispatches on `PlayerMode` to `fly_camera_system` or
   `character_controller_system` (`character.rs:80-91`), and I re-derived both inner
   bodies' access surfaces against the declared union — `fly_camera_system`
   (`camera.rs:16-95`) touches only `PlayerMode`, `ActiveCamera`, `InputState`,
   `ActionState` and `Transform`; all five are declared.
5. **Re-entry & panic policy (checklist 4).** `Scheduler` has no `impl Resource`
   anywhere in the workspace (only `SchedulerSystemTimings` and `SchedulerAccessReport`
   do), and is owned by `App` (`main.rs:472`), so re-entry from a system body remains
   structurally impossible (#868). Fail-fast on panic is treated as the documented
   design (#1412); no `catch_unwind` finding filed.
6. **No bare `add_to` on any parallel system (checklist 6) — the counter is not lying.**
   All 9 parallel registrations use `add_to_with_access`; there are zero `add_to(` /
   `add(` / `try_add_to(` calls in `build_scheduler`. The only production registration
   outside `boot.rs` is `crates/debug-server/src/lib.rs:34`,
   `scheduler.add_exclusive(Stage::Late, drain_system)` — exclusive, so it cannot move
   either parallel counter; every other hit is a test or a doc example.
7. **Declaration honesty on the 7 analyzable pairs.** Because only Early (2 systems) and
   Late (4 systems) have more than one parallel entry, the zero-conflict proof rests on
   six declarations. I re-read all six bodies against their declarations:
   `player_controller_system` + `refresh_action_state` (character.rs), `timer_tick_system`
   (`timer.rs:30,52` — `ScriptTimer` + `TimerExpired`, nothing else),
   `camera_follow_system` (PlayerMode, PlayerEntity, ActiveCamera, InputState,
   CharacterController, GlobalTransform, Transform — all declared, #2676 read intact),
   `reverb_zone_system` (`audio.rs:58-94` — `CellLightingRes` read, `AudioWorld` write,
   nothing else), `log_stats_system` (`debug.rs:135-162` — reads only) and
   `metrics_sample_system` (`metrics.rs:77-133` + the #2389 additions). All complete;
   Early's single pair and all six Late pairs are genuinely disjoint.
8. **Cross-stage sequencing over every declared single-writer type (checklist 5b).**
   Computed writer→reader stage ordering for all 19 declared systems. Correct
   (reader at-or-after writer) for: `AnimationTextKeyEvents` and `StringPool`
   (animation, Update par → cinematic_animation_event, Update excl);
   `CharacterController` (player_controller, Early → camera_follow, Late);
   `WaterContact` (physics_sync, Physics → water_damage + water_interaction, Late);
   `SubmersionState` (submersion → water_audio, both Late excl, ord 59 < 63);
   `CellLightingRes` (weather, Early excl → reverb_zone, Late par);
   `WindField` → billboard (PostUpdate), physics_sync (Physics), submersion (Late).
   Multi-writer types were checked for same-stage/same-phase collision and found clean:
   `Transform` has five writers but each in a distinct stage; `GlobalTransform`'s four
   writers are one PostUpdate parallel + two PostUpdate exclusives + one Late parallel;
   `RippleEvent`/`SplashEvent` (submersion ord 59, water_interaction ord 62) both precede
   their `water_audio` reader (ord 63); `AudioWorld` is written by `reverb_zone` (Late
   parallel) then `water_audio` then `audio_system` (Late exclusives, in that order);
   `PendingDeathReconciliations` is produced in Early/Late and drained by
   `reconcile_pending_dead_actors_system` last; `PhysicsWorld` and `ActorValues` writers
   are all in distinct stages.
9. **`sys.accesses` reporting surface.** `SysAccessesCommand`
   (`commands/world_info.rs:298-400`) prints all six counters in its header line and
   renders both `Conflict` and `Unknown` rows with the offending type and direction; it
   reads the boot-time `SchedulerAccessReport` snapshot, so it reflects the schedule the
   assertions validated.
10. **The 5a regression would be caught automatically.** Flipping `weather_system` to
    `add_to_with_access(Stage::Early, …)` produces a resource `WriteRead` conflict against
    `player_controller_system`'s declared `WindField` read → `known_conflict_count() != 0`
    → both the boot assertion and two `cargo test` pins fire. The dedicated pin
    `player_wind_read_is_declared_and_weather_writer_is_exclusive`
    (`scheduler_access_tests.rs:95-122`) additionally greps the registration form itself,
    so the catch does not depend on the declaration staying complete.

### Coverage gaps

- **47 of 57 exclusives are undeclared**, so the mechanical 5b sweep could only be run
  over the 19 declared systems. I hand-checked the highest-value undeclared consumers
  (`particle_system`, `footstep_system`, `cinematic_root_motion_system`,
  `animate_lights_system`, `spin_system`, `ragdoll_writeback_system`,
  `audio_system`, `event_cleanup_system`) against the declared writers, which is how
  CONC-D4-2026-08-30-02 surfaced — but the remaining ~39 scripting/AI exclusives were not
  individually traced. This is the coverage cost of #3473 and is not independently
  filable.
- **Three of five stages hold exactly one parallel system**, so `analyzed_pair_count()`
  is 7 and the zero-conflict proof only actually constrains six declarations. This is a
  known, already-pinned property (#2393, the `>= 9` / `>= 7` floors), not a new finding,
  but it bounds how much assurance the green KPI carries.
- **CONC-D4-2026-08-30-01's visible symptom was not observed on a running engine** — the
  reasoning is entirely from the stage table and the read/write sites, and no engine was
  launched (per the no-parallel-launch rule, the user may have their own instance up).
  The code-level ordering claim is nonetheless verifiable without a device.
- **Interaction with the renderer's own camera snapshot** (`build_render_data`) was
  treated as "runs after the whole schedule"; I did not audit whether any render-side
  path re-derives billboard orientation independently, which would change
  CONC-D4-2026-08-30-01's impact but not its premise.

---

## Dimension 5 — RwLock Patterns: Resource↔Storage & Physics Step


Audit date: 2026-08-30 · HEAD `64f64480` · Depth: **deep**
Scope: `crates/physics/**`, `byroredux/src/systems/{character,locomotion,follow,navmesh_path,guard,escort,patrol,wander,travel,cinematic,camera,bounds,water}.rs`,
`byroredux/src/{ragdoll,combat,interaction,scene,save_io,render}.rs`, `byroredux/src/cell_loader/unload.rs`,
`byroredux/src/commands/{view,physics}.rs`, `byroredux/src/boot.rs`, `docs/engine/ecs.md`, `crates/core/src/ecs/lock_tracker.rs`.

**Headline: one HIGH finding, confirmed empirically — the `lock-order-check` CI job is RED at HEAD.**
Running the project's own detector on a clean tree at `64f64480`:

```
$ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins
test result: FAILED. 1642 passed; 5 failed; 17 ignored
  ragdoll::tests::activate_then_writeback_moves_bones
  ragdoll::tests::falling_ragdoll_expands_skinned_mesh_world_bound
  ragdoll::tests::writeback_inverts_body_local_offset_round_trip
  ragdoll::tests::writeback_rederives_non_body_descendant_from_simulated_parent
  ragdoll::tests::writeback_uses_seed_time_scale_not_live_scale_after_mutation

$ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins -- --skip combat_approach_line_of_sight
test result: ok. 1645 passed; 0 failed; 17 ignored; 2 filtered out
```

The second run isolates the cause: the two `combat_approach_line_of_sight_*` tests are the only
producers of the cycle-closing edge.

---

### Findings

#### CONC-D5-2026-08-30-01: `combat_approach_line_of_sight_reaches` holds the `PhysicsWorld` guard across `RapierHandles`, closing a live `PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld` lock cycle
- **Severity**: HIGH
- **Dimension**: RwLock Patterns — Resource↔Storage & Physics Step
- **Location**: `byroredux/src/commands/view.rs:175-215` (edge source); cycle partners `crates/physics/src/sync.rs:821-844` and `byroredux/src/ragdoll.rs:488-495`
- **Status**: NEW (introduced by `5c8a1581`, "Fix #3422, Fix #3424, and gate combat.approach on line of sight (#3423)"; not in the pre-filtered open-issue list, not in `docs/audits/AUDIT_CONCURRENCY_2026-08-{24,27,27b}.md`)
- **Trigger Conditions**: Any process in which all three edges are observed. Already reproducible with **zero concurrency** in a debug build: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` aborts 5 `ragdoll::tests::*` at HEAD. A genuine hang needs two threads that hold overlapping guards in the opposing orders; today the three sites are separated by scheduler stages (`Stage::Physics` parallel batch / `Stage::Late` exclusives), so the deadlock is *latent*, but the detector abort is *live*.
- **Verification Path**: `cargo test` — `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` (run; output above). Panic text from `crates/core/src/ecs/lock_tracker.rs:411`.
- **Description**:
  `combat_approach_line_of_sight_reaches` binds the `PhysicsWorld` resource read guard to a
  named local at line 175 (`let Some(physics) = … else { return true; };`), so the guard is
  alive for the whole function body. It then acquires the `RapierHandles` **storage** at line
  186 and the `ActorColliderOwner` storage at line 213 underneath it. That records the edges
  `PhysicsWorld → RapierHandles` and `PhysicsWorld → ActorColliderOwner` in `lock_tracker`'s
  single TypeId graph (which keys storages *and* resources together — `World::resource`/
  `try_resource` go through the same `lock_tracker::TrackedRead/Write`, `crates/core/src/ecs/world.rs:708,738`).

  Two long-standing, canonical edges complete the ring:
  - `RapierHandles → GlobalTransform` — `collect_newcomers` (`crates/physics/src/sync.rs:821-844`)
    and `push_kinematic` (`:1055-1063`) both hold `RapierHandles` while acquiring `GlobalTransform`.
    `docs/engine/ecs.md:602-604` *documents* this as the process-wide order.
  - `GlobalTransform → PhysicsWorld` — `ragdoll_writeback_system` (`byroredux/src/ragdoll.rs:491-495`)
    holds the `GlobalTransform` write guard while taking `PhysicsWorld`.

  Every other `PhysicsWorld` site in the tree acquires storages **before** the resource and drops
  them first (`collect_newcomers`→`register_newcomers`, `push_kinematic`, `pull_dynamic`,
  `apply_buoyancy`, `dump_awake_fallers` #2136, `spawn_collider_census_report` #3266,
  `probe_walkable_floor_near`, `interaction::target_has_line_of_sight`, `combat.rs`'s melee swing,
  `release_victim_rapier_bodies`). This function is the **only** inversion — and its own comment
  at lines 184-185 ("Same lock discipline as the swing: resolve body ownership before touching
  PhysicsWorld-adjacent component storages") describes the discipline it does not follow: the
  swing (`byroredux/src/combat.rs:159-172`) resolves ownership first and takes `PhysicsWorld` after.
- **Evidence** (current file, `byroredux/src/commands/view.rs`):
```
175	    let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
176	        return true;
177	    };
…
184	    // Same lock discipline as the swing: resolve body ownership before
185	    // touching PhysicsWorld-adjacent component storages.
186	    let (excluded_body, owners) = match world.query::<byroredux_physics::RapierHandles>() {
…
198	    let Some(hit_body) = physics
199	        .cast_ray(camera_pos, direction, distance, excluded_body)
…
212	    let hit_root = world
213	        .get::<byroredux_physics::ActorColliderOwner>(collider_entity)
```
  Detector output (verbatim, two distinct rings reported):
```
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of
`byroredux_physics::world::PhysicsWorld` while holding
`byroredux_core::ecs::components::global_transform::GlobalTransform` … cycle:
PhysicsWorld → RapierHandles → GlobalTransform → PhysicsWorld

ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of
`byroredux_physics::world::PhysicsWorld` while holding
`byroredux_core::ecs::components::hierarchy::Children` … cycle:
PhysicsWorld → RapierHandles → RigidBodyData → Parent → Children → PhysicsWorld
```
- **Impact**:
  1. **CI is red.** The dedicated `lock-order-check` job (`.github/workflows/ci.yml:108-121`,
     `BYRO_LOCK_ORDER_CHECK: 1`, `cargo test --workspace`) fails at HEAD. Every subsequent
     concurrency regression is masked while it stays red — the job is the project's only dynamic
     ABBA proof for exercised paths.
  2. **Real ABBA risk on the next scheduling change.** `PhysicsWorld` is currently reached from
     many storage guards in the safe direction (`FollowBehavior`/`GuardBehavior`/`TravelBehavior`/
     `EscortBehavior` → `PhysicsWorld` in `systems/{follow:139→255, guard:144→229, travel:184→268,
     escort:205→363}.rs`; `Transform`/`Parent`/`Children`/`GlobalTransform` → `PhysicsWorld` in
     `ragdoll.rs`). Each of those becomes a cycle the instant a `PhysicsWorld`-held storage
     acquisition exists — this one edge opens all of them at once.
  3. `combat.approach` is a `DebugDrainSystem` (Late exclusive) command: running it in a debug
     engine build with the detector on aborts the session (the #2388 precedent).
- **Related**: #313, #2675 (reachability-based cycle detection), #2136, #2404, #3303, #3423 (the
  commit that introduced it), `docs/engine/ecs.md:596-628`.
- **Suggested Fix**: Move the `RapierHandles` snapshot (lines 186-196) and the
  `ActorColliderOwner` resolution (212-215) **out** from under the `PhysicsWorld` guard — resolve
  `excluded_body`/`owners` first, then scope the `physics` guard to the `cast_ray` call alone
  (`let hit_body = { let physics = world.try_resource::<PhysicsWorld>()?; physics.cast_ray(…) }`),
  exactly as `combat.rs:159-172` and `interaction.rs:790-805` already do; the `None` case must
  still return `true`. Then delete/repoint the now-accurate comment at 184-185.

#### CONC-D5-2026-08-30-02: `ragdoll_writeback_system` acquires `LocalBound` and `WorldBound` while holding the `PhysicsWorld` guard — a second latent `PhysicsWorld → storage` edge
- **Severity**: MEDIUM
- **Dimension**: RwLock Patterns — Resource↔Storage & Physics Step
- **Location**: `byroredux/src/ragdoll.rs:494-505`
- **Status**: NEW
- **Trigger Conditions**: Not cycle-closing today (nothing establishes `LocalBound ⇝ PhysicsWorld`
  or `WorldBound ⇝ PhysicsWorld`; `make_world_bound_propagation_system`,
  `byroredux/src/systems/bounds.rs:133-172`, walks `Parent → Children → GlobalTransform →
  SkinnedMesh → LocalBound → WorldBound` and never touches `PhysicsWorld`). It becomes a cycle the
  moment any bounds-side code path reaches a physics query — e.g. a future WorldBound-driven
  broadphase pre-pass, or a `LocalBound`-keyed collider synthesiser.
- **Verification Path**: `cargo test` — would surface as a new `lock_tracker` cycle panic naming
  `PhysicsWorld → LocalBound` / `PhysicsWorld → WorldBound` under `BYRO_LOCK_ORDER_CHECK=1`.
- **Description**: The function correctly takes the hierarchy cluster in canonical order and then
  `PhysicsWorld` last of the *pre-existing* set — but two more storages are acquired **after** the
  resource guard, inverting the crate-wide "no storage under a `PhysicsWorld` guard" rule. Unlike
  finding 01 this one has no opposing edge yet, so it is a defence-in-depth gap rather than a live
  defect; it is listed because it is the same shape and the same file already documents its
  ordering reasoning, so the omission reads as intentional.
- **Evidence** (current file):
```
488	    let transform_q = world.query::<Transform>();
489	    let parent_q = world.query::<Parent>();
490	    let children_q = world.query::<Children>();
491	    let Some(mut gtq) = world.query_mut::<GlobalTransform>() else {
492	        return;
493	    };
494	    let Some(pw) = world.try_resource::<PhysicsWorld>() else {
495	        return;
496	    };
…
504	    let local_bound_q = world.query::<LocalBound>();
505	    let mut world_bound_q = world.query_mut::<WorldBound>();
```
- **Impact**: Widens the `PhysicsWorld → storage` surface from one site to two; a fix for 01 that
  does not also cover this leaves the class open.
- **Related**: #1981 (the pass that added the bound queries), #313, #2388.
- **Suggested Fix**: Hoist `local_bound_q` / `world_bound_q` above the `try_resource::<PhysicsWorld>()`
  line (they are independent of `pw`), keeping the canonical `… → GlobalTransform → LocalBound →
  WorldBound` order intact and leaving `PhysicsWorld` as the last acquisition with nothing under it.

#### CONC-D5-2026-08-30-03: the canonical acquisition-order table in `docs/engine/ecs.md` omits `PhysicsWorld`, so the rule that finding 01 breaks is unwritten
- **Severity**: LOW
- **Dimension**: RwLock Patterns — Resource↔Storage & Physics Step
- **Location**: `docs/engine/ecs.md:596-635`
- **Status**: NEW
- **Trigger Conditions**: n/a (doc gap)
- **Verification Path**: doc read
- **Description**: The "Canonical acquisition order" block names the hierarchy/skinning/bounds
  cluster (`CharacterController → RapierHandles → Transform → Parent → Children → GlobalTransform
  → SkinnedMesh → MeshHandle → LocalBound → WorldBound → Name → StringPool`) and gives the CHARAL
  pair (`CharacterRuleset → ActorValues`) its own paragraph, but says nothing about `PhysicsWorld`
  — even though it is the single most widely co-acquired resource in the engine, participates in
  the cluster through `RapierHandles`/`GlobalTransform`, and is now the hub of a live cycle. The
  actual rule ("`PhysicsWorld` is acquired last and no storage may be acquired while it is held")
  exists only as prose inside individual functions (`crates/physics/src/sync.rs:236-246`,
  `:611-614`, `byroredux/src/scene.rs:232-235`), one of which (`commands/view.rs:184-185`) states
  it while doing the opposite.
- **Evidence**: `docs/engine/ecs.md:601-604` (the code block) contains no resource type; the
  paragraph at `:620-628` covers only `CharacterRuleset`/`ActorValues`/`CharacterLevel`.
- **Impact**: A reviewer following the documented order has no signal that `view.rs:175` is wrong.
- **Related**: #2404, #3441, #313.
- **Suggested Fix**: Add `PhysicsWorld` as an explicit tail of the physics prelude in the canonical
  table — "storages first, `PhysicsWorld` last, nothing acquired under it" — and name the
  `collect_newcomers` / `ragdoll_writeback_system` sites as the worked examples the way the
  `CharacterRuleset` paragraph names `pool_regen_tick_system`.

---

### Dropped (stale premise)

**13 candidate findings dropped** after checking the premise against HEAD:

1. **`pull_dynamic` closes `GlobalTransform → Transform`** (the #3303 shape) — fixed and intact:
   `crates/physics/src/sync.rs:1180-1197` resolves `Parent`+`GlobalTransform` into an intermediate
   `Vec` in its own block, and `transform_q` is only acquired at `:1213` after that block ends.
   Guard test `pull_dynamic_does_not_close_transform_global_transform_lock_cycle`
   (`sync.rs:1476-1503`) is present and *is* executed by CI (`lock-order-check` runs
   `cargo test --workspace` with `BYRO_LOCK_ORDER_CHECK=1`, `.github/workflows/ci.yml:108-121`).
2. **`push_kinematic` / `pull_dynamic` hold storage guards across the `PhysicsWorld` guard** — the
   verbatim premise of `.claude/issues/2404/ISSUE.md`. Both now snapshot and `drop()` explicitly:
   `sync.rs:1078-1080` and `sync.rs:1132-1134`. Stale.
3. **`dump_awake_fallers` holds `PhysicsWorld` under `RapierHandles`** (#2136) — inverted and
   fixed: the snapshot block `sync.rs:317-336` closes before `query::<RapierHandles>()` at `:338`.
4. **`ContactConfig` re-locked per newcomer** — snapshotted once per batch at `sync.rs:885-889`
   via `.map(|r| *r)`, which drops the guard inside the closure; the per-newcomer loop uses the
   owned `cfg` only.
5. **`release_victim_rapier_bodies` frees after despawn** — it collects `RapierHandles`/`Ragdoll`
   into scratch `Vec`s under a scoped block (`unload.rs:566-583`), drops them, then takes
   `try_resource_mut::<PhysicsWorld>()` (`:587`); the caller invokes it at `unload.rs:328`,
   **before** `world.despawn_batch(victims)` at `:334`. Clean.
6. **`set_linear_velocity` / `set_kinematic_translation` keep the `RapierHandles` read guard alive
   into `resource_mut::<PhysicsWorld>()`** — no: `world.query::<RapierHandles>().and_then(|q| q.get(entity).copied())`
   moves the guard into the closure, which drops it on return; the match scrutinee retains no
   guard temporary. `sync.rs:53-62` and `:85-93`.
7. **A caller holds a `PhysicsWorld` guard when calling those helpers** — checked all 8 call sites
   (`character.rs:498,722,824`, `cinematic.rs:139,392,611,758`, `save_io.rs:657`, `camera.rs:113`,
   `commands/view.rs:358`). `character_controller_system` drops `pw` at `:386`, 112 lines before
   the call. `cinematic.rs`'s three sites gate on `if world.try_resource::<PhysicsWorld>().is_some()`,
   whose condition temporary is dropped before the block (Rust temporary-scope rule for `if`
   conditions). Clean.
8. **`compute_underwater_params` / `camera::assemble_camera` / `render::mod` hold `ActiveCamera`
   across storage queries** — all use `.map(|a| a.0)` or an explicit `drop(active)`
   (`systems/water.rs:525`, `render/camera.rs:136-138`, `render/mod.rs:873-875`,
   `cell_loader/transition.rs:405-407`). False positives from the mechanical scan.
9. **`interaction::target_has_line_of_sight` is the same bug as finding 01** — it is not: the
   gate at `:777` binds `_` (dropped at end of statement) and the real guard is scoped to a block
   at `:797-804`; both `RapierHandles` acquisitions sit outside it.
10. **`combat.rs` melee swing inverts the pair** — no: `:159-169` resolves ownership under a match
    arm that drops the guard, then takes `PhysicsWorld` at `:171`. It is the *correct* reference
    that `view.rs` claims to copy.
11. **`render/water.rs` holds `WaterPlane` across `try_resource_mut::<WaterDrawIndexScratch>()`**
    (`:111-128`) — real overlap, but in the safe storage→resource direction, and
    `WaterDrawIndexScratch` has exactly one consumer, so no opposing edge can exist. Not reported.
12. **`CharacterRuleset` / `ActorValues` cycle regressed** — not regressed. `condition.rs:481-491`
    still snapshots `ActorValues` (clone) and drops the guard before `try_resource::<CharacterRuleset>()`
    at `:500`; `regen.rs:176-179` still takes ruleset-then-`ActorValues`-write. Per CONTEXT rule 5,
    not re-filed.
13. **`physics_sync_system` co-scheduled with another `PhysicsWorld` toucher** — it is the only
    `Stage::Physics` registration (`boot.rs:1273`), there is no `Stage::Physics` exclusive, and
    `install_runtime_registries` (`boot.rs:1541-1558`) asserts
    `undeclared_parallel_count()==0`, `known_conflict_count()==0`, `unknown_pair_count()==0` as
    release assertions. `player_controller_system` (the other `PhysicsWorld` writer) is
    `Stage::Early`.

### Verified clean

- **Checklist 1 — 4-phase collect/register split.** `collect_newcomers` (`sync.rs:807-866`)
  acquires `RapierHandles → CollisionShape → RigidBodyData → GlobalTransform → ActorBoneCollider`,
  collects owned `Newcomer` values, and every guard is a function-scope local dropped on return.
  `register_newcomers` (`:884-1049`) takes `resource_mut::<PhysicsWorld>()` with **no** storage
  guard live, does no `world.*` access inside the loop, and `drop(pw)` at `:1019` precedes
  `query_mut::<RapierHandles>()` at `:1035`. Phase 2 (`push_kinematic`), 2.5 (`apply_buoyancy`,
  `crates/physics/src/water.rs:669-1023`), 3 (`step`, scoped block at `sync.rs:154-157`) and 4
  (`pull_dynamic`) all follow the same snapshot-drop-acquire shape.
- **Checklist 2 — the three Session-76 cycles.** `Transform → GlobalTransform` canonical direction
  preserved in `pull_dynamic` (#3303, guard test present and CI-executed); `CharacterRuleset →
  ActorValues` (#3441) not regressed; the #3312 residual sweep holds — I found no *new* reverse
  edge among the hierarchy cluster. The one *new* cycle (finding 01) is exactly the predicted
  shape: **one site holding a guard across a call that acquires the pair the other way**.
- **Checklist 3 — helper lock order.** Verified at the Rust temporary-lifetime level (see Dropped
  #6/#7): the `RapierHandles` read guard is moved into the `and_then` closure and dropped there,
  strictly before `resource_mut::<PhysicsWorld>()`; no caller holds a `PhysicsWorld` guard.
- **Checklist 4 — `ContactConfig`.** Snapshotted once per batch, never re-locked in the
  per-newcomer loop; also declared in `boot.rs:1290` so the analyzer sees it (#1787).
- **Checklist 5 — cell-unload teardown.** Reads-before-write discipline intact and ordered before
  `despawn_batch` (see Dropped #5). Idempotent for repeated victims (#3380).
- **Checklist 6 — single-threaded placement.** `physics_sync_system` is alone in `Stage::Physics`;
  the parallel-batch conflict invariants are release-asserted at construction.
- **Checklist 7 — wider sweep.** `locomotion.rs` takes no locks at all (pure helpers).
  `navmesh_path.rs` production paths take only `NavmeshTile`; its `PhysicsWorld` use is via
  `step_along_waypoints`, which receives an already-borrowed `&PhysicsWorld`. `wander.rs:270` and
  `patrol.rs:99` scope their behaviour guard inside the pass-1 block. `follow.rs:139`,
  `guard.rs:144`, `travel.rs:184`, `escort.rs:205` keep it alive across `try_resource::<PhysicsWorld>()`
  and the pass-2 writes — this is the **safe** storage→resource direction and records no reverse
  edge, so it is not a finding, but it is exactly what makes finding 01 expensive (see 01 Impact 2)
  and is the natural place to tighten once 01 is fixed.

### Coverage gaps

- **Detector coverage is test-reachability-bounded** (`lock_tracker.rs:242-249`). My empirical run
  covers only what the `byroredux` binary's unit tests drive. Paths no test exercises — most of
  `cell_loader::spawn`, the live streaming steppers in `app_step.rs`, `save_io`'s live reload —
  contribute no edges and are therefore neither cleared nor flagged. I read them for the
  `PhysicsWorld`-held-across-storage shape and found none, but that is static reading, not proof.
- I did not run `cargo test --workspace` under the detector (only `-p byroredux --bins`), so
  cycles that need edges from two different test binaries in one process cannot exist by
  construction, but cycles confined to another crate's test binary were not exercised here.
- The second reported ring (`PhysicsWorld → RapierHandles → RigidBodyData → Parent → Children →
  PhysicsWorld`) contains a `RigidBodyData → Parent` hop whose producing site I did not pin down;
  it does not change the diagnosis (the same single `PhysicsWorld → RapierHandles` edge closes
  both rings, and removing it makes the whole suite green), but the hop is unattributed.
- Resource↔storage overlaps outside the physics domain (`AudioWorld`, `StringPool`, `WindField`,
  `WaterDrawIndexScratch`, animation's `Name`/`StringPool` pair) were only spot-checked; they
  belong to other dimensions and several are already filed (#3444, #3445, #3446).

---

## Dimension 6 — Resource Lifecycle (GPU teardown ordering)


ByroRedux concurrency audit 2026-08-30 · HEAD `64f64480` · depth **deep**

Scope walked: `crates/renderer/src/vulkan/context/{mod,init,teardown,resize,draw,
screenshot,depth_capture,helpers,skinned_blas_refit}.rs`, every `destroy()` /
`recreate_on_resize()` under `crates/renderer/src/vulkan/` (gbuffer, svgf, taa,
caustic, water_caustic, volumetrics, bloom, composite, ssao, restir, exposure,
frame_upscaler, upscaling, presentation, morph_compute, skin_compute, water,
placeholder, compute, gpu_timers, sync, swapchain, buffer, material,
scene_buffer/, acceleration/), plus `texture_registry.rs` and `mesh.rs`.

### Findings

**2 findings** (0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW) + 1 LOW doc note folded in
below.

---

#### CONC-D6-2026-08-30-01: `skin_slots` teardown is nested under `skin_compute.is_some()` — the exact shape #3374 un-nested for `morph_slots`

- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/context/teardown.rs:46-51`
- **Status**: NEW
- **Trigger Conditions**: Shutdown (`VulkanContext::drop`) in any future
  configuration where `self.skin_compute` is `None` while `self.skin_slots` is
  non-empty. **Not reachable at HEAD** — `skin_compute` is assigned exactly once
  (`context/init.rs:600` / `:647`, via `couple_skin_compute_to_palette`) and
  never re-assigned, and every `skin_slots.insert` site
  (`context/skinned_blas_refit.rs:315`) sits inside a `skin_compute` guard. This
  is a defense-in-depth gap, not a live leak.
- **Verification Path**: `cargo test` cannot reach it (needs a live device).
  A source-shape regression test in the style of
  `skinned_blas_refit.rs`'s existing `#3374` pin is the practical guard.
- **Description**: The per-skinned-entity `SkinSlot` drain in `Drop` runs only
  when `skin_compute` is `Some`, because `destroy_slot` is a method on the
  pipeline (it must `free_descriptor_sets` back into the pipeline's
  `FREE_DESCRIPTOR_SET` pool). `MorphSlot`'s sibling drain immediately below was
  deliberately taken *out* of the analogous `(skin_compute, accel_manager)`
  guard by #3374 for exactly this reason — see the long comment at
  `skinned_blas_refit.rs:774-797` ("That is the #2494 mistake one nesting level
  out"). The skin half still carries the coupling.
  The consequence if the gate ever goes false is worse than a plain leak: each
  `SkinSlot::output_buffer` is a `GpuBuffer` holding its own
  `Arc<Mutex<Allocator>>` clone (`vulkan/buffer.rs:633`). Skipping
  `destroy_slot` means those clones are released only by the *natural* Drop pass
  that runs after `VulkanContext::drop` returns — i.e. after the
  `Arc::try_unwrap` at `teardown.rs:346-384` has already given up — which takes
  the #665 leak-guard branch that intentionally leaks the device, surface,
  instance and debug messenger.
- **Evidence** (`crates/renderer/src/vulkan/context/teardown.rs`):
  ```rust
  46          if let Some(ref skin) = self.skin_compute {
  47              let slots = std::mem::take(&mut self.skin_slots);
  48              for (_eid, slot) in slots {
  49                  skin.destroy_slot(&self.device, alloc, slot);
  50              }
  51          }
  52          // #3231 — MorphSlot owns plain buffers with no descriptor sets
  53          // or pipeline dependency (unlike SkinSlot above), so it can be
  54          // torn down unconditionally.
  55          for (_eid, mut slot) in std::mem::take(&mut self.morph_slots) {
  56              slot.destroy(&self.device, alloc);
  57          }
  ```
- **Impact**: None today. Under a future edit that nulls `skin_compute` at
  runtime (a device-lost recovery path, an RT-optional configuration, a
  pipeline-rebuild-on-resize), shutdown silently leaks every live skinned output
  buffer *and* trips the allocator-outstanding-reference guard, leaking the
  `VkDevice`/`VkInstance` too.
- **Related**: #3374 (the `morph_slots` half), #2494, #665 / LIFE-L1, #927.
- **Suggested Fix**: Make the drain unconditional and let `destroy_slot` become
  a free function (or an inherent `SkinSlot::destroy(device, allocator)` plus an
  `Option`-guarded `free_descriptor_sets` when the pipeline exists) — the
  descriptor sets are freed implicitly by pool destruction anyway, so the buffer
  half never needs the pipeline. Add the same source-shape pin #3374 added.

---

#### CONC-D6-2026-08-30-02: `destroy_allocator_owned_resources` documents "reverse-creation order"; the block is neither reverse nor forward creation order

- **Severity**: LOW
- **Dimension**: Resource Lifecycle (GPU teardown ordering)
- **Location**: `crates/renderer/src/vulkan/context/teardown.rs:11-20` (doc),
  `:27-167` (body), `:172-174` (the same claim in `Drop`'s SAFETY comment)
- **Status**: NEW
- **Trigger Conditions**: None at runtime — this is doc rot on a comment that
  invites a specific harmful edit.
- **Verification Path**: `cargo test` (a source-order pin, as
  `resize.rs`'s existing `old_image_views_destroyed_between_…` test does).
- **Description**: Checklist item 1 asks that the actual destroy order be built
  from the code and checked against creation order. It does not match the
  documented claim. Creation order in `context/init.rs` versus the destroy
  sequence inside the allocator guard:

  | destroy # | subsystem | `init.rs` creation line | creation rank |
  |---|---|---|---|
  | 1 | `texture_registry` | 349 | 1st |
  | 2 | `scene_buffers` | 403 | 2nd |
  | 6 | `accel_manager` | 429 | 3rd |
  | 7 | `cluster_cull` | 562 | 4th |
  | 8 | `skin_compute` | 602 | 5th |
  | 9 | `ssao` | 864 | 12th |
  | 10/11 | `placeholder_ao` / `placeholder_caustic_sink` | 746 / 761 | 9th / 10th |
  | 12 | `exposure` | 910 | 13th |
  | 13 | `frame_upscaler` (allocations) | 1341 | 23rd |
  | 14 | `composite` | 1251 | 21st |
  | 15 | `caustic` | 1114 | 19th |
  | 16 | `volumetrics` | 937 | 14th |
  | 17 | `bloom` | 1203 | 20th |
  | 18 | `water_caustic_accum` | 777 | 11th |
  | 19 | `svgf` | 1029 | 17th |
  | 20 | `reservoir_buffers` | 1078 | 18th |
  | 21 | `taa` | 1288 | 22nd |
  | 22 | `gbuffer` | 977 | 16th |

  The block *starts* with the first-created subsystem and ends with the 16th;
  true reverse-creation order would start with `presentation` /
  `frame_upscaler` and end with `texture_registry`. The order is nonetheless
  **correct**: after the `device_wait_idle` at `teardown.rs:176`, Vulkan imposes
  no cross-subsystem destroy ordering (a `VkDescriptorSet` may name a destroyed
  `VkImageView` as long as it is never used again, and every parent/child pair —
  framebuffer→render pass, view→image, image→memory, sets→pool — is contained
  inside a single subsystem's own `destroy`). The four orderings that *are*
  load-bearing are all local and separately commented:
  `skin_slots` before `skin_compute` (:38-45), placeholders after the passes
  whose descriptors name them (:98-107), `frame_upscaler::destroy_allocations`
  after `destroy_device_objects` (:124-130), and `exposure` before the
  `Arc::try_unwrap` (:114-123).
- **Evidence** (`crates/renderer/src/vulkan/context/teardown.rs`):
  ```rust
  11      /// Destroy every subsystem whose resources are owned by the GPU
  12      /// allocator, in reverse-creation order.
  ...
  17      /// the subsystem's own `destroy`). The ordering inside is load-bearing
  18      /// and unchanged: this is a *move*, not a reorganisation.
  ...
  27      unsafe fn destroy_allocator_owned_resources(&mut self, alloc: &SharedAllocator) {
  28          self.texture_registry.destroy(&self.device, alloc);   // created FIRST (init.rs:349)
  29          self.scene_buffers.destroy(&self.device, alloc);      // created SECOND (init.rs:403)
  ```
- **Impact**: A maintainer who reads "reverse-creation order" as a live
  invariant and "restores" it would reshuffle the four local orderings above —
  in particular moving `skin_compute`'s pipeline/pool destroy ahead of the
  per-slot `free_descriptor_sets`, which is a real
  `VUID-vkFreeDescriptorSets-descriptorPool-parameter` violation.
- **Related**: #1749 / TD1-004 (the move that introduced the doc), #2406 /
  TD1-003, #1483.
- **Suggested Fix**: Replace "in reverse-creation order" with an enumeration of
  the four real constraints, and note that the remaining order is free once the
  device is idle. Optionally pin the four with an `include_str!` source-order
  test alongside the existing ones in `resize.rs`.

**Folded-in LOW note (same family, no separate ID):** the `#1483` block comment
at `teardown.rs:189-208` and the `#2158` comment at `:229-238` both justify
their hoists by an "allocator-`None` Drop path (#1426 early-return, or any
future allocator-taken-early path)". At HEAD that path does not exist: `grep`
over `crates/renderer/src` + `byroredux/src` finds no assignment of
`VulkanContext::allocator` to `None` other than the final
`self.allocator.take()` at `teardown.rs:346`, and `init.rs:1459` always
constructs `allocator: Some(gpu_allocator)`. The hoists are correct and worth
keeping as defense-in-depth, but the comments assert a live hazard that isn't.

---

### Per-checklist result

| # | Item | Result |
|---|---|---|
| 1 | Reverse-order destruction, `Drop` reaches all GPU resources, allocator freed last | **Order claim is wrong (LOW #02); functional order correct.** Every one of the 42 GPU-owning `VulkanContext` fields has a reached destroy — full field↔destroy diff done. Allocator is `take()`n + `Arc::try_unwrap`ed at `:346`, after every allocator-dependent destroy, and *before* `destroy_device` at `:385`. The `alloc` passed to the helper is an `Arc` clone whose binding scope ends at `:268`, so it cannot itself defeat `try_unwrap`. Nothing needing the allocator runs after the guard: `destroy_depth_resources` (`:275`) and `mesh_registry.destroy_all` (`:320`) both re-borrow `self.allocator`, still `Some` at that point. |
| 2 | No use-after-destroy across swapchain recreate | **Clean.** Every per-FIF image set is freed with a full `drain(..)` / whole-vec loop — no `[0]`-only destroy anywhere: gbuffer (7 attachments × MAX_FRAMES_IN_FLIGHT, `gbuffer.rs:189-207` + `:449-467`), svgf (`indirect_history` + `moments_history` + `atrous_color`, `svgf.rs:1449-1478` / `:1671-1704`), taa (`taa.rs:834-846`), caustic (`caustic.rs:1189-1200`/`1294`), water_caustic (all slots via `destroy_slot`, `water_caustic.rs:423-430`), volumetrics (all six froxel vectors + both noise volumes chained in one loop, `volumetrics.rs:2762-2779`; whole pipeline destroyed+rebuilt on resize), bloom (`down_mips` chained with `up_mips` per `BloomFrame`, `bloom.rs:861-871`), composite (hdr + scene images/views/allocations + framebuffers, `composite.rs:1139-1191`), restir (`restir.rs:136-140`), frame_upscaler (`destroy_allocations` drains all three vecs). egui framebuffers rebuilt at `resize.rs:905-961` with the format-stable and format-changed arms both destroying the taken pass on failure (#2685). |
| 3 | AS cleanup on shutdown | **Clean.** `acceleration/mod.rs:332-403`: `drain_pending_destroys` first (drains BOTH `pending_destroy_blas` and `pending_destroy_scratch` regardless of countdown, `blas_static.rs:133-159`), then `blas_entries`, all `tlas[]` slots (accel + buffer + instance_buffer + instance_buffer_device), `skinned_blas` (drained unconditionally since #1138, so the App-level pre-drain at `teardown.rs:69-71` is an optimisation only), `scratch_buffers[]`, `blas_scratch_buffer`. Per-entity skin output buffers live in `skin_slots` and are released only on cell-unload/idle eviction (`skinned_blas_refit.rs:753-757`) or shutdown. |
| 4 | Other GPU SSBO/descriptor cleanup | **Clean.** `SceneBuffers::destroy` (`scene_buffer/descriptors.rs:318-372`) covers all 12 `Vec<GpuBuffer>` + 4 single `GpuBuffer`s + pool + layout — diffed against the full struct field list; the R1 material SSBO is `material_buffers` and is destroyed (`MaterialTable` itself is CPU-only). `TextureRegistry::destroy` drains `pending_destroy`, per-entry textures, the `StagingPool` via `take()` (so its allocator Arc releases), all 4 samplers, pool + layout. `EguiPass::destroy` flushes `pending_free`, destroys framebuffers then render pass, and the taken `Option<EguiPass>` drops at `teardown.rs:184` so `egui_ash_renderer::Renderer`'s Drop releases its own pool/pipeline/buffer pools while the device is alive. |
| 5 | Per-frame leaks | **Clean — 0 found.** Grep of the per-frame path (`context/draw.rs`, `post_passes.rs`, `geometry_pass.rs`, `skinned_blas_refit.rs`) returns no `allocate_descriptor_sets`, no `allocate_command_buffers`, no `create_image/_view/_framebuffer/_sampler`. The only per-frame `create_*` reachable are: `sync::recreate_image_available_for_frame` (`sync.rs:298-314`, `mem::replace` + `destroy_semaphore` of the old — error path only); `TextureRegistry::update_rgba` (`:1596-1645`, one image per animating-UI frame, retired through the `should_destroy_pending` ring which advances via `begin_frame` at `draw.rs:1792` — bounded at MAX_FRAMES_IN_FLIGHT; the *cost* is **Existing: #3429**, not re-filed); `WaterPipeline::upload_params` grow path (`water.rs:544-571`, geometric, `mem::replace` + `old_buffer.destroy`); `TlasState` resize (`acceleration/tlas.rs:817-1001`, 2× + 8192-floor so it fires on cell transitions only, and the commit point at `:985-1001` retires accel + all three buffers). Screenshot/depth-capture staging is size-gated (`screenshot.rs:258-266`, `depth_capture.rs`) and freed on shrink + at teardown. `blend_pipeline_cache` is bounded by its `(u8,u8,bool,bool)` key and drained by `destroy_render_pass_pipelines` on both format-change resize and Drop. |
| 6 | `resize.rs` vs `Drop` symmetry | **Clean.** Every resize-rebuilt resource is destroyed on the way in: main framebuffers, depth + depth-history, swapchain image views (`#654` ordering pinned by `old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`), render pass + triangle/wireframe/blend pipelines + water pipeline (all four gated on the *same* `format_changed` flag on both the destroy side `:187-212` and the create side `:299-360`), ssao, gbuffer, svgf, reservoirs, caustic, water_caustic, bloom, volumetrics, composite, egui, taa, presentation, frame_upscaler, frame_sync. Nothing is destroyed in `Drop` that resize rebuilds without destroying, and nothing is double-destroyed (every site is `take()`-then-destroy or `recreate_on_resize` with an internal drain). `depth_history_sampler`, both 1×1 placeholders, `exposure`, `image_health_buffers`, `scene_buffers`, `cluster_cull`, `gpu_timers`, `skin_compute`/`skin_palette` and `accel_manager` are extent-independent and correctly *not* touched by resize. |

### Dropped (stale premise)

**10 candidates dropped.**

1. *`CompositePipeline::destroy` never `clear()`s `param_buffers`, so the #732/LIFE-N1 allocator-Arc clones survive `Arc::try_unwrap`.* — `composite.rs:1562` does clear it; and since #927 `GpuBuffer::destroy` sets `self.allocator = None` itself (`buffer.rs:1295-1302`), so the `clear()` is belt-and-braces everywhere.
2. *`placeholder.rs` creates 2 images + 2 views + 1 sampler but destroys 1 view / 1 sampler.* — the extra `create_*` hits are the two constructors (`new_white_ao` / `new_storage_sink`) funnelling into one `create()`; the extra `destroy_image` hits are the `destroy_image` closure in the error path (`placeholder.rs:199-253`). `destroy()` at `:342-364` is symmetric.
3. *`egui_pass.rs` shows 7 `create_framebuffer` vs 4 `destroy_framebuffer` and 4 vs 5 for render passes.* — the surplus creates are the `new()` failure-cleanup arms (#2685) and `#[cfg(test)]` source-shape assertions.
4. *`exposure` / `skin_slots` / `GpuBuffer` self-Drop runs after `destroy_device` on the allocator-`None` Drop path → CRITICAL use-after-destroy.* — the allocator-`None` path does not exist at HEAD; `VulkanContext::allocator` is `Some` from `init.rs:1459` until the final `take()` in `Drop`. (Reduced to the LOW doc note folded into finding #02.)
5. *`pending_skin_unload_victims` grows unbounded when `skin_compute` is `None` (the leak `#3374` fixed for morph).* — `queue_skin_unload_victims` (`byroredux/src/cell_loader/unload.rs:612-633`) pushes only when `slot_present(eid)`, so nothing accumulates when no slots exist.
6. *TAA/SVGF history freed for slot 0 only.* — all three use `drain(..)`.
7. *The animating-menu path leaks a full-viewport `VkImage` per frame.* — the old texture goes onto `TextureEntry::pending_destroy` and is reclaimed once `current_frame_id` (bumped in `begin_frame`) advances past `MAX_FRAMES_IN_FLIGHT`. The remaining cost is **#3429**; not re-filed.
8. *Old swapchain image views leaked on resize.* — destroyed at `resize.rs:213-243`, between the new-swapchain create and the old-swapchain destroy, and pinned by a source-order test.
9. *A failed `FrameUpscaler::recreate` leaves a husk that gets double-destroyed by `Drop`.* — `recreate` calls `destroy` (which `drain(..)`s all three output vecs and `take()`s the SDK context) before `Self::new`; a failed `new` leaves empty vecs, so the later `destroy_allocations` is a no-op.
10. *`depth_history_sampler` recreated on resize without a matching destroy.* — resize only *passes* the handle (`resize.rs:288`); it is created once at `init.rs:303` and destroyed once at `teardown.rs:284-286`.

### Verified clean

- Checklist items **2, 3, 4, 5, 6** in full (see the table above).
- Checklist item **1** functionally: field↔destroy diff over all 42 GPU-owning
  `VulkanContext` fields found no unreached resource; allocator is released last
  among allocator-dependent work and before `vkDestroyDevice`; the `Arc` clone
  handed to the teardown helper is scoped so it cannot defeat `try_unwrap`.
- Parent/child destroy ordering: main framebuffers (`:260`) before both the
  render pass (`:339`) and every attachment view (guard, `:164`); pipelines
  (`:310`) before the render pass; `overlay_pipeline` (inside
  `presentation.destroy`, `:185`) before the shared `pipeline_layout` (`:316`);
  swapchain image views before the swapchain (`swapchain.rs:278-288`); swapchain
  before surface; device before surface/instance.
- Error-path cleanup in every subsystem constructor examined
  (`try_or_cleanup!`, `inspect_err`, explicit closure-based unwinds) — no
  constructor leaks a partially-built resource set.
- Leak safety nets present and correctly gated against unwind:
  `Attachment::Drop` (gbuffer), `HistorySlot::Drop` (svgf), `GpuBuffer::Drop`,
  `Texture::Drop`, `StagingPool::Drop`, `ExposureResource::Drop`.
- `MeshRegistry::destroy_all` additionally releases the in-flight #3298 chunked
  `geometry_rebuild` job's target buffers and the geometry `StagingPool`.

### Coverage gaps

- **Validation-layer confirmation not run.** Every conclusion here is
  source-derived. A `BYRO_VALIDATION=1` release run through
  `--bench-hold` + a window resize + clean exit would confirm zero
  `VUID-vkDestroyDevice-device-05137` ("device destroyed with live objects")
  and zero `VUID-vkFreeMemory-memory-00677`, which is the only real proof for
  items 1/2/6. I did not run the engine (per the no-parallel-launch rule and
  the no-speculative-Vulkan rule).
- **`egui_ash_renderer::Renderer`'s internal teardown** is third-party and was
  not read; I verified only that its `Drop` runs while the device and the
  allocator `Arc` are both still alive.
- **FSR SDK (`fsr3::Context`) internal allocations** are outside
  `gpu-allocator`'s view and outside this repo; only the drop *ordering*
  (`destroy_device_objects` before `destroy_allocations`) was verified.
- **Runtime device-lost / `VK_ERROR_DEVICE_LOST` teardown** was not modelled —
  every `destroy` in this subsystem assumes a live device, and no path in
  `draw.rs` was traced for a lost-device abort that skips `Drop`.

---

## Dimension 7 — Worker Threads & Thread-Safety Bounds


Audit: ByroRedux concurrency sweep, 2026-08-30. HEAD `64f64480`. Depth: **deep**.

Scope walked: `byroredux/src/streaming.rs` + `streaming_helpers.rs` + `app_step.rs`,
`crates/debug-server/src/{listener,system,evaluator,lib}.rs`, `crates/debug-ui/src/lib.rs`,
`crates/core/src/ecs/{storage,resource,resources/mod,scheduler}.rs`,
`crates/renderer/src/vulkan/allocator.rs` (+ every `allocator.lock()` site),
`byroredux/src/asset_provider/{mod,archive,texture,material}.rs`,
`crates/bsa/src/{archive/extract,archive/open,ba2}.rs`, `crates/ui/src/{lib,player}.rs`,
`crates/cxx-bridge/`, `crates/audio/src/lib.rs`, `crates/plugin/src/esm/records/common.rs`,
`tools/byro-dbg/src/tui.rs`, plus a workspace-wide sweep for
`thread::spawn` / `JoinHandle` / `mpsc` / `rayon` / `unsafe impl` / `Arc<Mutex|RwLock>` /
`static mut` / `AtomicPtr` / `thread_local!` / `OnceLock`.

Prior-report reconciliation: `AUDIT_CONCURRENCY_2026-08-27b.md` reported **0** Dim-7
findings, and `git diff HEAD~5 HEAD` over the Dim-7 surface touches only
`crates/core/src/ecs/components/material.rs` (+16 lines). The one finding below is
therefore in a sub-area that pass did not open (the *span* of the archive `Mutex`,
as opposed to its existence), not a regression of anything it cleared.

---

### Findings

**1 finding — 0 CRITICAL, 0 HIGH, 1 MEDIUM, 0 LOW.**

#### CONC-D7-2026-08-30-01: `Ba2Archive::extract` holds the `Mutex<File>` across zlib/LZ4 decompression, so the cell-stream worker's mesh inflate blocks the main thread's archive reads on every BA2 game

- **Severity**: MEDIUM
- **Dimension**: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds
- **Location**: `crates/bsa/src/ba2.rs:403-445` (guard acquisition + both dispatch arms), with the decompression inside `extract_general` (`:841-858`) and `extract_dx10` (`:873-895`). Sibling that does it right: `crates/bsa/src/archive/extract.rs:39-132`.
- **Status**: NEW. (Observed in passing by `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md:206` inside an `Existing: #877` entry — "BA2 path is worse than BSA-compressed (which drops the lock before zlib/LZ4)". #877 is CLOSED, and its fix was the two-phase serial-extract/parallel-parse split *inside* the worker; it never touched this lock span and does not cover the main-thread↔worker case. No open issue in `/tmp/audit/concurrency/issues.json` covers it.)
- **Trigger Conditions**: Any BA2-backed game (FO4 / FO76 / Starfield) in exterior streaming. The cell-stream worker is inside Phase 1's serial extract loop (`byroredux/src/streaming.rs:1358-1364`) on a compressed GNRL mesh or a multi-chunk DX10 entry; concurrently the main thread calls `extract_mesh` on the *same* `Arc<TextureProvider>` — the LOD-band reconcile (`byroredux/src/cell_loader/terrain_lod_btr.rs:218`, `object_lod.rs:272`), the sync REFR loader (`cell_loader/references/synth_child.rs:519`), or resumable NPC spawn (`npc_spawn/resumable.rs:644,684,849,981,1068,1158,1195,1243`). Both go through the one `Vec<Archive>` in `TextureProvider.mesh_archives` (`asset_provider/texture.rs:7-10, 57-65`) and therefore the one `Mutex<File>` per archive.
- **Verification Path**: `cargo test` cannot see it (no timing assertion exists). Confirming signal is a timing measurement, not a validation layer: run `--game fo4 --grid <x>,<y> --radius 3 --bench-frames 300 --bench-hold` and compare `StreamingTelemetry`'s apply-slice percentiles / `CpuFrameTimings` against a build with the guard released before `decompress_chunk`. The static half is already verifiable by reading the two files side by side.
- **Description**: `BsaArchive::extract` deliberately releases the file guard before inflating, with the rationale spelled out in place:
  > "Drop the lock before the decompression CPU work — the file handle isn't needed for decompression and other extracts shouldn't have to wait." (`archive/extract.rs:128-132`)

  `Ba2Archive::extract` never adopted that. It binds `let mut file = …lock()` at `:403` and passes `&mut *file` straight into `extract_general` / `extract_dx10`, both of which do `seek → read_exact → decompress_chunk` with the guard still alive; `extract_dx10` does it once **per mip chunk** in a loop. The guard is only released when `extract` returns, i.e. after the whole texture/mesh has been inflated. The file handle is not needed for any of that work — `decompress_chunk` operates on the already-read `packed: Vec<u8>`.
- **Evidence** (current file, `crates/bsa/src/ba2.rs`):
  ```
  403        let mut file = match self.file.lock() {
  ...
  414        match entry {
  419            } => extract_general(
  420                &mut *file,                      // guard still held
  ...
  433            } => extract_dx10(
  434                &mut *file,                      // guard still held
  ...
  841 fn extract_general<R: Read + Seek>(
  849     reader.seek(SeekFrom::Start(offset))?;
  855         let mut packed = vec![0u8; packed_size as usize];
  856         reader.read_exact(&mut packed)?;
  857         decompress_chunk(&packed, unpacked_size as usize, compression)   // <-- under the lock
  ...
  873 fn extract_dx10<R: Read + Seek>(
  881     for chunk in chunks {
  882         reader.seek(SeekFrom::Start(chunk.offset))?;
  ...
  890             let buf = decompress_chunk(&packed, chunk.unpacked_size as usize, compression)?;  // <-- per chunk, under the lock
  ```
  Contrast, `crates/bsa/src/archive/extract.rs`:
  ```
  128            // Drop the lock before the decompression CPU work — the file
  129            // handle isn't needed for decompression and other extracts
  130            // shouldn't have to wait.
  132            drop(file);
  ```
- **Impact**: Priority inversion, main thread behind a background worker. The main-thread callers above run inside the per-frame `STREAMING_APPLY_BUDGET` / LOD reconcile budget, so the stall lands directly in frame time as a hitch whose length is a whole macro-mesh or DX10 mip chain's inflate — precisely the class of work Starfield's `.bto`/`.btr` and DX10 texture entries make large. It is not a correctness bug: the `Mutex` still serialises every `seek`+`read_exact` pair correctly, and nothing is torn. FNV / FO3 / Oblivion / Skyrim(-SE) are unaffected (BSA path already correct); FO4 / FO76 / Starfield are.
- **Related**: `#877` (closed — the intra-worker two-phase split that sidesteps *rayon* contention but not main↔worker contention); `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md:204-206`; `#1170` (poison recovery, which touched the same two functions and kept them asymmetric); `byroredux/src/streaming.rs:1330-1345` (the comment block that reasons about this mutex and correctly describes the BSA behaviour, but does not distinguish the BA2 case).
- **Suggested Fix**: Restructure `extract_general` / `extract_dx10` so the *read* half takes the `&mut File` and returns the packed bytes, and the *decompress* half runs after `Ba2Archive::extract` drops the guard — mirroring `archive/extract.rs:132` exactly, so the two backends stop diverging. For `extract_dx10` that means reading every chunk's packed bytes under one guard hold, then inflating and concatenating outside it.

---

### Dropped (stale premise)

**13 candidates dropped** — each was checked against the file at HEAD and disproved.

1. **Screenshot readback races the copy's submission across frame-in-flight slots.** `screenshot_pending_readback` is a single `Option`, not per-slot, so a copy recorded on slot A looked like it could be read back after only slot B's fence. Disproved: `context/draw.rs:1624-1637` waits on **both** `in_flight[frame]` and `in_flight[(frame+1) % MAX_FRAMES_IN_FLIGHT]`, and `sync.rs:46-48` `const_assert`s `MAX_FRAMES_IN_FLIGHT == 2`, so the dual wait is device-idle for all prior frames. `screenshot_finish_readback` at `:1706` is fence-proven. (The `(f+1) %` scan pin is #3442's, already filed.)
2. **BSA `extract` holds the file mutex across inflate.** Disproved — explicit `drop(file)` at `crates/bsa/src/archive/extract.rs:132` with rationale in place. Only the BA2 sibling does (finding #1).
3. **`WorldStreamingState` field-drop order detaches the worker before the channel closes (#1167 regressed).** Disproved: `Drop::drop` (`streaming.rs:966-970`) delegates to `shutdown`, which does `self.worker.take()` (`:942`) → `self.request_tx.take()` (`:952`) → `join_with_timeout` (`:953`) — sender dropped before the join, and a second entry short-circuits on `worker: None`. `join_with_timeout` (`:1002-1027`) polls `is_finished` and drops (detaches) only on deadline.
4. **`unsafe impl Send` / `unsafe impl Sync` without a safety argument.** Disproved: there are **zero** `unsafe impl Send`/`Sync` in the workspace. The only three `unsafe impl`s are `AnyBitPattern` (`crates/nif/src/blocks/bs_geometry.rs:350-352`, plus the macro at `stream.rs:49-56`), each carrying a correct safety comment; `BoneWeight`/`Meshlet`/`CullData` are verified padding-free `#[repr(C)]` scalar aggregates.
5. **The worker reaches `merge_external_material` / `MaterialProvider`.** Disproved: the free fn has exactly four production call sites — `scene/nif_loader.rs:284`, `cell_loader/partial.rs:81`, `cell_loader/precombined.rs:314`, `cell_loader/references/import.rs:94` — all on the main-thread drain/apply path. `mat_provider` is a bare field on `WorldStreamingState` (`streaming.rs:597`), never `Arc`'d, never placed in a `LoadCellRequest`.
6. **`Arc<TextureProvider>` is mutated after being shared.** Disproved: no `Arc::get_mut` / `Arc::make_mut` anywhere in `byroredux/src`; `TextureProvider` is two `Vec<Archive>` populated only in `build_texture_provider` before the `Arc::new` at `streaming.rs:779`. `extract`/`extract_mesh`/`has_mesh`/`has_texture` all take `&self` and mutate nothing.
7. **An allocator guard spans a `queue_submit`.** Disproved: the only two `queue_submit` calls are `texture.rs:816` and `draw.rs:3841`; every `allocator.lock()` site in the renderer is either a statement-scoped temporary (`ssao.rs:156`, `buffer.rs:348`, `composite.rs:1220/1292`, `exposure.rs:86`, `helpers.rs:487`, all the `…free(a).ok();` sites) or a `let`-bound guard in a function with no submit (`allocator.rs:236`, `:309`; the `Drop` impls at `texture.rs:628`, `buffer.rs:1659`). `texture.rs:801-818` additionally scopes the *queue* guard to the submit only, not the fence wait (#1713).
8. **The ESM `thread_local!` localization flag is invisible to a parse running on a worker.** Disproved: `crates/plugin` contains no `rayon`, no `thread::spawn`, and no channel; ESM parsing is main-thread-only, and both thread-locals (`records/common.rs:13-30`) are RAII-guarded (`LocalizedPluginGuard`, `StringsTableGuard`). The streaming worker consumes an already-built `record_index`, never the parser.
9. **`WorldStreamingState.pending` grows unboundedly under rapid boundary crossings**, backing up the unbounded `payload` channel. Disproved: `app_step.rs:162-164` prunes via `streaming::stale_pending_coords(&state.pending, player_grid, state.radius_unload)` every tick, and `queue_loads` (`streaming.rs:876-908`) suppresses duplicates against `loaded`/`pending`.
10. **Ruffle's `Player` / wgpu device is reachable as an ECS `Resource` (which would require `Sync`).** Disproved: `crates/ui/src/lib.rs:6` — "UiManager is NOT an ECS Resource because Ruffle's Player is not Send+Sync"; there is no `impl Resource for UiManager`. `SwfPlayer.player: Arc<Mutex<Player>>` is `!Send` if `Player` is, so it cannot cross a thread boundary even by accident. The one process-wide `OnceLock<Arc<Descriptors>>` (`player.rs:95`) compiles only because wgpu's `Descriptors` is `Send + Sync`.
11. **Debug-client wire desync when the 5 s `recv_timeout` fires and the drain later sends the real response.** Disproved: `rx` is a per-command local re-bound each loop iteration (`listener.rs:308`), so it is dropped before the next `wire::decode`; the drain's later `response_tx.send` returns `Err` and is discarded (`system.rs:186`). Exactly one response per request reaches the socket.
12. **Nested-rayon deadlock from `stream_pool.install(…)` on the cell-stream worker.** Disproved: the calling thread (`byro-cell-stream`) is not a member of any rayon pool, so `install` is a plain block-until-done; the private pool (`build_stream_parse_pool`, `streaming.rs:1050-1060`) has an independent registry from the global one the scheduler uses.
13. **cxx-bridge FFI pointer lifetimes unbounded.** Disproved: the entire bridge is `fn native_hello() -> String` (`crates/cxx-bridge/src/lib.rs:7-15`, `cpp/native_utils.cpp`). No pointers, no handles, no shared state.

---

### Verified clean

**Checklist 1 — Streaming Drop ordering (#1167): CLEAN.**
`shutdown(&mut self, timeout)` (`streaming.rs:938-963`) takes the `JoinHandle` first so the
`Drop` safety net observes `worker: None`, then takes and drops `request_tx` (closing the
worker's `recv` loop), then joins with the bounded poll (`join_with_timeout`, `:1002-1027`,
10 ms cadence, drop-on-deadline = detach). `Drop` (`:966-970`) delegates with a 1 s timeout.
The `Option<Sender>` field shape is hidden behind `send_request` (`:864-870`), which returns
`Err(SendError)` post-shutdown; `queue_loads` rolls back the `pending` entry on that error
(`:900-905`). `payload_rx` is still alive during the join (it drops with the struct fields
after `Drop::drop` returns), and `mpsc::Sender::send` on an unbounded channel never blocks,
so a worker mid-`send` cannot wedge the join.

**Checklist 2 — Worker ↔ main data flow: CLEAN except finding #1's lock span.**
- Payload crosses on `mpsc::Sender<LoadCellPayload>`; no `&mut World`, no `&World`, and no
  raw pointer is captured by `cell_pre_parse_worker` (`:1071-1116`). The request carries only
  `Arc<ExteriorWorldContext>`, `Arc<TextureProvider>`, `Arc<HashSet<String>>` and plain
  scalars.
- `PartialNifImport: Send` is enforced at the declaration site by the `const _: fn() = ||`
  assert at `streaming.rs:573-579` (#1171), so a future non-`Send` field fails at the struct
  rather than at the distant channel send.
- Both archive backends still serialise `File` access: `BsaArchive.file: Mutex<File>`
  (`crates/bsa/src/archive/mod.rs:49`), `Ba2Archive.file: Mutex<File>` (`ba2.rs:124`). Every
  `seek`/`read_exact` pair is inside the guard on both paths — **no seek escapes the lock**,
  which is the data-race question. Both recover from poison rather than re-panicking (#1170).
  Texture and mesh archives are separate `Archive` instances with separate `File` handles even
  when the same path is passed twice (`open_with_numeric_siblings`, `archive.rs:365-390`), so
  a main-thread *texture* read never contends a worker *mesh* read.
- `merge_external_material` is main-thread-only (see Dropped #5); `MaterialProvider` is not
  shared with the worker.
- NIF import cache: the worker gets a read-only `Arc<HashSet<String>>` built once per crossing
  by `NifImportRegistry::snapshot_keys` (`cell_loader/nif_import_registry.rs:429-431`) at
  `app_step.rs:178-182`; write-back (`touch_keys` / `accumulate_hits` / insert) is entirely on
  the main-thread drain.
- Panic containment is two-layer: per-NIF `catch_unwind` in `parse_one_nif` (`:1219-1235`) and
  whole-cell `pre_parse_cell_panic_safe` (`:1125-1143`), so neither a parser panic nor a
  rayon-propagated panic can kill the worker and silently disable streaming (#854).

**Checklist 3 — Debug server: CLEAN, modulo the two already-filed issues.**
- Per-client threads (`handle_client`, `listener.rs:246-341`) never touch `World`; they decode,
  `try_enqueue_command`, block on a per-command `mpsc::Receiver`, and write bytes back.
- The queue is bounded at `MAX_QUEUED_COMMANDS = 64` (`:44`) with an atomic check-and-push
  under one lock hold (`try_enqueue_command`, `:71-86`), so two clients cannot both slip past
  the cap; over-cap requests are rejected synchronously with an "overloaded" response
  (`:311-321`). Per-client backpressure is naturally 1-in-flight.
- `DebugDrainSystem` is registered `add_exclusive(Stage::Late, …)` (`crates/debug-server/src/lib.rs:34`),
  and drains with `std::mem::take` under a short lock hold (`system.rs:135-141`).
- Screenshot handoff is fence-proven (Dropped #1) and hardened on the ownership side:
  `try_claim` CAS (`resources/mod.rs:126-137`), owner-gated `take_result_for` (`:172-186`),
  and the `generation` counter (#1603) that invalidates a cancelled-then-resumed straggler
  readback (`:190-210`, `:236-241`). `DepthCaptureBridge` deliberately omits the generation,
  with the reasoning recorded at `:55-63`.
- Listener shutdown: `shutdown_and_join` (`:117-141`) sets the flag, `shutdown(Both)`s every
  live client socket via the `Weak<TcpStream>` registry (#1009) *before* joining the listener,
  and the post-accept shutdown check is folded into the `active_streams` critical section
  (#1172, `:210-221`) so no thread is spawned after the flag flips.
- **Already filed, not re-reported**: #3090 (cancelled screenshot's early `return` at
  `system.rs:72-78` skips that frame's whole command drain — still present verbatim) and
  #3449 (uncapped OS thread per connection at `:228-232`).

**Checklist 4 — Allocator sharing: CLEAN.** See Dropped #7. `SharedAllocator = Arc<Mutex<vulkan::Allocator>>`
(`allocator.rs:15`); all dispatch is single-threaded inside `draw_frame`; no holder keeps the
guard across either `queue_submit`.

**Checklist 5 — `Send + Sync` bounds: CLEAN.**
- `Component: 'static + Send + Sync + Sized` with `type Storage: … + Send + Sync + 'static`
  (`crates/core/src/ecs/storage.rs:17-18`); `Resource: 'static + Send + Sync`
  (`resource.rs:13`) behind `RwLockReadGuard<Box<dyn Any + Send + Sync>>`; `System: Send + Sync`
  (`system.rs:15`). Storage is reachable only through the World's query/resource guards.
- No `unsafe impl Send`/`Sync`, no `static mut`, no `AtomicPtr` anywhere in the workspace.
- The two process-wide caches in `asset_provider/material.rs` (`sf_cdb_cache` at `:158-160`,
  capped at `SF_CDB_CACHE_MAX_ENTRIES = 128`; the `WARNED` dedup set at `:967-970`) are
  `OnceLock<Mutex<…>>` with poison recovery (#2398) and are pure memoisation — a torn entry
  re-derives from the archive.
- Ruffle/wgpu stays off the ECS entirely (Dropped #10); `crates/audio`'s `AudioWorld` /
  `SoundCache` are `Resource`s holding only `Arc<StaticSoundData>` (kira's own backend thread
  is library-internal).
- cxx-bridge has no FFI lifetime surface at all (Dropped #13).

**Checklist 6 — Workspace thread/channel sweep: CLEAN.** Exactly five production
`thread::spawn`/`thread::Builder` sites exist (everything else is test code):
`streaming.rs:773` (cell-stream worker — joined with timeout, above), `listener.rs:169`
(debug listener — joined by `DebugServerHandle::Drop`), `listener.rs:228` (per-client —
deliberately detached, documented contract #855, thread count is #3449), and
`tools/byro-dbg/src/tui.rs:388` (CLI net thread — exits when either channel closes; not joined,
but it is a short-lived CLI process, so no leak survives the tool). Rayon appears in exactly
two production dispatch shapes: the scheduler's `par_iter_mut().for_each` over `&World`
(`scheduler.rs:497-500`, which is what `World: Sync` exists for, with the fail-fast panic
policy documented at `:420-473`) and the streaming worker's private pool
(`streaming.rs:1050-1060`, `:1204-1240`). Neither captures a raw pointer or a `&mut World`.
No `crossbeam`. `crates/debug-ui/src/lib.rs` has no threading surface whatsoever (zero
`Mutex`/`RwLock`/`Arc`/thread references).

---

### Coverage gaps

- **No runtime confirmation.** No engine process was launched (per the no-parallel-launch
  rule), so finding #1's frame-hitch magnitude is reasoned from the code shape, not measured.
  The confirming run is named in the finding's Verification Path and has not been executed.
- **BA2 archives were not opened.** The finding's severity assumes real FO4/Starfield entries
  are compressed (GNRL `packed_size != 0`) and multi-chunk DX10; that is the format's normal
  shape and is what `decompress_chunk`'s LZ4/zlib arms exist for, but no census over an
  installed `* - Main.ba2` was run to quantify the fraction, and no number is invented here.
- **`crates/debug-server/src/evaluator.rs` was read only for its threading contract**
  (it runs exclusively inside `DebugDrainSystem::run` on the main thread). Its internal
  lock-acquisition order is Dimension 3's, and #3445 already covers its
  `Name → StringPool` edge.
- **`crates/sdk`, `crates/mod-runtime`, `crates/physics`, `crates/save`, `crates/facegen`,
  `crates/hkx`** were covered only by the workspace-wide primitive sweep (which found no
  thread, channel, or `unsafe impl` in any of them). Their internals are not claimed as
  audited by this dimension.
- **`crates/fsr3-sys` FFI lifetimes** — out of this dimension's scope (`/audit-safety` Dim 1);
  only its `OnceLock<bool>` env cache was seen by the sweep.

---

# Consolidated Verification Status

## What was actually executed

| Check | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` | **FAILED — 1642 passed, 5 failed** (all `ragdoll::tests::*`). Reproduced independently by the orchestrator. This is CONC-D5-2026-08-30-01. |
| Same, `-- --skip combat_approach_line_of_sight` | **ok — 1645 passed, 0 failed.** Isolates the single cycle-closing edge. |
| `cargo test -p byroredux --bin byroredux scheduler_access` | **15 passed, 0 failed**, incl. `build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, `player_wind_read_is_declared_and_weather_writer_is_exclusive`, `submersion_runs_after_camera_follow_and_before_water_audio`. |
| `cargo test -p byroredux-renderer --lib -- one_time_lock_scope_tests vulkan::sync::tests` | **7 passed** — the #1713 queue-guard-scope pin is green (which is what makes CONC-D1-02's doc line false). |

## What was NOT executed — and what would confirm it

- **No GPU / Vulkan run.** `BYRO_VALIDATION=1` on a **release** build with Synchronization
  Validation is the cheapest confirming channel for D1-01, D2-01, D2-02, D2-03 and D6-01, and none
  of those five should be shipped before it. Each finding names its exact expected string:
  - D2-01 → `SYNC-HAZARD-READ-AFTER-WRITE` naming the `SkinSlot::output_buffer` `VkBuffer` at the
    `vkCmdDispatch` in `CausticPipeline::dispatch`, `prior_access = SYNC_COMPUTE_SHADER_SHADER_STORAGE_WRITE`.
    Needs a cell satisfying all three trigger conditions at once (skinned actor + live TLAS +
    caustic-source hit) — e.g. FO4 `DmndDugoutInn01` or a Skyrim interior with water.
  - D2-02 → `SYNC-HAZARD-READ-AFTER-WRITE` on the caustic accumulator `VkImage`,
    `prior_access = SYNC_COPY_TRANSFER_WRITE`.
  - D2-03 → `SYNC-HAZARD-WRITE-AFTER-WRITE` on `integrated_volumes[f]`, same prior access.
  - D6-01/D6-02 → a clean-exit + window-resize release run showing zero
    `VUID-vkDestroyDevice-device-05137` and zero `VUID-vkFreeMemory-memory-00677`.
  Per the project rule, no engine instance was launched (the user may have their own running).
- **`BYRO_LOCK_ORDER_CHECK=1` was run only over `-p byroredux --bins`**, not `--workspace`, and the
  detector is reachability-bounded: paths no test drives (most of `cell_loader::spawn`, the live
  `app_step.rs` steppers, `save_io`'s live reload) contribute no edges and are therefore neither
  cleared nor flagged. D3's and D5's non-executed cycle claims (D3-01/02/03, D5-02) are derived
  from reading both acquisition sites, not from an observed panic.
- **CONC-D4-2026-08-30-01's visible symptom was reasoned from the stage table, not observed.**
  The code-level ordering claim (`PostUpdate = 2` executes before `Late = 4`; the billboard system
  is a PostUpdate exclusive; `camera_follow_system` is the sole Character-mode pose author and is
  Late-parallel) is verifiable without a device and was checked; the "visible shear on fast yaw"
  impact estimate was not.

## Coverage gaps carried forward

1. **47 of 57 exclusive systems are undeclared** (#3473), so the cross-stage producer/consumer
   sweep that produced D4-01 and D4-02 could only be run mechanically over the 19 declared
   systems. The ~39 remaining scripting/AI exclusives were not individually traced. This is the
   concrete coverage cost of #3473 and is the strongest argument for closing it.
2. **Three of five stages hold exactly one parallel system**, so `analyzed_pair_count()` is 7 and
   the zero-conflict proof actually constrains only six declarations. Already pinned (#2393); it
   bounds how much assurance the green KPI carries, and is not itself a finding.
3. **`CONC-D1-H1` from the 2026-08-12 sweep is untracked.** `recreate_image_available_for_frame`
   destroying a semaphore with a pending acquire signal
   (`VUID-vkDestroySemaphore-semaphore-05149`) was raised as a hypothesis, never settled, and
   appears in **no open GitHub issue**. Not re-filed here (prior-report overlap) but flagged so it
   is not lost a third time. Do not change that code before a fault-injected validation run.
4. **`transfer_pool` is a bare `vk::CommandPool` with no external-synchronisation wrapper.** Every
   caller is main-thread today so nothing races, and it is not a finding at HEAD — but if a
   streaming-upload thread is ever added, the *pool*, not the queue, is the next thing that needs
   a lock.
5. **Static analysis limits in D3.** Guards moved into closures/iterator adapters and dropped
   elsewhere, and guards crossing a trait-object boundary (`ConsoleCommand::execute`,
   `SaveRegistry`'s `SaveFn`/`ValidateFn` pointers, `Box<dyn System>`), are not followed precisely;
   the call-graph closure stops at function-pointer dispatch.
6. **`draw.rs` (4,900+ lines) was read in ranges**, covering the fence/acquire/AS/submit/present
   spine and every error exit in the acquire→submit window. Passes with no queue or AS interaction
   were not read.
7. **FSR / `frame_upscaler.rs` / `presentation.rs` internals not audited** — the FFX SDK's internal
   barriers are opaque to source reading and the surrounding `scene_color` barriers are #3247.

---

# Deduplication

- Baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200` (2026-08-30), saved to
  `/tmp/audit/concurrency/issues.json`.
- Prior reports scanned: all `docs/audits/AUDIT_CONCURRENCY_*.md` (2026-08-12 through 2026-08-27b),
  plus `AUDIT_CONCURRENCY_UI_2026-08-12.md`.
- Same-day sibling audits cross-checked to avoid double-filing: `AUDIT_ECS_2026-08-30.md`,
  `AUDIT_SAFETY_2026-08-30.md`, `AUDIT_RENDERER_2026-08-30.md`.
  Three candidates that overlap `AUDIT_ECS_2026-08-30.md` (ECS-D1-01 `scene_centroid_distance`,
  ECS-D1-02 `track_read`'s recursive-read fast path skipping `record_and_check`, ECS-P2-01/02) were
  independently confirmed at HEAD but are recorded as overlap in Dimension 3 rather than re-filed.
  Note for the publisher: the *cycle* reading of ECS-P2-01 is false — the finding as filed
  (single hold-across-call, no reverse edge, MEDIUM) is the accurate one.
- Open issues encountered and correctly skipped: #3090, #3247, #3249, #3429, #3442, #3444, #3445,
  #3446, #3449, #3473, #3493.

## Suggested labels for `/audit-publish`

`concurrency` for the CPU-side lock-ordering / scheduler / `RwLock` findings (D3, D4, D5, and D7's
CPU-side mutex span); `sync` for the GPU-side semaphore/fence/barrier findings (D1, D2, D6).
D7-01 additionally warrants `import-pipeline` (BSA/BA2 readers have no label of their own) plus
`performance`, and `game:fo4` / `game:fo76` / `game:starfield`. Doc-rot rows (D1-02, D1-03, D3-04,
D4-03, D5-03, D6-02) take `documentation` + `doc-rot`.

---

# Next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-30.md
```

Recommended fix order, by evidence strength rather than severity alone:

1. **CONC-D5-2026-08-30-01** — the only finding backed by a reproducible test failure, and it is
   turning the `lock-order-check` CI job red, which masks every subsequent concurrency regression.
   The fix is a scope narrowing at one site (`byroredux/src/commands/view.rs:175-215`).
2. **CONC-D5-2026-08-30-03 + CONC-D3-2026-08-30-04** — add `PhysicsWorld` and the three undocumented
   clusters to `docs/engine/ecs.md`'s canonical table. Cheap, and it is the shared root cause of
   five of this sweep's findings.
3. **CONC-D2-2026-08-30-01** — but run `BYRO_VALIDATION=1` first. The suggested change is an
   additive dst-stage widening; the value of doing it after the run is the before/after hazard
   count, which is also what would justify the proposed source-assert test (an include-graph trace
   demonstrably cannot catch an inline deref).
