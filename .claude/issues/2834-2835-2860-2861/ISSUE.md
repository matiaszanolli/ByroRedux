# #OPEN REN-D23-01: view_space_to_meters_factor is hard-coded to 1.0, but the engine's view space is Bethesda units (70 per metre)

### REN-D23-01: `view_space_to_meters_factor` is hard-coded to `1.0`, but the engine's view space is Bethesda units (70 per metre)

- **Severity**: MEDIUM
- **Dimension**: 23 — FSR Upscaler (SDK input contract)
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs` — the `view_space_to_meters_factor: 1.0` field of the `fsr3::DispatchDescription` literal inside `FrameUpscaler::record`
- **Status**: NEW
- **Description**: FSR derives view-space depth from the `camera_near` / `camera_far` / `camera_fov_angle_vertical` triple the engine supplies, then converts it to metres by multiplying by `viewSpaceToMetersFactor`. `build_fsr_frame_parameters` (`crates/renderer/src/vulkan/context/draw.rs`) sources that triple from `Camera::near` / `Camera::far` via `DofView`, i.e. **Bethesda units** — the whole renderer treats world/view units as BU, which is exactly why `crates/renderer/src/vulkan/volumetrics.rs` defines `WORLD_UNITS_PER_METER = byroredux_core::lighting::BETHESDA_UNITS_PER_METER` (= 70.0) and divides by it. The dispatch nevertheless declares one view-space unit == one metre, so every "metres" quantity inside the SDK is inflated 70×. The parameter appears nowhere in `docs/engine/fsr3-upscaler-integration-plan.md`'s input-contract section — an unexamined default, not a considered choice.
- **Evidence**: All SDK consumers go through `GetViewSpaceDepthInMeters(d) = GetViewSpaceDepth(d) * ViewSpaceToMetersFactor()`, and two are distance-tuned. `ReconstructedDepthMvPxThreshold(m) = ffxLerp(0.25f, 0.75f, ffxSaturate(m / 100.0f))` is intended to ramp over 0–100 m; fed BU it saturates at the far-field 0.75 px past 100 BU ≈ **1.43 m**, so effectively the entire frame uses the far-field motion-vector threshold. `const FfxFloat32 fDistanceFactor = ffxSaturate(0.75f - params.fFarthestDepthInMeters / 20.0f);` is zero for anything past 15 BU ≈ **0.21 m**, so that term is dead across the whole scene — and it is one of the `max` inputs to the history-rectification box scale, so near geometry never gets the tighter, more history-rejecting clipping box AMD tuned for it. Secondary: `prepare_inputs.h` clamps with `ffxMin(GetViewSpaceDepthInMeters(...), FSR3UPSCALER_FP16_MAX)`, which at factor 1.0 saturates at 65 504 BU ≈ 936 m against a `Camera::far` of 300 000 BU, so exterior far-field depth also flat-tops; at the correct factor the same range maps to ≈ 4285 m, comfortably inside FP16.

  Confirmed against current code (`crates/renderer/src/vulkan/frame_upscaler.rs`): `view_space_to_meters_factor: 1.0` is a literal, with no reference to `BETHESDA_UNITS_PER_METER` anywhere in the file.
- **Impact**: Reconstruction runs with two of its distance-dependent heuristics permanently pinned to their far-field values on every scene, at every preset, **on the engine's default render path** (`UpscalerMode::default()` is `Fsr3(Quality)`). Visual only — expected signature is extra history retention (mild ghosting/smearing) on near-camera surfaces, and small sub-pixel motions discarded during depth reconstruction. Invisible to `cargo test`, to the validation layers, and to the SSIM matrix in `byroredux/tests/upscaler_quality.rs`, which scores FSR against the engine's own TAA render rather than ground truth.
- **Related**: `crates/renderer/src/vulkan/volumetrics.rs` (the one subsystem that already does the BU→m conversion correctly); `docs/engine/fsr3-upscaler-integration-plan.md`; REN-D23-06 (same "SDK contract asserted by hand rather than queried" class, filed separately if in scope).
- **Suggested Fix**: Pass `1.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER` sourced from the existing constant rather than a new literal, and add the parameter to the plan's input-contract table. Re-run the quality matrix afterwards — **a shift in the committed thresholds is the measurement of the fix, not a regression.** (Blocked in practice on REN-D23-02: there is currently no working bench.)

## Completeness Checks
- [ ] **SIBLING**: Audit the rest of `fsr3::DispatchDescription`'s literal fields in `FrameUpscaler::record` for the same "unexamined default vs. sourced constant" gap
- [ ] **TESTS**: A regression asserts `view_space_to_meters_factor` is derived from `BETHESDA_UNITS_PER_METER`, not a bare literal; quality-matrix thresholds re-baselined after the fix (depends on REN-D23-02's bench harness being restored first)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-01).


---
# #OPEN REN-D23-02: the FSR bench harness changed measurement conditions and TSV schema in f19f7f15 without a re-bench; fsr_bench_report.py now crashes on its own committed archive

### REN-D23-02: the FSR bench harness changed its measurement conditions *and* its TSV schema in `f19f7f15` without a re-bench, and `fsr_bench_report.py` now crashes on its own committed archive

- **Severity**: MEDIUM
- **Dimension**: 23 — FSR Upscaler (bench-harness stability)
- **Location**: `scripts/fsr-bench-matrix.sh` (the `--bench-mode renderer-stepped --bench-camera "$CAMERA_PATH"` addition and the widened `printf` header) and `scripts/fsr_bench_report.py` (`main`), against `docs/audits/BENCH_R6a-stale-17_head_3a02b02d.tsv` and `docs/audits/BENCH_R6a-stale-17_control_e153b50c.tsv`
- **Status**: NEW
- **Description**: `git log --oneline -- scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py` returns exactly two commits: `e153b50c` (2026-07-24) and `f19f7f15` (2026-08-11). That second commit changed three things at once: (a) every run now executes `--bench-mode renderer-stepped --bench-camera "$CAMERA_PATH"` instead of the previous parked-camera capture, so **the workload being measured is different**; (b) the TSV gained six columns (`mode`, `camera`, `sim_time_s`, `lights`, `tlas`, `state_hash`); (c) the report script gained a scene-state fingerprint gate that indexes those columns unconditionally. **No bench table was refreshed in or after `f19f7f15`** — `git log f19f7f15..HEAD -- ROADMAP.md docs/engine/fsr3-upscaler-integration-plan.md docs/audits/BENCH_*.tsv` returns only the session-65 closeout, which touched neither. Every published FSR number therefore describes the pre-`f19f7f15` harness and cannot be compared against any run of the current one.
- **Evidence**: reproduced on the committed archive:
  ```
  $ python3 scripts/fsr_bench_report.py docs/audits/BENCH_R6a-stale-17_head_3a02b02d.tsv
    File ".../scripts/fsr_bench_report.py", line 102, in main
      row["mode"],
  KeyError: 'mode'
  ```
  Both committed TSVs still carry the 17-column pre-`f19f7f15` header while `fsr-bench-matrix.sh` now emits 23 columns. Independently reproduced against current `main` (`e4ab12e8`): the same `KeyError: 'mode'` traceback fires at `fsr_bench_report.py` line 102/171.
- **Impact**: The two artefacts the repo keeps specifically so cross-commit FSR comparisons stay checkable are **unreadable by the tool that produced them**, and the phase-7 net-frame-recovery table — the stated justification for FSR Quality being the engine default — has no reproducible path. The methodology change is itself defensible (`docs/engine/fsr3-troubleshooting.md` argues a parked camera hides disocclusion failures, and `f19f7f15` did update that doc), but it landed without re-taking the baseline it invalidates. ROADMAP.md independently flags the bench-of-record as 116 commits stale and "unreliable", so **there is currently no live FSR bench of any kind** — which blocks measuring REN-D23-01's fix.
- **Related**: #2560, #2084, #2279 (all closed, same bench-staleness class); ROADMAP.md R6a-stale-19; REN-D23-01 (blocked on this finding's fix).
- **Suggested Fix**: Re-run `scripts/fsr-bench-matrix.sh` on a current HEAD and replace the phase-7 table with the stepped-camera figures, labelling the old table with the harness commit it was taken on. Make `fsr_bench_report.py` tolerate a missing column (`row.get(key, "-")`) so the committed historical TSVs stay readable, or archive them with an explicit harness-commit header line. **No FPS or ms figure is asserted anywhere in this report; that is the point of the finding.**

## Completeness Checks
- [ ] **SIBLING**: Check `fsr_bench_report.py` for any other column accessed by bare `row[...]` indexing that would break the same way against an older-schema TSV
- [ ] **TESTS**: A regression feeds `fsr_bench_report.py` a pre-`f19f7f15` 17-column TSV and asserts graceful handling (no `KeyError`) instead of a crash

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D23-02).


---
# #OPEN PHYS-D1-01: Uniform GlobalTransform scale is silently dropped at collider creation — every authored-bhk collider ignores REFR XSCL

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: HIGH · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:585-587` (+ `crates/physics/src/convert.rs:57-59`); producers `byroredux/src/cell_loader/spawn.rs:1064-1090`, `byroredux/src/scene/nif_loader.rs:481-504`

