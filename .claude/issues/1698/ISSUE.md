# RT-1: Skyrim Dragonsreach bench-window FPS collapsed 321→8.7 — ECS scheduler stalls ~140 ms/frame for ~28 s

**Severity**: HIGH
**Dimension**: performance / ecs (scheduler) — surfaced via runtime telemetry
**Location**: `atw_scheduler` stage in `byroredux/src/main.rs:2272` (bench timing) + `crates/core/src/ecs/scheduler.rs` (parallel system scheduler); per-frame systems registered in `byroredux/src/main.rs` / `byroredux/src/systems.rs`. Evidence: `/tmp/audit/runtime/skyrim_se-WhiterunDragonsreach.engine.log`.
**Status**: NEW (CONFIRMED against live telemetry 2026-06-23)

## Description
On the heaviest baselined interior (WhiterunDragonsreach, 6049 entities, 294 newly-parsed meshes), the 240-frame bench window runs at a **steady** ~7 fps / dt≈147 ms for its entire ~28 s duration, then recovers instantly to 555–697 fps / dt≈1.5 ms the moment the window ends. The cost is **entirely CPU-side in the scheduler stage** — `wall_ms=114.3`, `systems_ms=113.5`, while `draw_ms=0.9` and every GPU pass reads ~0. The per-second `cpu_ms` breakdown pins it precisely: `atw_scheduler=138..147` ms during the window vs `atw_scheduler=1` ms once warm. This is a 37× regression against the contract metric (baseline 321.1 fps from AUDIT_RUNTIME_2026-06-14).

## Evidence
```
bench: frames=240 wall_fps=8.7 wall_ms=114.31 ... draw_ms=0.87 systems_ms=113.54 entities=6049 draws=2445/2b/4c
cpu_ms: ... atw_scheduler=138 atw_post=1   (during window)
cpu_ms: ... atw_scheduler=143 atw_post=1   (during window)
```
The same metric on all four other games shows `systems_ms` 0.14–1.18 ms with zero dt>100 ms frames — the pathology is unique to this cell.

## Impact
The first ~28 wall-seconds after entering Dragonsreach (or any cell of comparable scheduler load) render at ~7 fps — a multi-second hitch on cell entry. Reproducible across two runs (run 2: 8.5 fps / systems_ms=116.3 / 27 slow seconds). The recover-after-N-frames shape points to a transient backlog draining through the scheduler (candidates: first-frame query-cache population across the 294 fresh meshes, deferred BLAS/descriptor warm-up serialized onto the main scheduler, or a newly-added per-frame system doing one-time-amortized work). It is **not** the M47.2 scripting systems (`trigger_detection_system` / `recurring_update_tick_system` iterate only the sparse `TriggerVolume` / `RecurringUpdate` sets — and an O(entities) system would not self-recover after 28 s).

