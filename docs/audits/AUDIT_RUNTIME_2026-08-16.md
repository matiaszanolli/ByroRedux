# Runtime Telemetry Audit — 2026-08-16

**Scope.** The five committed per-game runtime baselines under
`.claude/audit-baselines/runtime/`, driven headless under `xvfb-run` against a
real Vulkan device, plus — **explicitly in scope** — the **P2 gameplay slice**
(`byroredux/src/combat.rs`, `byroredux/src/inventory.rs`,
`byroredux/src/settings_io.rs`, and the action half of
`byroredux/src/interaction.rs`), which has no owner audit skill. Its runtime
contract is `docs/smoke-tests/p0-door-interaction.sh`,
`docs/smoke-tests/p1-character-traversal.sh` and
`docs/smoke-tests/p2-melee-core.sh`; all three were executed against this tree.

**Game arms.** `fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4` **RAN**.
`starfield` **SKIPPED** — the profile ships empty archives and no
`sample_cells`, so no cell baseline exists (per `audit-runtime/SKILL.md`); no
telemetry was fabricated for it. Game data and a Vulkan device were available
for all six installs.

**This audit is the final arm of the `comprehensive` sweep.** The sweep had
already established, with runtime evidence, that the P2 slice is non-functional
on every game for a different per-game reason. Those causes are **not re-filed
here**. This report measures instead what the three gates *actually exercise*
versus what they claim — and the answer changes the sweep's conclusion: two of
the three gates are currently **RED**, and the one that is green is green while
its own fixture is unplayable.

---

## Harness note (affects which numbers are trustworthy)

The first sweep reproduced the RT-1 / #1619 mis-attribution: `xvfb-run`'s PID is
a wrapper, so tearing down `$!` left every engine alive holding port 9876 and
four of five telemetry captures were silently attributed to the FNV engine. The
sweep was discarded and re-run with a distinct `BYRO_DEBUG_PORT` per game
(9881–9885), which produced correct per-game telemetry.