> Merges PHYS-D3-03 (Dimension 3), which is the registration-side face of the same defect. PHYS-D4-01 (ragdoll joint pivots) is a *distinct* sibling this fix would not cover.

## Trigger Conditions
Any cell containing a REFR with `XSCL != 1.0` (or a collision-bearing `NiNode` with node scale != 1.0) whose NIF carries decodable classic `bhk` collision — Oblivion / FO3 / FNV / Skyrim, where scaled rocks, rubble and clutter are routine. FO4+/Starfield are unaffected (they take the synth-trimesh path, which bakes scale).

## Description
`spawn_collision_shapes` composes `final_scale = ref_scale * coll.scale` into both `Transform` and `GlobalTransform`, then `register_newcomers` builds the Rapier body from **translation and rotation only**:

```rust
// crates/physics/src/sync.rs:585-586 — final_scale never read again
let mut body_builder = RigidBodyBuilder::new(body_type)
    .position(iso_from_trs(n.global.translation, n.global.rotation))
```

and hands `collision_shape_to_parts` the *unscaled* `CollisionShape`. Nothing in `crates/physics` reads `GlobalTransform::scale` (`grep -rn '\.scale' crates/physics/src/sync.rs` returns nothing). Rapier exposes `SharedShape::scaled` and every primitive here is uniformly scalable, so this is a **dropped** value, not an unrepresentable one.

