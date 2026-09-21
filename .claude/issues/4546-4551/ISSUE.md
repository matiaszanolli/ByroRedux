# 4546: walk_anim / cinematic lock-order cycle panics under BYRO_LOCK_ORDER_CHECK=1 — five walk_anim tests observe AnimationPlayer → ActorCinematicState across an existing ACState → Transform → AnimationPlayer cycle

State: OPEN  Labels: ['bug', 'animation', 'medium']

**Discovered**: 2026-09-20 (playable-slice P3 session; pre-existing, never filed — surfaced while filing the Wave-2 pre-wave of `docs/engine/near-term-action-plan.md`)

**Reproduce**:
```bash
BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux walk_anim
```
The default suite is green; the opt-in global lock-order graph (#313) is what catches this.

**Panic** (`crates/core/src/ecs/lock_tracker.rs:476`), five of six walk_anim tests:
`moving_actor_swaps_to_walk_clip_and_back`, `scratch_reuses_allocation_across_frames`, `seated_actor_is_never_taken_over_and_abandons_mid_walk`, `skyrim_style_actor_gains_then_loses_a_player`, `yielding_when_another_writer_swapped_the_clip_mid_walk`:

```
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of
byroredux_scripting::cinematic::ActorCinematicState while holding
byroredux_core::animation::player::AnimationPlayer — that closes a cycle:
ActorCinematicState → Transform → AnimationPlayer → ActorCinematicState
```

**Site**: `byroredux/src/systems/walk_anim.rs:116-126` — `npc_walk_animation_system` acquires its seven queries in the order WalkAnimation → Transform → AnimationPlayer → HavokAnimationTarget → Seated → Dead → ActorCinematicState, observing the edges Transform → AnimationPlayer and AnimationPlayer → ActorCinematicState. Some other site (the cinematic consumer is the obvious candidate — verify before fixing) observes ActorCinematicState → Transform, closing the cycle cross-thread.

**Impact**: latent ABBA deadlock risk between the walk-takeover system and the cinematic system, not an observed hang — the debug graph panics at the second observation. Both systems run on the PostUpdate schedule; a schedule change that overlaps them turns this into a real deadlock.

**Suggested fix**: pick the process-wide order per `docs/engine/ecs.md`'s canonical-acquisition section (AnimationPlayer and ActorCinematicState are not yet in the documented chain — add them), then either reorder walk_anim's acquisition (cinematic before player) or fix the other site that holds ActorCinematicState while acquiring Transform. Extend the documented canonical order in ecs.md with whatever wins. Pin with the five failing tests under the checker (they already fail; the fix makes them pass without weakening the graph).

---

# 4547: playable-smoke.yml cannot dispatch the p0[oblivion] route the fixture declares — game choice list and forwarded data-env both omit oblivion

State: OPEN  Labels: ['bug', 'low', 'tech-debt']

**Discovered**: 2026-09-20 (Wave-2 pre-wave sweep of `docs/engine/near-term-action-plan.md`; pre-existing, never filed)

**The gap**: `docs/smoke-tests/fixtures/oblivion.env` declares `FIXTURE_GATES=(p0-door-interaction)` with a full measured route (AbandonedMine → Tamriel exterior, REFR 00033AD4), and the data is present on the dev machine. But the only lane that runs the real gates — the manually dispatched `Playable Smoke Gates` workflow — cannot select it:

1. `.github/workflows/playable-smoke.yml` `inputs.game.options` lists `skyrim_se, fnv, fo3, fo4` — no `oblivion`.
2. The same workflow's `env` block forwards `BYROREDUX_SKYRIM_DATA` / `_FNV_DATA` / `_FO3_DATA` / `_FO4_DATA` from `vars` — no `BYROREDUX_OBLIVION_DATA`, so even a hand-edited dispatch would fall back to the fixture's local default path on the runner and SKIP 77 (which `run_gate` then turns into an error).

**Consequence**: `scripts/check-playable-smoke-contracts.sh` dutifully verifies `p0[oblivion]`'s data-less SKIP shape (exit 77 + explicit diagnostic), so CI reads the route as contract-covered — but the measured route itself has never been dispatchable. The contract lane gives false confidence exactly the way its own comment warns against ("a title could be 'covered' by a gate that silently no-ops").

**Local check** (this machine, data present): `smoke_load_fixture p0-door-interaction oblivion && smoke_require_data` passes — the route is live, it just has no lane.

**Suggested fix**: add `oblivion` to the workflow's game choices, forward `BYROREDUX_OBLIVION_DATA: ${{ vars.BYROREDUX_OBLIVION_DATA }}`, and set that var on the `byroredux-game-data` runner. Optionally extend `check-playable-smoke-contracts.sh` to fail a fixture whose FIXTURE_GATES declares a gate no workflow choice can dispatch (the general form of this gap).

---

# 4548: FO4 facegeom BGSMs bind an _msn map while authoring Model_Space_Normals=false — those faces take the tangent-space path on an object-space normal map

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'legacy-compat']