Stale engines survived the second teardown too, so **`wall_fps` and
`frame_p*_ms` are contaminated for every game except `fnv`** (which ran first and
alone). Those metrics are advisory and never gating (RT-2 / #1701), so no
finding depends on them, but they are reported as **NOT MEASURED** rather than
guessed.

The **structural** metrics are unaffected: `entities_total`,
`tex_missing_unique_paths`, `mesh_cache_failed_count`, the skin pool triple and
the `draws=N/Mb/Kc` split came out **byte-identical between the two independent
sweeps** for all four overlapping games. That cross-run identity is the evidence
that they are contention-independent, and every gating finding below rests only
on them.

---

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| `fnv` | `FreesideAtomicWrangler` | **REGRESSION (MEDIUM)** | batches 89→164 (+84.3 %), gpu_calls 25→35 (+40.0 %); entities 9271→9403 (+1.42 %, in band); tex 1→1, mesh 11→11, skin 677/1364+0 unchanged |
| `fo3` | `MegatonPlayerHouse` | **REGRESSION (MEDIUM)** | batches 96→114 (+18.8 %), gpu_calls 9→12 (+33.3 %); entities 3311→3467 (+4.71 %, out of band); cmds 1839→1576 (−14.3 %); tex 0→0, mesh 3→3 |
| `oblivion` | `ICMarketDistrictTheGildedCarafe` | **PASS** | entities 701→704 (+0.43 %); cmds 324→324; batches 47→20, gpu_calls 4→2 (both improved); tex 0→0, mesh 0→0 |
| `skyrim_se` | `WhiterunDragonsreach` | **PASS** | entities 8126→8279 (+1.88 %, in band); cmds 2342→2399 (+2.43 %); batches 9→9, gpu_calls 2→2; tex 0→0, mesh 9→9 |
| `fo4` | `InstituteBioScience` | **REGRESSION (MEDIUM)** | entities 12448→14688 (+18.0 %) **with** cmds 3440→3954 (+14.9 %); skin_live 124→248; batches 753→222 and gpu_calls 42→13 (both improved); tex 1→1, mesh 0→0 |
| `starfield` | — | **SKIPPED** | no cell baseline exists |

`light_count_directional` was `1` on every arm (the single `CellLightingRes`
sun), matching every baseline. `skin_pool_max` was `1364` and
`skin_pool_overflow_attempts` was `0` on every arm — the #1284 cap is under no
pressure anywhere.

### Advisory (non-gating) frame metrics

| Game | baseline `bench_fps_p50` | measured `wall_fps` | note |
|---|---|---|---|
| `fnv` | 166.1 | 136.4 | clean single-tenant measurement; `frame_p50_ms=6.52`, `p95=8.36` |
| `fo3` | 93.3 | NOT MEASURED | harness contention |
| `oblivion` | 613.2 | NOT MEASURED | harness contention |
| `skyrim_se` | 161.9 | NOT MEASURED | harness contention |
| `fo4` | 68.3 | NOT MEASURED | harness contention |

---

## Playable-slice gate results (real runs against this tree)

| Gate | Result | Cause |
|---|---|---|
| `docs/smoke-tests/p0-door-interaction.sh` | **FAIL** (exit 1, reproduced twice) | assertion-string drift from `eb5d76fe`, cascading into 3 further phantom failures |
| `docs/smoke-tests/p1-character-traversal.sh` | **FAIL** (exit 1) | same assertion-string drift |
| `docs/smoke-tests/p2-melee-core.sh` | **PASS** (exit 0) | written after the drift; passes on a fixture whose player cannot stand or walk |

---

## Findings

### RT-2026-08-16-01: `p2-melee-core.sh` passes while its own fixture's player has no floor — the gate asserts `physics_synced`, never `grounded`
- **Severity**: HIGH
- **Dimension**: Playable-slice gate semantics (P2 gameplay slice)
- **Location**: `docs/smoke-tests/p2-melee-core.sh:144-160`, `byroredux/src/commands/view.rs:186-245`, `byroredux/src/systems/character.rs:707-726`
- **Status**: NEW
- **Description**: The P2 gate is the project's declared closure evidence for its
  active execution focus, and it is green. Driving the same fixture by hand shows
  the character in `BleakFallsBarrow01` **cannot stand and cannot walk**, and that
  `combat.approach` — the gate's setup command — leaves it falling out of the
  world. The gate cannot observe either condition because it asserts only
  `physics_synced=true` (the return value of the kinematic-translation write) and
  never `grounded=true`, and because it re-issues `combat.approach` immediately
  before **every one of the seven swings**, resetting the position each time.
  `p1-character-traversal.sh:238` does assert `grounded=true`; `p2` dropped that
  assertion for the cell where it actually fails.
- **Evidence**: live probe, `--player --radius 1` (the gate's own flags), `BYRO_DEBUG_PORT=9891`.

  At the authored spawn, the player is permanently un-grounded, immobile, and
  pinned at terminal fall velocity while never translating:
  ```
  byro> player.status
  "mode=Character player=17052
   body=(-7485.59, 1130.47, -980.67) grid=(-2,0) grounded=false vertical_velocity=-2000.00"
  byro> input.hold forward 90
  "input.hold: queued Move forward through the W binding for 90 frames"
  byro> player.status            # after the hold completed
  "body=(-7485.55, 1130.48, -980.70) grounded=false vertical_velocity=-2000.00"
  byro> player.status            # 15 s later — unchanged
  "body=(-7485.55, 1130.48, -980.70) grounded=false vertical_velocity=-2000.00"
  ```
  90 frames of held forward input move the body **0.05 BU**. An unmodified attack
  from that spawn misses: `outcome=melee swing missed`.

  `combat.approach` then reports success while its floor probe misses, and the
  body free-falls:
  ```
  byro> combat.approach 12587
  "Character placed in melee range of entity 12587 at (9015.58, -2053.70, 4724.62)
     body=(9015.58, -1985.70, 4604.62) distance=120.0 physics_synced=true"
  engine: Transition arrival: grounding capsule at (9015.6, -1985.7, 4604.6)
          — floor probe MISSED, using destination height (destination y=-2053.7)
  +0 s   grounded=false vertical_velocity=-4.55     y=-1985.76
  +2 s   grounded=false vertical_velocity=-2000.00  y=-4565.85
  +12 s  grounded=false vertical_velocity=-2000.00  y=-24781.14
  ```
  The gate's own structure corroborates this: it must re-approach before each
  swing precisely because the character does not stay put.
- **Impact**: The P2 combat core is certified against a fixture that fails
  `docs/engine/playable-vertical-slice.md`'s gate 2 ("Character movement,
  collision, camera … survive … without a soft lock or falling out of the world")
  and gate 1 ("`byro-dbg` is not required to move, interact, fight"). What the
  gate proves is the damage arithmetic — ray → `HitEvent` → `ActorValues` → `Dead`
  → ragdoll — not a playable encounter. Any regression in spawn grounding,
  character traversal or collision coverage in this cell is invisible to it.
- **Related**: RT-2026-08-16-02 (the other two gates are RED), RT-2026-08-16-03
  (the probe/KCC disagreement that causes it), `docs/engine/p2-combat-fixture.md`.
- **Suggested Fix**: Add `wait_for_pattern "player.status" "grounded=true"` after
  the first `combat.approach` and before the swing loop, and assert it once more
  after the last swing; make `combat.approach` return failure (not
  `physics_synced=true`) when `ground_character_body_at` reports a probe miss.

---

### RT-2026-08-16-02: `p0-door-interaction.sh` and `p1-character-traversal.sh` are deterministically RED — one reworded log line broke both, three commits ago
- **Severity**: HIGH
- **Dimension**: Playable-slice gate integrity
- **Location**: `docs/smoke-tests/p0-door-interaction.sh:115`, `docs/smoke-tests/p1-character-traversal.sh:251`, `byroredux/src/interaction.rs:492`
- **Status**: NEW
- **Description**: `eb5d76fe` (HEAD~2) replaced the per-action `input.press`
  response with a generic template. `byroredux/src/interaction.rs:492` now emits
  `"input.press: queued {action} through the {label} binding"` →
  *"input.press: queued Activate through the E binding"*. `p0` still greps the
  pre-`eb5d76fe` literal `"input.press: queued KeyE through the normal Activate
  binding"` and `p1` greps `"normal Activate binding"`; neither substring exists
  any more. `p2-melee-core.sh:148` greps `"queued Attack through the R binding"`
  — the **new** wording — which is the only reason it is green. The underlying
  functionality is fine in both cases: p0's engine log shows the full chain
  completing (`interaction: entity 904 activated; queued exterior 'whiterunworld'
  (6,-2)` → `Cell transition applied`). These are assertion failures, not
  behaviour failures — but the gates are red, and the docs say they are closed.
- **Evidence**: two independent p0 runs, identical output:
  ```
  smoke[p0-door-interaction]: FAIL -- smoke input entered through the normal KeyE binding
      (missing 'input.press: queued KeyE through the normal Activate binding')
  smoke[p0-door-interaction]: FAIL -- exactly one Activate edge was consumed (missing 'activations=1')
  smoke[p0-door-interaction]: FAIL -- canonical ActivateEvent was emitted (missing 'event_emitted=true')
  smoke[p0-door-interaction]: FAIL -- post-transition trace retained the successful outcome
  P0_EXIT=1
  ```
  ```
  smoke[p1-character-traversal]: PASS -- door prompt recovered after walking
  smoke[p1-character-traversal]: FAIL -- interior door activation bypassed the action binding
  P1_EXIT=1
  ```
  The press command itself succeeded in the same run:
  `byro> "input.press: queued Activate through the E binding"`.
  p0's three trailing failures are a **cascade**, not independent: the first
  `require_in` sets `hard_fail=1`, which gates the post-transition arrival probe
  behind `if (( hard_fail == 0 ))` (`p0-door-interaction.sh:137`), so
  `arrival.debug.log` is never written and its three assertions fail against a
  missing file. One stale string produces four red lines.
- **Impact**: `docs/engine/playable-vertical-slice.md` records P0 as "Closed
  2026-08-10" and P1's gate as "now passes"; `audit-runtime/SKILL.md` cites all
  three as "the runtime contract for the gameplay slice". Two of the three have
  been red since `eb5d76fe` and nothing noticed, because no CI job runs them
  (see RT-2026-08-16-04). A gate that is red-but-unrun is worse than no gate: it
  is cited as evidence in three documents.
- **Related**: RT-2026-08-16-04 (no CI wiring), RT-2026-08-16-01.
- **Suggested Fix**: Update both greps to the current template (match
  `"queued Activate through the"`, not the full sentence), and re-order p0 so the
  arrival probe runs regardless of earlier `hard_fail` state so one stale
  assertion cannot manufacture three more.

---

### RT-2026-08-16-03: the walkable-spawn gate certifies the camera column while the character is spawned at the door column, and its "floor" disagrees with the controller
- **Severity**: HIGH
- **Dimension**: Runtime spawn placement (P2 fixture)
- **Location**: `byroredux/src/scene.rs:883-903`, `byroredux/src/scene.rs:1122-1165`
- **Status**: NEW
- **Description**: `scene.rs:895` runs `probe_spawn_ground` at `cam_pos` and uses
  the result solely to decide Character vs FlyCam (`ground_walkable`,
  `scene.rs:903`) — the EX-04 "Character mode may only start from a *verified*
  walkable surface" gate. The character is then placed by an entirely different
  path, the interior door-teleporter spawn at `scene.rs:1122-1165`, at a
  different `(x,z)`. The gate therefore verifies a surface the player never
  stands on. In the P2 fixture the door path's answer comes from its **last-resort
  rung** — the full-cell sweep, which starts at the cell AABB ceiling and returns
  the first walkable-normal hit — and in a multi-level barrow that is an upper
  surface 550 BU *above* the door it just came through. The controller then
  disagrees with the probe at that very point: the capsule sits exactly
  `character_spawn_center_y` above the reported surface, yet reports
  `grounded=false` forever and is blocked from translating.
- **Evidence**: boot log of the P2 fixture (`--player --radius 1`):
  ```
  spawn-probe: result=grounded colliders=2468 surface_y=901.2 spawn_y=969.2
  Player rig: Character (M28.5 kinematic capsule + gravity)
  M28.5 spawn at door teleporter: door at (-7537.2, 512.0, -1018.5);
    inward nudge (51.7, _, 37.8) BU; floor probe hit y=1062.4 via
    full-cell sweep at nudged XZ; placing capsule at (-7485.6, 1130.4, -980.7)
  ```
  The gate's verified `spawn_y=969.2` is discarded; the applied height is
  `1130.4`, 161 BU higher, derived from a different column. The "hit" surface
  `y=1062.4` is 550 BU above the door at `y=512.0`. Live state at that position
  is `grounded=false vertical_velocity=-2000.00` with the body frozen (see
  RT-2026-08-16-01's mobility measurement) — contact exists (gravity cannot move
  it) but the controller's support test rejects the surface the spawn probe
  accepted. The comment at `scene.rs:1110-1121` records that this rung was
  deliberately widened to span `max.y - min.y` after #2013 because the narrow
  sweep *missed*; widening it from the ceiling substituted the opposite failure.
- **Impact**: The interior spawn point in the P2 fixture cell is unplayable, and
  the EX-04 walkable-ground gate cannot catch it by construction, because it
  never evaluates the position that is actually used. Any interior whose door
  column has geometry above it is exposed to the same substitution.
- **Related**: RT-2026-08-16-01 (the gate that cannot see it); #2876 (the
  collider census that would diagnose it is boot-time-only). Distinct from
  AUDIT_PHYSICS_2026-08-16's `ground_character_body_at` finding, which covers the
  lock-order question only.
- **Suggested Fix**: Run the EX-04 probe at the *resolved spawn column* rather
  than `cam_pos`, after the door-teleporter path has chosen `(spawn_x, spawn_z)`;
  and make the full-cell rung sweep downward from near door height rather than
  from the cell AABB ceiling, so it cannot return a surface above the threshold.

---

### RT-2026-08-16-04: no CI job runs any smoke gate, and all three exit 0 when Skyrim data is absent
- **Severity**: MEDIUM
- **Dimension**: Verification infrastructure
- **Location**: `.github/workflows/ci.yml`, `docs/smoke-tests/p0-door-interaction.sh:41-43`, `docs/smoke-tests/p1-character-traversal.sh:46-48`, `docs/smoke-tests/p2-melee-core.sh:45-48`
- **Status**: NEW
- **Description**: `ci.yml` defines six jobs (shader parity, test/check/clippy,
  ABBA lock-order, dhat-heap, lavapipe Vulkan validation, bench determinism).
  None invokes anything under `docs/smoke-tests/`. Grepping the whole `.github/`
  tree for `smoke-tests`, `p0-door`, `p1-character` or `p2-melee` returns
  nothing. Independently, every gate's data preflight is
  `echo "…: SKIP -- missing $required"; exit 0` — a **zero** exit. Combined,
  the slice's entire runtime contract is manual, and any harness that did adopt
  it on a runner without game data would report green while executing nothing.
- **Evidence**: `grep -rn "smoke-tests\|p2-melee\|p1-character\|p0-door" .github/`
  → no matches. `ci.yml` job names: *Shader source/artifact parity*, *Test +
  Check + Clippy*, *ABBA lock-order detector*, *NIF heap-allocation regression
  (dhat-heap)*, *Vulkan validation layers (lavapipe)*, and the bench-determinism
  step at `:209-213`. RT-2026-08-16-02 is the direct consequence: p0 and p1 have
  been red for three commits with nobody informed.
- **Impact**: "P2 combat core passing 2026-08-16" is a one-time manual
  observation, not a standing guarantee, and the same is true of P0's and P1's
  recorded closures. Nothing prevents the next such regression from also going
  unnoticed until an audit runs the scripts by hand.
- **Related**: RT-2026-08-16-02.
- **Suggested Fix**: Add a self-hosted (game-data-bearing) job that runs the three
  gates, and change the missing-data preflight to a distinct non-zero "skipped"
  exit code the caller can tell apart from "passed" — or at minimum have the
  runner assert the data is present before invoking them.

---

### RT-2026-08-16-05: fixing the FO3/FNV `AVIF` prefix bug will not make actors damageable — the auto-calc derivation has no Health term at all
- **Severity**: MEDIUM
- **Dimension**: P2 gameplay slice — cross-game reachability
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:180-208`, `byroredux/src/npc_spawn.rs:102-114`
- **Status**: NEW
- **Description**: The sweep's established FO3/FNV root cause is that
  `EsmIndex::actor_value_form_id` misses `AV`-prefixed AVIF EditorIDs. That is
  necessary but **not sufficient**, and the proposed remedy will appear to
  change nothing. `derive_autocalc_actor_values` emits exactly the 7 members of
  `AttributeSet::FALLOUT` and the 15 members of `SkillSet::FALLOUT_FO3_FNV` —
  neither roster contains Health. `stamp_actor_values` inserts `ActorVitals`
  only when the resolved Health key is **already present in `pairs`**:
  ```rust
  let health = index
      .health_actor_value_key(game)
      .filter(|health| pairs.iter().any(|(form_id, _)| form_id == health));
  ```
  With the prefix fixed, `health_actor_value_key` would resolve, the `filter`
  would still find no matching pair, and `ActorVitals` would still never be
  stamped. `byroredux/src/npc_spawn.rs:112` is the only production `ActorVitals`
  writer in the workspace.
- **Evidence**: rosters read directly — `crates/core/src/character/attribute.rs:85-95`
  (Strength…Luck) and `crates/core/src/character/skill.rs:158-176` (Barter…BigGuns).
  Neither lists Health. `combat.rs:200` bails on a missing `ActorVitals`, and
  `commands/view.rs:152-159` rejects such an entity with *"is not a damageable
  actor"* — the rejection the sweep already observed on every FNV NPC.
  Contrast the two arms that do work: `derive_skyrim_actor_values`
  (`actor_value_derive.rs:138-154`) explicitly pushes
  `(health_key, starting_health + health_offset)`, and `derive_stored_actor_values`
  (`:162-176`) explicitly pushes the baked `DNAM` Health.
- **Impact**: An FO3/FNV arm added to `p2-melee-core.sh` after the prefix fix
  lands would still fail, and the fix would be recorded as ineffective rather
  than incomplete. FO3/FNV needs its own Health derivation (the GECK model is
  Endurance- and level-driven) before the melee slice can reach the reference
  title at all.
- **Related**: the sweep's FO3/FNV `AVIF` finding (not re-filed);
  FNV-2026-08-16-D8-01 (the gates are Skyrim-only).
- **Suggested Fix**: Add a Health term to the FO3/FNV arm — derive it from the
  class/ACBS data the way `derive_skyrim_actor_values` derives TES5 Health — and
  pin it with a test that asserts `stamp_actor_values` yields an `ActorVitals`
  for a vanilla FNV NPC, not a synthetic fixture.

---

### RT-2026-08-16-06: draw-batch merge regression on `fnv` and `fo3` — batches and GPU calls past the ×1.1 contract
- **Severity**: MEDIUM
- **Dimension**: Baseline diff — render load
- **Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`, `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv`, `byroredux/src/render/mod.rs`
- **Status**: NEW
- **Description**: Two arms moved the `draws=N/Mb/Kc` split against its gate
  (`≤ baseline ×1.1`) on the merge and GPU-call halves while the DrawCommand
  input count `N` stayed flat or fell. That is a batching-policy change, not more
  geometry. The same window moved `fo4` and `oblivion` sharply the *other* way,
  so one mechanism is producing opposite outcomes per content shape.
- **Evidence**:

  | Game | `cmds` | `batches` | `gpu_calls` |
  |---|---|---|---|
  | `fnv` | 2553 → 2562 (+0.35 %) | 89 → **164** (+84.3 %) | 25 → **35** (+40.0 %) |
  | `fo3` | 1839 → 1576 (−14.3 %) | 96 → **114** (+18.8 %) | 9 → **12** (+33.3 %) |
  | `fo4` | 3440 → 3954 | 753 → 222 (−70.5 %) | 42 → 13 (−69.0 %) |
  | `oblivion` | 324 → 324 | 47 → 20 (−57.4 %) | 4 → 2 (−50.0 %) |

  All eight numbers reproduced identically across the two independent sweeps.
  `fnv`'s baseline header already attributes a prior 10→25 GPU-call move to
  `draw_sort_key`'s depth-primary ordering for alpha-over; this is a further
  move on top of that, and `fo3` — which was not part of that regen — moved too.
- **Impact**: 40 % more GPU calls on the bench-of-record cell for the reference
  title. Modest in absolute terms (25→35) but it is the metric the skill
  designates as the exact render-load contract, and it is now drifting in
  opposite directions per game without an accompanying baseline regen note.
- **Related**: #2215 (the prior `draw_sort_key` regen), #1258 (the three-way split).
- **Suggested Fix**: Bisect the merge predicate in `byroredux/src/render/mod.rs`
  over the window since the 2026-08-06 regen; if the divergence is the deliberate
  cost of a compositing fix as in #2215, regenerate all five baselines together
  with one shared header note rather than leaving four arms un-regenerated.

---

### RT-2026-08-16-07: `fo4 InstituteBioScience` grew +18.0 % entities **and** +14.9 % DrawCommands — the entity rise is rendering, not bookkeeping
- **Severity**: MEDIUM
- **Dimension**: Baseline diff — entity/render load
- **Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`
- **Status**: NEW
- **Description**: `entities_total` moved 12448 → 14688 (+18.0 %), far outside the
  ±2 % tolerance band. The documented benign pattern for that metric (#1705 /
  RT-3) is a rise in *non-rendering* bodies, corroborated by `bench_draws_cmds`
  holding or falling — that is exactly how the current baseline's own header
  justifies its previous 11279 → 12448 regen. This time the corroboration goes
  the other way: `bench_draws_cmds` rose 3440 → 3954 (+14.9 %), also past its
  ×1.1 gate, and `skin_pool_live` doubled 124 → 248. Both halves of the split
  moved together, so this is more geometry being drawn, not more markers.
- **Evidence**: `entities=14688 … draws=3954/222b/13c`, `skin=248/1364+0`, against
  baseline `entities_total 12448`, `bench_draws_cmds 3440`, `skin_pool_live 124`.
  `tex_missing_unique_paths` stayed 1 and `mesh_cache_failed_count` stayed 0, so
  nothing failed to load — the extra draws are resolved content.
- **Impact**: Either FO4 is now spawning content it previously dropped (an
  improvement that should be recorded) or the precombine de-dup gate is emitting
  originals alongside precombines (a regression that would show as exactly this
  signature). The baseline cannot distinguish them, and `skin_pool_live`
  doubling on a cell with no overflow suggests real additional skinned actors.
- **Related**: #2698 / #2699 (XPRI precombine de-dup gate), #2216 (the previous
  FO4 entity regen).
- **Suggested Fix**: Diff the FO4 cell's spawn census against the 2026-08-06
  capture to attribute the +2240 entities; if precombine de-dup is implicated,
  fix before regenerating, otherwise `--regen` with a header note naming the
  cause the way the existing FO4 header does.

---

### RT-2026-08-16-08: the debug server announces "listening on 127.0.0.1:\<port\>" regardless of whether the bind succeeded, and `bench-hold:` then advertises the port unconditionally
- **Severity**: MEDIUM
- **Dimension**: Telemetry integrity
- **Location**: `crates/debug-server/src/lib.rs:27-39`, `crates/debug-server/src/listener.rs`
- **Status**: NEW — the code-level residual of #1619, which was closed by editing skill text
- **Description**: `start()` calls `listener::spawn(port)` and then
  unconditionally logs `Debug server listening on 127.0.0.1:{port}` at INFO. The
  bind actually happens on the listener thread, which reports failure separately
  at ERROR. On a port collision the operator therefore sees a confident
  "listening" line *first*, an ERROR second, and then the `bench-hold:` notice
  telling them to "attach via `cargo run -p byro-dbg` (port 9876)" — a port this
  process does not own. Any `byro-dbg` capture then silently reaches whichever
  engine *did* win the port, and attributes its telemetry to the wrong run.
  #1619 closed this by documenting a serial-run contract in
  `audit-runtime/SKILL.md`; the observable that makes the failure silent is
  unchanged.
- **Evidence**: reproduced in this audit's first sweep, verbatim from
  `fo3-MegatonPlayerHouse.engine.log`:
  ```
  12: INFO  byroredux_debug_server] Debug server listening on 127.0.0.1:9876
  13: ERROR byroredux_debug_server::listener] Debug server failed to bind port 9876:
            Address already in use (os error 98)
  ...
  1183: bench-hold: engine held open in live interactive mode — attach via
        `cargo run -p byro-dbg` (port 9876). Ctrl+C / window close to exit.
  ```
  Four of five games in that sweep produced telemetry identical to the FNV
  engine's; the mis-attribution was detectable only by noticing that `fo3`'s
  `bench:` line said `entities=3467` while its captured `stats` said
  `Entities: 9403`.
- **Impact**: A wrong-engine capture is indistinguishable from a correct one in
  the captured artefacts. This audit lost a full sweep to it, and any future
  runtime audit, smoke script or manual session that hits a stale engine will
  silently diff one game against another's baseline. Serial execution is not
  sufficient protection, because the engine's own teardown does not guarantee the
  port is released (`xvfb-run` wrappers, detached processes).
- **Related**: #1619 (closed, skill-text fix only); `audit-runtime/SKILL.md`'s
  serial-run contract.
- **Suggested Fix**: Have `listener::spawn` return the bind result, log
  "listening" only on success, and make `bench-hold:` print the port only when
  the server is actually bound (or print "debug server unavailable: port N in
  use"). Failing fast on a collision would be better still for headless harnesses.

---

### RT-2026-08-16-09: `p2-melee-core.sh` asserts none of the fixture identity its own spec says it asserts, and pins the unarmed fallback as a literal
- **Severity**: MEDIUM
- **Dimension**: Playable-slice gate semantics
- **Location**: `docs/smoke-tests/p2-melee-core.sh:130`, `:156`, `docs/engine/p2-combat-fixture.md`
- **Status**: NEW
- **Description**: `docs/engine/p2-combat-fixture.md`'s closure gate states "The
  smoke also asserts the frozen reference/base FormIDs and weapon family at
  preflight." The script asserts the placed reference `0x0380B4` and nothing
  else: not the base NPC `000E9895`, and neither concrete weapon leaf
  (`0001CB64` Draugr Battleaxe 18, `000236A5` Draugr Greatsword 17). What it
  does assert about damage is `grep -Fq "damage=8.0"` on all seven swings — the
  `UNARMED_DAMAGE` constant, which `combat.rs:269-273` returns *only when the
  aggressor has no `EquippedWeapon`*. The gate therefore encodes the absence of
  the weapon family as a pass condition, and the seven passing assertions are
  positive runtime proof that the player carries no weapon in this fixture.
- **Evidence**: the gate's own passing run asserts `damage=8.0` seven times and
  `health_after` `42.0 → -6.0` in 8.0 steps. `attack_damage` is
  `world.get::<EquippedWeapon>(aggressor).map_or(UNARMED_DAMAGE, …)`, so
  `damage=8.0` for every swing means no `EquippedWeapon` on the player. Grepping
  the script for the fixture doc's other frozen IDs returns nothing:
  `000E9895`, `0001CB64` and `000236A5` appear in `docs/engine/p2-combat-fixture.md`
  and in no smoke script.
- **Impact**: Two ways to fail. First, the fixture doc overstates what the gate
  checks, so a content or FormID drift on the base NPC or the weapon leaves would
  pass. Second, the literal `damage=8.0` makes the gate a lock on the current
  broken state: any fix that gives the player an authored weapon — which is
  exactly what the sweep's player-loadout finding calls for — turns this gate
  RED, and the natural reading of that red will be "the loadout fix broke
  combat" rather than "the gate asserted the fallback".
- **Related**: RT-2026-08-16-01; the sweep's `PLAYER_BASE_FORM_ID` finding (not
  re-filed); ECS-2026-08-16-04.
- **Suggested Fix**: Assert the base NPC and both weapon leaves at preflight as
  the fixture doc claims, and replace the literal `damage=8.0` with a check that
  damage matches the player's *resolved* `EquippedWeapon` damage or the documented
  unarmed rule — so the gate tracks the contract rather than the current value.

---

### RT-2026-08-16-10: two of the four P2-slice modules have no runtime gate and no console surface at all
- **Severity**: LOW
- **Dimension**: P2 gameplay slice coverage
- **Location**: `byroredux/src/inventory.rs`, `byroredux/src/settings_io.rs`
- **Status**: NEW
- **Description**: `_audit-common.md` scopes the un-owned gameplay slice as
  `combat.rs` + `inventory.rs` + `settings_io.rs` + the action half of
  `interaction.rs`, and names the three P0/P1/P2 scripts as its gates. Measured
  against the scripts, the gates touch only `combat.rs` and the action half of
  `interaction.rs`. `inventory.rs` (546 LOC) and `settings_io.rs` (334 LOC) —
  880 LOC, a third of the slice — are exercised by no gate, and cannot be:
  the engine exposes no `byro-dbg` command for inventory or settings state.
- **Evidence**: the complete command surface (`help` against a live engine) lists
  `interaction.status`, `combat.status`, `combat.approach`, `input.press`,
  `input.hold`, `input.look`, `player.status` and no inventory or settings
  command. `p2-melee-core.sh:121` issues `entities Inventory`, but only to locate
  the Draugr by editor ID — it never inspects a single item, stack or equip slot.
  `settings_io.rs` has two `#[test]` functions and no runtime coverage.
  Independently confirmed live: the player entity appears in `entities Inventory`,
  but no command can show what is in it.
