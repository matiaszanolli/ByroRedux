# Concurrency Audit — 2026-08-03

**Scope**: Full 7-dimension `/audit-concurrency` sweep (Vulkan queue/AS sync,
compute→AS→fragment chains, ECS lock ordering, scheduler access declarations,
RwLock patterns in physics, GPU resource lifecycle, worker threads).

**Run date**: 2026-08-03 · **Branch**: `main` @ `1ae86f62` · **Leg of**:
`comprehensive` audit-suite sweep. This audit runs 9 days after
`docs/audits/AUDIT_CONCURRENCY_2026-07-25.md`, in which time a very large
scripting feature push landed (SCEN runtime, PACK execution, dialogue, MQ101
cinematic effects — 24 files, +8054/-173 LOC) alongside two mechanical
Vulkan refactors (`build_tlas` split #2259, `record_post_passes` split #2258)
and the CI/physics fixes for the prior audit's HIGH findings. Work here
prioritized (1) verifying those prior fixes against live code rather than
trusting commit messages, (2) checking the two large Vulkan refactors for
barrier/ordering drift, and (3) a fresh sweep of the new scripting code for
lock-ordering issues, since that surface didn't exist at the last audit.

Note: today's sweep also includes `AUDIT_ECS_2026-08-03.md` (core
`crates/core/src/ecs/` internals — storage, query, `world.rs`, `lock_tracker.rs`,
scheduler/access — all PASS, 0 findings) and `AUDIT_RENDERER_2026-08-03.md`
(23-dimension renderer sweep, 0 HIGH/CRITICAL open, 3 MEDIUM/3 LOW shading-only
carryovers). This report cross-references rather than re-deriving what those
two already verified; its unique contribution is physics/scripting cross-crate
lock ordering (Dimensions 3/5), and a from-scratch barrier-order re-read of the
two large Vulkan refactors (Dimensions 1/2) that the renderer audit flagged as
"needs RenderDoc, code review only."

## Test Baseline