The checklist's three acceptable outcomes — reject / convert to TriMesh / explicitly document — are all unmet. The bhk path is the only one of three collider producers that does not pre-bake scale:
- `synthesize_static_trimesh` multiplies every vertex by `world_scale` (`byroredux/src/cell_loader/spawn.rs:340-343`)
- `spawn_packed_havok_proxy` passes `ref_scale` through (`byroredux/src/cell_loader/spawn.rs:263`)
- `spawn_collision_shapes` does neither.

Note: the engine's `Transform`/`GlobalTransform` scale is a scalar `f32`, so the *non-uniform* case the usual guidance warns about cannot arise. The uniform case is dropped instead.

## Impact
Colliders are the wrong size relative to the geometry they represent on every scaled placement — a 2x rock has a half-size collider (player clips into visible stone), a 0.5x one has an oversized invisible wall.

**Worse for multi-part collision**: `compose_trs` *does* scale each part's position, so a multi-node assembly on a scaled REFR gets its parts spread apart while each keeps its original size — literal gaps open between adjacent colliders that a KCC or dynamic body passes through.

Blast radius is every classic-chain game and every scaled placement. Invisible to `cargo test`: no test exercises a non-unit scale through the collider boundary.

## Suggested Fix
Bake the uniform scale at the single `collision_shape_to_parts` boundary (multiply primitive dims / vertex sets, and scale composed child translations during the compound flatten), or wrap each emitted part in `SharedShape::scaled`. Pass `GlobalTransform::scale` into `collision_shape_to_parts` explicitly so the drop cannot recur silently. Add a regression test that a `ref_scale = 2.0` cuboid emits doubled half-extents, and state the convention in `docs/engine/physics.md` beside the existing note at `:383-384` so all three producers document one rule.