- **Impact**: The slice's inventory/equipment half — which
  `docs/engine/playable-vertical-slice.md` gate 5 requires to survive
  save → exit → reload, and which corpse loot will build on — has unit tests over
  hand-built fixtures and nothing that observes real authored data. Given the
  sweep's finding that the player template is seeded from a FormID matching no
  NPC_ record, the one thing a runtime gate would have caught is precisely what
  went unnoticed.
- **Related**: RT-2026-08-16-09; `_audit-common.md`'s un-owned-subsystem table.
- **Suggested Fix**: Add an `inventory.status` console command (player stacks,
  equipped slot mask, resolved `EquippedWeapon`) and assert a non-empty authored
  player loadout in a gate — the cheapest possible regression guard for the
  player-seeding bug.

---

## Clean dimensions (no findings)

- **`tex_missing_unique_paths`** — matched baseline exactly on all five arms
  (`fnv` 1, `fo3` 0, `oblivion` 0, `skyrim_se` 0, `fo4` 1). No texture-resolution
  or NIFAL material-boundary regression.
- **`mesh_cache_failed_count`** — matched baseline exactly on all five arms
  (`fnv` 11, `fo3` 3, `oblivion` 0, `skyrim_se` 9, `fo4` 0).
- **Skin slot pool** — `skin_pool_max` 1364 and `skin_pool_overflow_attempts` 0
  on every arm; `skin_pool_live` matched baseline exactly on `fnv` (677),
  `oblivion` (3) and `skyrim_se` (83). The #1284 cap is under no pressure. The
  `fo3` 0→7 and `fo4` 124→248 rises are reported inside RT-06/RT-07 rather than
  separately, since neither approaches the cap.