## Suggested Fix
Bisect bench-window `atw_scheduler` on Dragonsreach across the 06-14→06-23 range (`git log --since=2026-06-14 -- crates/core/src/ecs/ byroredux/src/systems.rs byroredux/src/main.rs`). Add a one-line per-system-cost dump for the first 60 frames (the scheduler already times each system post-#1647) to name the offending stage, then decide whether the backlog should be amortized across frames or moved off the per-frame scheduler. Pair with `/audit-performance` and `/audit-ecs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked across other heavy interiors (any cell with comparable fresh-mesh + scheduler load)
- [ ] **LOCK_ORDER**: If a RwLock scope changes in the scheduler, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test (or the runtime baseline gate) pins this specific fix

## ROOT CAUSE — confirmed via live profiling (2026-06-24, RTX 4070 Ti + Skyrim SE data)
Ran `BYRO_PROFILE=1 ... --game skyrim_se --cell WhiterunDragonsreach --bench-frames 240 --bench-hold`.
Per-system dump (`sched top systems`) names it unambiguously:
- `byroredux_physics::sync::physics_sync_system = 144ms/frame` (everything else <0.4ms).
Per-phase (`physics_sync phases`):
- load frame: collect/register=12.5ms (new=1574 bodies), step=0 (all spawn ASLEEP, awake dyn=0).
- window frames: `step=144–196ms (5 substeps) | awake dyn=~480 kin=1`.
~480 of the 1574 dynamic clutter bodies are AWAKE; the Rapier solver steps them at
MAX_SUBSTEPS=5 (dt≈115ms inflates the fixed-timestep accumulator to the 5-substep cap),
so cost = 5 × (~29ms solve over 480 bodies). Self-recovers when the bodies re-sleep (~28s).
Trigger: the character-controller body free-falls (`M28.5: body Y -232→-241, grounded=false`,
rapier_bodies=1575) — no floor collider under the player spawn — and the first step resolves
spawn interpenetration across ~480 bodies. The fly-camera (what renders) is decoupled from the
falling physics body, so the view looks fine while the sim thrashes.

NOT the animation cache / scripting / draw — those are all <0.4ms. The audit's candidate list
is superseded by this direct attribution.

### Fix direction (needs decision)
Levers: (a) stop the free-falling controller body from waking the cell (floor-collision coverage
for the player spawn), (b) keep spawn-interpenetrating clutter asleep (contact-skin / sleep
threshold), (c) anti-spiral: don't run 5 substeps once a frame is already slow. (a)+(b) attack the
root (awake-body count); (c) caps the 5× multiplier. Re-run the same bench to verify (fps gate).

## FIX PROGRESS (2026-06-24/25 session)
### Done — ragdoll half (commit 036a7788)
Live NPC ragdoll bones were imported MO_SYS_DYNAMIC and free-simulated.
Flipped to MotionType::Keyframed at NPC spawn (both spawn paths) → driven by
push_kinematic from the animated GlobalTransform. Verified: 162 bones →
kinematic, awake dynamics 480→325. Bethesda-correct; tests green.

### Remaining — clutter half (RT-1b, separate root cause)
Census (BYRO_PROFILE, Dragonsreach): total=1574 bodies → static=154,
dynamic=1258, kinematic=162. Of 1258 dynamic clutter, ~325 are awake/falling;
933 sleep fine. Static colliders EXIST (154) and their AABB
(x[-2480,1712] y[-246,2160] z[-4368,384]) covers the character at
(-1244,-232,-2895) — yet character + 325 clutter free-fall through.

Hypotheses tested:
- TUNNELING (fast ×70-gravity fall through thin trimesh) → tried
  `ccd_enabled(true)` on dynamic newcomers: step EXPLODED 144ms→1400-3900ms
  and awake stayed 325. DISPROVED — CCD has nothing to land on, so the bodies
  genuinely lack a collider beneath them (coverage gap), not tunneling. CCD
  reverted.
- Conclusion: a SUBSET of furniture/surfaces the 325 clutter rest on produce
  no static collider (933 others do). Likely FURN / specific STAT meshes whose
  bhk (or synth-trimesh fallback) collision isn't materializing, OR clutter
  spawn-snap above an uncovered surface. The character free-fall is a related
  but separate KCC-vs-floor grounding bug (it's KinematicPositionBased, not a
  dynamic body — CCD-on-dynamics wouldn't touch it).

Next-session starting points: (1) identify which STAT/FURN base meshes under
the 325 awake bodies lack colliders (log entity→base-form for awake fallers);
(2) check the synth-trimesh fallback gate (base_layer==Architecture excludes
FURN); (3) character KCC down-cast vs the existing floor collider.

### Done — anti-spiral amortization (2026-06-24 session)
Shipped lever (c) from the fix-direction list: a per-frame **wall-clock
substep budget** in `PhysicsWorld::step` (`SUBSTEP_TIME_BUDGET = PHYSICS_DT`,
the break-even point past which catch-up is futile). The fixed-timestep loop
already *capped* the backlog at `MAX_SUBSTEPS=5` but still ran all 5 substeps
× ~325 awake bodies (~29 ms each) every frame → a stable 144 ms/frame plateau.
The budget stops the catch-up loop once this frame's substeps have eaten one
sim-tick of wall-time and forfeits the rest of the backlog (slight
slow-motion), so the same settle work amortizes across more, individually-
cheap frames. At least one substep always runs (sim still advances). No-op in
the common case (sub-ms substeps never approach the budget) → steady-state
unchanged. 3 regression tests; full physics suite + workspace green.
Expected effect: Dragonsreach bench window 7 fps → playable for the same
~28 s storm. Does **not** restore the 321 fps baseline *during* the storm —
that needs the awake-body count itself driven down (leads 1–3 below).

Lead (2) DISPROVEN at code level this session: `RecordType::render_layer()`
maps FURN → `RenderLayer::Architecture` (`crates/plugin/src/record.rs:288`),
so the synth-trimesh gate at `spawn.rs:1078` already covers FURN. The
coverage gap is elsewhere (lead 1: a subset of surfaces whose bhk parse or
synth fallback silently produces no collider). Lead (3) is a separate
character-grounding bug — the KCC body is kinematic, so it adds no solver
cost; it is not part of the FPS collapse.

### Done — culprit-naming diagnostic (lead 1, later 2026-06-24 session)
`crates/physics/src/sync.rs::dump_awake_fallers`, gated by the opt-in env
flag `BYRO_PROFILE_FALLERS` (separate from `BYRO_PROFILE`), one-shot per
process, pure logging. On the first frame with ≥16 awake dynamic bodies it
dumps them sorted by most-negative vertical velocity — large `-vy` = free-
falling with no collider beneath (the coverage gap); `vy≈0` = spawn-
interpenetration jitter pile — each tagged with its entity's `RenderLayer`
and stable local form id (resolved through `FormIdPool`). The form ids are
the **entity→base-form** link lead 1 asks for: resolve them in xEdit to name
the exact STAT/FURN/clutter whose collision isn't materializing. Unit-tested
(`worst_fallers` sort/cap). **This is the instrument, not the cure** — it
turns the next Dragonsreach runtime run (`BYRO_PROFILE_FALLERS=1 … --game
skyrim_se --cell WhiterunDragonsreach`) into a root-cause-naming run; the
actual collider fix lands once those form ids are known. Issue stays OPEN.