**Source**: side finding of #3922 (fixed b56b649a9), surfaced by `byroredux/examples/msn_basis_probe.rs` while extending the basis measurement to FO4. Never filed; flagged "needs verification through the full material merge" at the time.

**Finding**: FO4's `facecustomization` _msn set (facegeom heads) ships BGSMs whose SLSF1 `Model_Space_Normals` bit is **false** while the material binds an `_msn` texture. Probe evidence (pre-#3922 basis measurement): through the MSN branch those maps select the flip at +0.411/+0.477 mean cosine vs −0.419/−0.525 identity — i.e. they ARE object-space (model-space) maps. But with the bit false, the in-engine classification routes the material down the **tangent-space** normal path and samples an object-space map through it — a wrong-basis shading on FO4 faces that #3922's fix does not reach (it fixed the MSN branch's basis, not the routing).

FO4 body/hands use tangent-space `_n` (correct), and FO4's remaining _msn corpus is terrain (non-discriminating, same as Skyrim LOD) — the face set is the live miss.

**Verification needed before fixing** (why this sat unfiled):
1. Trace one real FO4 face BGSM (e.g. a facecustomization head material) through `merge_external_material` → `translate_material` and confirm `model_space_normals` stays false while the `_msn` slot binds — i.e. the bit is genuinely authored false, not dropped by a merge/translate step.
2. Decide the rule from a source: does FO4 facegeom imply model-space normals regardless of the SLSF1 bit (a Creation-CLSG/FaceGeom convention, cf. nifly's facegeom handling), or do these BGSMs carry a different flag that should map? Prefer fixing the classifier input over a filename heuristic.
3. Confirm which shader arm the faces actually take in-engine post-#3922 (tangent-space path confirmed, or does the reconstruction arm already catch them via the blue-constant-zero shape?).

**Impact**: FO4 facecustomization heads (NPC facegen + player face) shade with normals in the wrong basis — plausibly the long-standing "FO4 faces look off" class. No crash.

**Suggested fix**: after verification, correct the classification (MSN routing keyed on the bound `_msn` slot + authored convention, or the missing-bit implication for the facegeom family), extend `msn_basis_probe` to print the resolved route per material, and pin with a material-merge test for the specific face BGSM shape.

---

# 4549: NIFAL-D2-2026-09-21-01: Static NiTransform translate boundary is non-finite-blind — NaN/inf rotations, translations and scales reach BLAS/TLAS as a non-finite AABB

State: OPEN  Labels: ['bug', 'nif-parser', 'high', 'vulkan', 'safety', 'nif', 'nifal']

**Severity**: HIGH · **Dimension**: Geometry/Transform · **Tier Violated**: no-leak · **Game Affected**: all (corrupt/hand-edited NIFs, modded content; vanilla incidence 0)
**Location**: `crates/nif/src/rotation.rs:14-20`, `:69-81`, `:165-197`; `crates/nif/src/import/coord.rs:58-70`; `crates/nif/src/stream.rs:745-755` / `:769-779` (both readers — translation/scale ungated); consumer `byroredux/src/scene/nif_loader.rs:1245-1253`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
Every gate on the static-transform path is an unordered float comparison, and all such comparisons are false for NaN:
- A **NaN** rotation matrix passes `sanitize_rotation` unchanged — `is_degenerate_rotation`'s `(det-1.0).abs() >= 0.1` is false on a NaN determinant and `is_non_orthonormal`'s column checks are likewise NaN-blind. `zup_matrix_to_yup_quat`'s `(det - 1.0).abs() < 0.1` fast path is also false, routing to SVD whose arithmetic on NaN yields an all-NaN quaternion (`normalize_quat`'s zero-length guard does not catch NaN).
- An **infinite** entry trips the degenerate branch, but `repair_rotation_svd_or_identity` then *returns a NaN matrix*: `max_sv < 0.01` (rotation.rs:176) is false for NaN singular values, so the "no meaningful orientation → identity" escape never fires — the repair function violates its own doc contract for non-finite input.
- `translation` and `scale` have no finite gate at all.

The NaN flows `Quat::from_xyzw` → `Transform` → `GlobalTransform` → TLAS instance / skinned BLAS refit with a non-finite AABB — undefined behaviour under Vulkan's AS-build contract, the exact chain `d53be91be` closed for the animation side (#4396/#4397). Repo-wide grep confirms no `is_finite` gate on static transforms downstream; the single downstream check (`cell_loader/spawn.rs:200-206`) only skips the mesh from the collision-proxy bounds union — the entity still spawns poisoned, and that check is itself the leak pattern in miniature. The collision and emitter paths already gate their own transforms, so geometry is the outlier, not the convention.

### Evidence
Call path: `read_ni_transform` (raw point + `sanitize_rotation(read_ni_matrix3())` + raw scale) → `compose_transforms` → `zup_matrix_to_yup_quat` (coord.rs:62) → `ImportedMesh.rotation` → `Quat::from_xyzw` (nif_loader.rs:1245, no gate).

### Impact
One corrupt float in any placed NIF poisons the entity's `GlobalTransform`; for a skinned or TLAS-instanced mesh that is a non-finite AABB in an acceleration-structure build — GPU UB / potential `VkDevice` loss, not a visual glitch. All games; corrupt/mod content only (vanilla incidence measured 0 for adjacent classes: #3532's 642,589-matrix census, #4397's 16.06M-key census).

### Related
#4396, #4397 (fixed anim-side siblings, `d53be91be`); #4166 (blast-radius precedent); #2383

### Suggested Fix
Gate the nine rotation cells, translation and scale on `is_finite()` at the two `stream.rs` read sites (or at the head of `sanitize_rotation`: non-finite → identity, matching the `max_sv < 0.01` precedent, behind the existing rate-limited warn). Have `repair_rotation_svd_or_identity` verify its output determinant is finite before returning.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix

- [ ] **SIBLING (anim side stays gated)**: the #4396/#4397 `normalized_rotation_sample` gates and the collision/emitter finite guards must not be weakened


---

# 4550: NIFAL-D5-2026-09-21-01: Zero-own-budget particle system inherits a sibling's authored budget via the whole-scene fallback

State: OPEN  Labels: ['bug', 'nif-parser', 'medium', 'nifal']

**Severity**: MEDIUM · **Dimension**: Particles · **Tier Violated**: no-fabrication · **Game Affected**: Oblivion / FO3 / FNV / Skyrim / FO4 (multi-emitter NIFs; Starfield N/A per #2354)
**Location**: `crates/nif/src/import/walk/emitter.rs:355-380` (`extract_emitter_max_particles`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`extract_emitter_max_particles` only returns from the per-instance `data_ref` path when the own block yields a budget `> 0` (`.filter(|m| *m > 0)` at :360). When the system's own `data_ref` resolves to its own data block and that block authored `0` (parsed to `None`, documented semantic "no authored budget → keep the preset", `blocks/particle.rs:167-177`), execution falls through to the whole-scene `find_map` scan — written for *unresolvable* refs and tested only as `null_data_ref_falls_back_to_the_whole_scene_scan`. In a multi-emitter NIF the scan returns the first budget-bearing block in block order, i.e. a **sibling** system's budget, which `apply_emitter_overlays` then clamps and writes into this system's `preset.max_particles`.

### Evidence
Own-ref hit requires `.and_then(|d| d.max_particles).filter(|m| *m > 0)`; both `None` (authored 0) and a non-`NiPSysBlock` target drop to the scene-wide scan (:379 has the same filter shape on the scan path). The existing zero-budget test uses a single-block scene, so the zero-own-budget + sibling-with-budget shape is untested.

### Impact
The emitter's pool silently jumps from its heuristic preset (~64–96) to `min(sibling_budget, 256)` — unlogged, game-agnostic, exactly on the multi-emitter NIFs #4261 measured at 67.3% of Oblivion+DLC content. Authored-0 budgets are rare, so frequency is low but the wrong value is silent.

### Related
#4261 (per-instance attribution — residual hole in that fix), #3344

### Suggested Fix
When `data_ref.index()` is `Some` **and** the target downcasts to `NiPSysBlock`, return its budget as-is (`None` stays `None`); reserve the whole-scene scan for NULL/non-resolving/non-downcasting refs. Add the two-system fixture (A budget 0, B budget 5000 → A keeps preset, B gets its own).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4551: NIFAL-D7-2026-09-21-01: Draugr combat clips are never installed on the cell-loader route — P2 combat takes silently no-op on --cell runs

State: OPEN  Labels: ['bug', 'animation', 'medium', 'game:skyrim', 'nifal']

**Severity**: MEDIUM · **Dimension**: Animation / controllers (P2 combat tail install path) · **Tier Violated**: none strictly (boundary correct; install-site coverage gap) · **Game Affected**: Skyrim
**Location**: `byroredux/src/cell_loader/load.rs:635-637` and `:1014-1015` (missing call) vs `byroredux/src/scene/world_setup.rs:1012-1017` (present); `byroredux/src/asset_provider/animation.rs:251` (`populate_draugr_combat_clips`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`populate_draugr_combat_clips` is invoked only from the `--game` world-setup route (sole production caller `world_setup.rs:1017`; the `animation.rs` hits are `#[cfg(test)]`). The `--esm … --cell` route installs `populate_idle_clip_runtime` + `populate_skyrim_walk_clip` at both of its sites but never the combat family, while the shared spawn finalize still inserts the `DraugrCombatAnim` marker (`npc_spawn/resumable.rs:1075-1080`, gated only on race + skeleton) — so `combat_feedback_system` runs, finds no `DraugrCombatClips` resource, and silently no-ops. The P2 gate smoke script drives exactly this route (`docs/smoke-tests/p2-melee-core.sh` → `--esm $FIXTURE_ESM --cell BleakFallsBarrow01`, target `encdraugr01ambushmelee2hheadm06` / REFR `0x0383F7`), so the fixture doc's step-4 gate ("assert the death take actually started") cannot pass where it is designed to run.

### Evidence
`grep -rn populate_draugr_combat_clips byroredux/src` → one production call site; `cell_loader/load.rs:636`/`:1015` install the walk clip with no combat sibling.

### Impact
The headline P2 capability ("play one attack/hit/death animation family and spatial sound family") is inert on the `--cell` route — the route every current smoke test and the AGENTS.md usage examples use — while working on `--game`.

### Related
`docs/engine/p2-combat-anim-sound-fixture.md` §Wiring step 4

### Suggested Fix
Add `populate_draugr_combat_clips` beside the two `populate_skyrim_walk_clip` call sites in `cell_loader/load.rs` (already idempotent + game-gated), or fold the three clip installers into one helper so a future route cannot pick up two of three.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix


---