- **`light_count_directional`** — 1 on every arm, matching every baseline.
- **`oblivion ICMarketDistrictTheGildedCarafe`** — clean on every structural
  metric, with batching improved. The known-good cell stayed known-good.
- **`skyrim_se WhiterunDragonsreach`** — clean on every structural metric.
- **`p2-melee-core.sh` mechanical chain** — the ray → bone → `ActorColliderOwner`
  → actor root → `HitEvent` → `ActorValues::apply_damage` → `Dead` → 18-body
  ragdoll path executed correctly seven times. The finding against this gate is
  about what it omits, not about the chain it does exercise.

## Disproved candidates (investigated, withdrawn)

- **"Oblivion's bloom pass costs 5.586 ms — 175× every other game."** Measured in
  the first sweep and structurally attractive because `gpu_bloom` is a GPU
  timestamp, not wall-clock, so the RT-2 headless-jitter caveat would not have
  applied. It is an artefact of the four concurrently-alive stale engines in that
  sweep. The clean second sweep reads `gpu_bloom=0.027` on Oblivion against
  `fnv` 0.031, `fo3` 0.030 and `skyrim_se` 0.028. **Withdrawn.**
- **"`EquippedWeapon` has no runtime writer at all."** The premise carried into
  this audit does not hold: `byroredux/src/npc_spawn.rs:783` writes it on the NPC
  spawn tail and `byroredux/src/inventory.rs:249` writes it in `attach_to_player`.
  The real defect is narrower (the player's *template* resolves empty) and is
  already filed by sibling audits, so nothing was filed here. RT-09 states only
  the part this audit measured directly.
- **"The large `wall_fps` drops are a real regression."** `oblivion` 613.2→75.4
  and `skyrim_se` 161.9→60.0 look dramatic, but the harness had stale engines
  competing for CPU and `bench_fps_*` is explicitly non-gating (RT-2 / #1701).
  Reported as NOT MEASURED rather than as a finding.
- **"`p0`'s three post-transition failures are independent defects."** They are a
  single cascade behind `if (( hard_fail == 0 ))`; folded into RT-02 as one
  finding rather than four.

---

## Reproduction

```bash
# structural sweep — one distinct debug port per game, serial
for pair in "fnv FreesideAtomicWrangler 9881" "fo3 MegatonPlayerHouse 9882" \
            "oblivion ICMarketDistrictTheGildedCarafe 9883" \
            "skyrim_se WhiterunDragonsreach 9884" "fo4 InstituteBioScience 9885"; do
  set -- $pair
  BYRO_DEBUG_PORT=$3 xvfb-run -a --server-args="-screen 0 1280x720x24" \
    ./target/release/byroredux --game "$1" --cell "$2" --bench-frames 240 --bench-hold &
  # ... wait for `bench-hold:`, then capture, then kill the REAL engine pid,
  # not xvfb-run's wrapper pid.
done

# the three gates
./docs/smoke-tests/p0-door-interaction.sh     # currently exit 1
./docs/smoke-tests/p1-character-traversal.sh  # currently exit 1
./docs/smoke-tests/p2-melee-core.sh           # currently exit 0

# P2 fixture playability probe
BYRO_DEBUG_PORT=9891 RUST_LOG="error,byroredux::systems::character=info,byroredux::scene=info" \
  ./target/release/byroredux --esm "$SKY/Skyrim.esm" --cell BleakFallsBarrow01 \
  --bsa "$SKY/Skyrim - Meshes0.bsa" --textures-bsa "$SKY/Skyrim - Textures0.bsa" \
  --scripts-bsa "$SKY/Skyrim - Misc.bsa" --player --radius 1 --bench-frames 5 --bench-hold
# then: player.status ; input.hold forward 90 ; player.status ; combat.approach <draugr>
```

Baselines under `.claude/audit-baselines/runtime/` were **not** modified — no
`--regen` was performed, because RT-06 and RT-07 need attribution before the
contract is moved.

---

Report ready. Publish with:

```
/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-16.md
```
