# PHYS-D6-2026-09-06-01: the XZ half of the collider-AABB-vs-body-origin split never landed — `0fd72cb6` fixed only Y, exactly as the predecessor finding predicted

Issue: #3973 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Water / Buoyancy · **Status**: NEW as an issue; re-derivation of **PHYS-D6-2026-08-30-01**, which was never filed under its own number
- **Location**: `crates/physics/src/water.rs:816` (union prefilter) · `:828-840` (current-volume containment) · `:855-870` (surface containment)
- **Trigger Conditions**: a Dynamic `bhk` body whose collider is offset in **X or Z** from its rigid-body origin — a `Compound`/`List` part at its own local isometry, or a ragdoll capsule bone (which since #3492 now *reaches* this loop) — positioned within that offset of a `WaterVolume`'s or `WaterCurrentVolume`'s XZ boundary. Shorelines and river banks.
- **Description**: `0fd72cb6` hoisted one shared `compute_aabb()` above both branches and moved the **vertical** metric onto the AABB centre. The horizontal pair was left on the rigid-body origin in *all three* places it appears, so the loop decides "is this body inside the volume?" using two different reference points on two different axes. The consequence is sharper than the pre-fix state: for the surface branch, `submerged_fraction(min_y, max_y, …)` is computed from the **AABB span** while eligibility to compute it at all is decided from the **origin's** XZ, so a body mostly outside the volume horizontally can still be handed a full submerged fraction.
- **Evidence**: `git show 0fd72cb6 -- crates/physics/src/water.rs` removes and re-adds the prefilter line **verbatim** (renamed `current_flow` → `aabb_y`); no XZ predicate is touched. At HEAD both containment predicates read `pos.x` / `pos.z` alongside `center_y` / `max_y`. `.claude/issues/3490/ISSUE.md:33-38` records the sibling search as scoped to *"other body-origin-vs-collider-centre **Y** reads"*.
- **Impact**: bounded and small — the discrepancy is the compound/bone XZ offset (tens of BU) against a water plane's XZ extent (typically thousands), so it mis-classifies only bodies within their own collider offset of a shore edge: a floating corpse or crate keeps or loses lift one body-radius early. No wake, force-accumulation or damping-restore invariant is violated. The reason to file remains scope completion — now compounded by a report that says it is done (PHYS-D6-2026-09-06-02).
- **Related**: #3490 (closed), #2887, PHYS-D6-2026-08-30-01, PHYS-D6-2026-09-06-02.
- **Suggested Fix**: what the 08-30 report already prescribed — derive one `reference_point` from `collider.compute_aabb().center()` at the top of the per-body loop and use it for the union prefilter, the current-volume containment, and the surface XZ/Y containment alike. A regression test can reuse the existing offset-compound fixture with the offset on X instead of Y.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