## Related
- `docs/engine/physics.md:379-390` (the packed-Havok fallback bullets, which *do* bake scale)
- #2543 (CLOSED — clamped `ref_scale` on the synth proxy path)
- PHYS-D4-01 (ragdoll pivots — sibling, separate fix)
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other collider producers, other cast sites, other wake sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `physics_sync_system` still releases read guards before taking write guards
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parse->canonical boundary; no `GameKind`/`bsver` branch is introduced downstream of it (PHYSAL doctrine, `docs/engine/physal.md`)
- [ ] **TESTS**: A regression test pins this specific fix


---
# #OPEN PHYS-D1-02: build_ragdoll never applies default_contact_skin_bu — the two production collider sites diverge on the anti-leak margin

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/ragdoll.rs:177-185` vs `crates/physics/src/sync.rs:621-630`

## Trigger Conditions
Any activated ragdoll (`ragdoll <id>` console trigger, or the future death/hit-react trigger) that contacts world geometry at speed, or two ragdolls contacting each other. Fires on every game whose ragdolls thread (Oblivion / FO3 / FNV / Skyrim).

## Description
`ContactConfig::default_contact_skin_bu` (1.0 BU ~ 1.4 cm) is documented as the *"per-collider contact skin ... wide enough to keep TriMesh seams from leaking the kinematic player through"* (`config.rs:64-69`). `register_newcomers` applies it to **every** part it emits, regardless of shape kind. `build_ragdoll` receives the same `&ContactConfig` — it reads `cfg.ragdoll_extra_angular_damping` from it two lines earlier (`ragdoll.rs:161`) — but its `ColliderBuilder` chain omits `.contact_skin(...)` entirely, so every ragdoll collider is built with Rapier's default skin of `0.0`.

```rust
// crates/physics/src/ragdoll.rs:178 — no .contact_skin()
let col = ColliderBuilder::new(shape)
    .position(iso).friction(...).restitution(...).mass(part_mass).build();

// crates/physics/src/sync.rs:624 — has it
let collider = ColliderBuilder::new(shape)
    .position(iso).friction(...).restitution(...).mass(part_mass)
    .contact_skin(contact_skin).build();
```

`grep -rn "contact_skin" crates byroredux` returns exactly `sync.rs:621/629` + `config.rs` — the ragdoll site is the only unskinned production path. Nothing in the crate or the docs states this is deliberate; `config.rs:1-11` enumerates the unification sites and simply never lists the ragdoll builder.

## Impact
Rapier's skin is **additive between the two colliders in a pair** (*"a small gap ... equal to the sum of their skin"*, `rapier3d-0.22.0/src/geometry/collider.rs:1002-1008`), so:
- a ragdoll limb against skinned static world geometry gets **half** the intended margin (1.0 BU instead of 2.0)
- two ragdolls against each other get **zero**

That is exactly the "unskinned collider adjacent to a skinned one" seam the config was created to eliminate. Self-collision within one ragdoll is already suppressed by interaction groups (#2338), so the exposure is ragdoll-vs-world tunnelling through TriMesh seams and ragdoll-vs-ragdoll interpenetration.

## Suggested Fix
Add `.contact_skin(cfg.default_contact_skin_bu.max(0.0))` to the `build_ragdoll` collider chain. If zero skin is intentional for reduced-coordinate multibody stability, instead give `ContactConfig` an explicit `ragdoll_contact_skin_bu` field and say so in the module doc — so the divergence is a decision rather than an omission.

## Related
- #2338 (CLOSED — ragdoll interaction groups)
- `crates/physics/src/config.rs:1-11` module doc's site enumeration
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other collider producers, other cast sites, other wake sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, `physics_sync_system` still releases read guards before taking write guards
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parse->canonical boundary; no `GameKind`/`bsver` branch is introduced downstream of it (PHYSAL doctrine, `docs/engine/physal.md`)
- [ ] **TESTS**: A regression test pins this specific fix


---