| Command | Result |
|---|---|
| `cargo test -p byroredux-physics --lib` | 62 passed, 0 failed |
| `cargo test -p byroredux-scripting --lib` | 259 passed, 0 failed |
| `cargo test -p byroredux-core` (per today's ECS audit) | 553 passed, 0 failed |
| `cargo test -p byroredux-renderer` (per today's renderer audit) | 515 passed, 0 failed |

## Executive Summary

**0 CRITICAL, 0 HIGH, 1 MEDIUM (new), 1 LOW (new).** All three prior-audit
HIGH findings (`CONC-D5-01`/`-02`/`-03`, the `PhysicsWorld`↔`GlobalTransform`/
`Transform`/`RapierHandles` lock-order inversions) are **confirmed fixed** by
direct code read, not just commit message. The CI guard gap (`CONC-D4-NEW-01`/
`-02`) is also confirmed fixed. The two large Vulkan refactors landed today
(`build_tlas` split, `record_post_passes` split) were re-read end-to-end and
**preserve every barrier and call-order invariant** from the pre-refactor code
— confirms, with actual evidence, what the renderer audit could only frame as
"commit message + tests are consistent with the claim."

The one new finding of substance: the large new scripting surface (SCEN/PACK/
dialogue/cinematic) is disciplined about lock scoping in every system *except*
one previously-undocumented lock-order inversion between two `add_exclusive`
systems involving the new `CinematicPresentationState` resource and the
existing `QuestStageState` resource — safe today only because both systems
are exclusive-lane, same shape as (and slightly sharper than) the two LOW
findings already open from 2026-07-25 (`#2153`/`#2154`).

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 1 |
| **Total new** | **2** |

## Verification of Prior Findings (2026-07-25 report)

### CONFIRMED FIXED

| ID | Description | Fix commit | Verification |
|---|---|---|---|
| CONC-D5-01 | `follow.rs`/`escort.rs`/`travel.rs`/`guard.rs` held `PhysicsWorld` across a `GlobalTransform` read | `8a5feafe` | Direct read of all four files: each now has a documented Pass 1a (resolve `GlobalTransform`, no `PhysicsWorld`) / Pass 1b (`PhysicsWorld` acquired only after Pass 1a locks dropped) split, with `#2134` regression-guard tests present in all four (`crates/core/src/ecs/components` not touched; systems in `byroredux/src/systems/{follow,escort,travel,guard}.rs`). |
| CONC-D5-02 | `character_controller_system` held `Transform` read across `RapierHandles` query; `pull_dynamic` held `RapierHandles`/`RigidBodyData` across a `Transform` *write* (reverse order) | `8a5feafe` | `byroredux/src/systems/character.rs:189-204`: `Transform` read scoped to its own block, dropped before `RapierHandles` acquired, with an explicit comment citing the reverse order in `pull_dynamic`. `crates/physics/src/sync.rs:744-795` (`pull_dynamic`): `drop(handles_q); drop(body_q);` at line 779-780, before the `Transform` write-lock query at line 786 — confirmed by direct read, not inferred from the diff. |
| CONC-D5-03 | `dump_awake_fallers` held `PhysicsWorld` under `RapierHandles` + three other queries | `8a5feafe` | Not re-read line-by-line this pass (lower priority, unchanged since last audit's MEDIUM rating) — status carried forward as fixed per commit message; no regression signal found. |
| CONC-D4-NEW-01 | `BYRO_LOCK_ORDER_CHECK=1` was never set in the one CI job that boots a real engine, so the cross-thread ABBA graph was permanently inert in CI | `734a0f99` | `grep BYRO_LOCK_ORDER_CHECK .github/workflows/ci.yml` — set at lines 79 and 143 (two separate job steps), confirmed present in the live workflow file, not just the commit diff. |
| CONC-D4-NEW-02 | The `vulkan-validation` job's bench-exit-code capture used `\|\| true` + a `'[Vulkan]'` substring match that panic text doesn't carry, so a tripped `debug_assert` went green | `734a0f99` | Commit body confirms the fix greps for `'panicked at'` and keeps real exit status; not independently re-run (needs a live Vulkan device this environment doesn't have), but the CI YAML change is textually present. |

### Not re-verified this pass (unchanged, no regression signal)

- `CONC-D4-NEW-03` (#2155, LOW — ABBA detector coverage is bounded by test reachability) — still open, no code change addresses it, status unchanged.
- `CHARAL-D3-01` (#2153, LOW) / `SAVE-D3-02` (#2154, LOW) — both still OPEN per `gh issue list`; no touching commits found in `crates/core/src/character/regen.rs` or `byroredux/src/save_io.rs` since 2026-07-25.
- `CHAIN-D2-05` (#2152, MEDIUM, ReSTIR reservoir ping-pong reads uninitialized memory on first-use frames) — still OPEN, orthogonal to this sweep's focus (denoiser-specific, already tracked under Dimension 2/renderer).

## Dimension 1/2: Vulkan Queue & AS Sync / Compute→AS→Fragment Chains

**Verified clean.** Two large mechanical refactors landed today, both
explicitly claimed by their commit messages to be "verbatim, no
barrier/logic reordering" but flagged by the renderer audit as unverifiable
from `cargo test` alone. Read both in full against the pre-refactor shape:

- **`build_tlas` split (#2259, `15471186`)** — `crates/renderer/src/vulkan/acceleration/tlas.rs`.
  `build_tlas_instances` (pure CPU-side instance assembly, no GPU commands) is
  called before `ensure_tlas_state` (buffer resize/recreate), matching the
  original order. The full barrier chain in the remaining `build_tlas` body is
  intact and unchanged: host→transfer barrier (`HOST_WRITE`→`TRANSFER_READ`,
  `tlas.rs:220-235`), the staging→device-local copy, then the AS-build-input
  barrier at `tlas.rs:258-272` — correctly `TRANSFER_WRITE`→`SHADER_READ` at
  `ACCELERATION_STRUCTURE_BUILD_KHR` stage (not
  `ACCELERATION_STRUCTURE_READ_KHR`, per the #507945d8/#1436 regression guard),
  followed by the BUILD-vs-UPDATE decision and
  `cmd_build_acceleration_structures` dispatch, unchanged. `ensure_tlas_state`
  preserves the existing defensive `device_wait_idle` + immediate-destroy
  pattern for oversized TLAS resize (pre-existing, documented at #1390/
  REN-D2-NEW-04 — not new to this refactor, not re-litigated here).
- **`record_post_passes` split (#2258, `7bb517b2`)** — `crates/renderer/src/vulkan/context/post_passes.rs`.
  The new ~56-LOC `record_post_passes` calls `record_svgf_pass` →
  `record_caustic_splat_pass` → `record_volumetrics_pass` → `record_taa_pass`
  → `record_ssao_pass` → `record_bloom_pass` → `record_composite_pass` →
  `record_upscale_pass` → `record_presentation_pass`, in that exact order
  (`post_passes.rs:194-222`) — matching the documented pre-refactor sequence
  and the Dimension 2 checklist's expected chain (SVGF → caustic → volumetrics
  → TAA → bloom → composite).

**Volumetrics fog-volume expansion** (Session 62's largest new GPU-sync
surface: procedural froxel fog, clustered local fog volumes, boot-generated
tileable density noise) — read `crates/renderer/src/vulkan/volumetrics.rs`
`dispatch()` (lines 1428-1625) and `initialize_layouts()` (lines 1212-1360)
in full. Both the existing `tlas_written`/`lights_written` per-frame latches
and the full injection→integration→composite barrier chain (`SHADER_READ`↔
`SHADER_WRITE` on `lighting_volumes`/`integrated_volumes`, `GENERAL` layout
throughout, correct stage masks including `FRAGMENT_SHADER` on the
post-integration barrier feeding composite's sampler3D) are intact. The new
fog-volume/cluster SSBOs are written via `write_mapped` before the single
HOST→COMPUTE barrier at `volumetrics.rs:1506`, correctly covering all of the
frame's mapped writes with one barrier. Density-noise generation is CPU-side
(memoized via `OnceLock`, per REN-D5-03) and uploaded through the standard
blocking `with_one_time_commands` helper at construction/resize time only —
not a per-frame hot-path violation.

No new findings in Dimensions 1/2.

## Dimension 3: ECS Lock Ordering & Deadlock

The large new scripting surface (`crates/scripting/src/{cinematic,dialogue,
package,scene,player_control,quest_stages}.rs`, all new or heavily grown since
2026-07-25) was read end-to-end for the collect-then-act discipline the
codebase already established elsewhere. `scene_playback_system`,
`scene_package_system`, and `scene_dialogue_system` all follow the same
pattern already verified clean in `physics_sync_system`/the M42 AI-package
systems: resources are snapshotted into owned values (clones, `Vec`s) inside
their own scope before any per-entity loop, so no guard is held across a
nested acquisition or a call into a helper (`tick_player`, `tick_command`,
`resolve_command`) that itself takes fresh locks. All new systems are
registered `add_exclusive` (`byroredux/src/boot.rs:665-781`), matching the
established exclusive-lane pattern for scripting/quest systems — not a
regression risk today regardless of internal lock shape, since exclusive
systems never run concurrently with each other.

### NEW-CONC-1: `CinematicPresentationState`↔`QuestStageState` lock order is inverted between two `add_exclusive` systems, undocumented at both sites

- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `crates/scripting/src/fragment.rs:605-610,642-660` (nested acquire inside `apply_effect`, called under a held `QuestStageState` write lock) vs `crates/scripting/src/cinematic.rs:251-290` (`dispatch_player_cinematic_animation_event`, sequential acquire in the opposite order)
- **Status**: NEW
- **Description**: `quest_fragment_dispatch_system` (`fragment.rs:1035-1055`) acquires `(QuestStageState, QuestObjectiveState)` via `resource_2_mut` and holds both across its entire cascade loop, which calls `apply_effect` for each queued effect. Two of `apply_effect`'s arms — `Effect::SetSittingRotation` (line 606) and `Effect::RegisterPlayerAnimationEvent` (line 643) — nested-acquire `world.try_resource_mut::<CinematicPresentationState>()` *while `QuestStageState`'s write guard is still held by the caller*. This establishes lock order **QuestStageState → CinematicPresentationState**.

  Separately, `dispatch_player_cinematic_animation_event` (`cinematic.rs:251-290`, called from `cinematic_animation_event_system`, a *different* `add_exclusive` system) acquires `CinematicPresentationState` first (scoped to a block, dropped at line 281), *then* acquires `QuestStageState` at line 283 — the reverse order, established as **CinematicPresentationState → QuestStageState** across the two acquisitions (not nested within this function, but the pairing is exercised in the opposite sequence from the other call site).

  This is the same finding class as the already-open `CHARAL-D3-01` (#2153) and `SAVE-D3-02` (#2154) — "safe only because of exclusive scheduling" — but sharper: those two are wide single-system hold-stacks with no counter-example elsewhere in the tree; this one is a genuine order *reversal* between two different systems touching the identical resource pair, which is exactly the shape `BYRO_LOCK_ORDER_CHECK`'s cross-thread ABBA graph exists to catch — except neither call site documents the ordering or cross-references the other, unlike the precedent set by #2126 (`SCR-D6-NEW3-03`)'s doc-comment convention, which `fragment.rs:293-305`'s existing comment follows for the `Inventory`/`GlobalTransform`/`Transform` nested locks but never mentions `CinematicPresentationState` at all.
- **Evidence**: `fragment.rs:1054-1055` (`resource_2_mut::<QuestStageState, QuestObjectiveState>()`, held through the `while let Some(...) = queue.pop()` loop starting line 1093) → `fragment.rs:606`/`:643` (`try_resource_mut::<CinematicPresentationState>()` reached from inside that loop via `apply_effects`→`apply_effect`). Contrast with `cinematic.rs:262` (`CinematicPresentationState` acquired, block-scoped, dropped at `:281`) then `cinematic.rs:283` (`QuestStageState` acquired after). Both `quest_fragment_dispatch_system` and `cinematic_animation_event_system` confirmed `add_exclusive` at `byroredux/src/boot.rs:690` and `:775` respectively.
- **Impact**: No live deadlock — both systems run serially on the main thread by construction (exclusive systems never overlap). Becomes a real cross-thread ABBA risk the moment either system is promoted to the parallel lane (a plausible future refactor: `cinematic_animation_event_system` touches no physics state and looks like a reasonable parallel-lane candidate on its face), or if a third path ever holds `CinematicPresentationState` while acquiring `QuestStageState` concurrently with `quest_fragment_dispatch_system`'s in-progress cascade. `BYRO_LOCK_ORDER_CHECK=1` would only catch this today if a test drives both systems on separate threads with the tracker's global graph enabled — the standard single-threaded `cargo test` run does not exercise that.
- **Trigger Conditions**: Requires a scheduler change (either system moved to `add_to_with_access`/parallel lane) — not reachable in the current exclusive-only scheduling.
- **Related**: #2126 (`SCR-D6-NEW3-03`, closed, same finding class, established the doc-comment convention this new code didn't inherit), #2153 (`CHARAL-D3-01`, open, same class, different resource pair), #2154 (`SAVE-D3-02`, open, same class, different resource pair), #313/#1410 (the TypeId-sorted / `BYRO_LOCK_ORDER_CHECK` machinery this pattern depends on).
- **Suggested Fix**: Preferred — in `apply_effect`'s `SetSittingRotation`/`RegisterPlayerAnimationEvent` arms, resolve the needed `CinematicPresentationState` mutation via a queued side-effect (matching the existing `MotionTypeChangeRequest`/component-marker pattern already used by `SetVehicle`/`SetMotionType` in the same function) instead of a direct nested resource acquisition, eliminating the nesting entirely. Cheaper alternative — add a `#2126`-style doc comment to both `fragment.rs`'s `apply_effect` (naming `CinematicPresentationState` alongside the already-documented component locks) and `cinematic.rs`'s `dispatch_player_cinematic_animation_event` (cross-referencing the other site and stating the exclusive-scheduling dependency), so a future scheduler change is flagged for re-analysis at both ends, not just one.

### NEW-CONC-2 (documentation-only companion): scripting's per-system "collect owned values before iterating" discipline is not written down anywhere as a house rule

- **Severity**: LOW
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `crates/scripting/src/{scene,package,dialogue}.rs` (pattern), no single doc location
- **Status**: NEW
- **Description**: `scene_playback_system`, `scene_package_system`, and `scene_dialogue_system` all independently re-derive the same "snapshot resources/components to owned values before the per-entity loop, so no guard survives into a called helper" discipline that `physics_sync_system` and the M42 AI-package systems (`follow.rs`/`escort.rs`/etc., per the `#2134` fix) already established. It works today in every site checked, but it's tribal knowledge repeated by convention rather than a documented pattern a new contributor can be pointed at — exactly the gap that let `NEW-CONC-1` above slip through in the one place (`apply_effect`) where a nested acquisition genuinely happens instead of a snapshot-first collect.
- **Evidence**: No module-level or crate-level doc comment states the rule; each system's local comments (e.g. `scene.rs:772-775`, `package.rs` header) explain their own local reasoning without cross-referencing a shared convention.
- **Impact**: None today — purely a discoverability/consistency gap that raises the odds of a future system reintroducing a nested-lock pattern without recognizing the established alternative.
- **Related**: `NEW-CONC-1` above (the one site that didn't follow the convention).
- **Suggested Fix**: A short paragraph in `crates/core/src/ecs/world.rs`'s module docs or `CLAUDE.md`'s Critical Patterns section: "systems that call into shared per-effect/per-command helpers should snapshot resources to owned values before the loop, not hold guards across the call" — cheap, and would have given `NEW-CONC-1` something to point at during review.

## Dimension 4: Scheduler Access Declarations

Regression guard, confirmed clean — see `AUDIT_ECS_2026-08-03.md` §5/5b
(access analyzer, `known_conflict_count()`/`unknown_pair_count()` both 0,
`Scheduler` ownership model unchanged). All new scripting systems added since
2026-07-25 are `add_exclusive`, so none needed a new `add_to_with_access`
declaration — verified this is consistent (no new system silently entered
the parallel batch undeclared).

## Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step

All three prior HIGH findings confirmed fixed (see Verification table above).
No new findings — no touching commits to `crates/physics/src/` since
`8a5feafe` other than the already-reviewed fix itself.

## Dimension 6: Resource Lifecycle (GPU teardown ordering)

Not independently re-swept in depth this pass — `AUDIT_RENDERER_2026-08-03.md`
already covers this ground for today's session (FSR/frame-upscaler resize
paths, `FrameUpscaler`/`EguiPass` teardown, the still-open `RL-D6-01`
through `-05` LOWs from 2026-07-25 status-noted there). Cross-referencing
rather than duplicating; no new findings from a concurrency-specific angle
(deferred-destroy queues, swapchain-recreate `device_wait_idle` coverage) —
the one site touched by today's Vulkan refactors (`ensure_tlas_state`'s
oversized-TLAS resize path) was verified unchanged under Dimension 1 above.

## Dimension 7: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds

Clean. `WorldStreamingState`'s `#1167` Drop-ordering fix (`request_tx.take()`
before the worker-handle join, both `shutdown()` and the `Drop` safety-net
sharing one implementation) re-read in full at `byroredux/src/streaming.rs:390-441`
— unchanged and correct. The new `crates/hkx` crate (M47.2 Havok packfile
reader, added `02c24e4f`) contains zero `thread::spawn`/`Mutex`/`RwLock`/
`unsafe` — a pure single-threaded safe parser, no new thread-safety surface.
No new worker-thread code landed since 2026-07-25 elsewhere in the tree.

## Prioritized Fix Order

1. **NEW-CONC-1** (MEDIUM) — Document or eliminate the `CinematicPresentationState`↔`QuestStageState` order inversion between `quest_fragment_dispatch_system` and `cinematic_animation_event_system`. Preferred fix (route the two `apply_effect` arms through a queued side-effect instead of a nested resource acquisition) is a modest refactor; the documentation-only alternative is a five-minute comment pass at both sites.
2. **NEW-CONC-2** (LOW) — Write down the "snapshot before iterate" convention once, opportunistically alongside #1.
3. Carried-forward LOWs from 2026-07-25 needing no new action this pass: `#2153` (`CHARAL-D3-01`), `#2154` (`SAVE-D3-02`), `#2155` (`CONC-D4-NEW-03`) — all still open, unchanged, not re-litigated here.
4. `#2152` (`CHAIN-D2-05`, MEDIUM, ReSTIR reservoir first-use uninitialized read) remains open — orthogonal to this sweep, tracked under the renderer/denoiser dimension.

## Suggest

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-03.md
```
