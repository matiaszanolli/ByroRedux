# PHYS-D5-2026-09-06-01: the per-frame ground probe is the one production floor cast with no walkable-normal screen, and #3799 promoted its answer to a co-authority on `is_grounded`

Issue: #3971 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Character Controller · **Status**: NEW
- **Location**: `byroredux/src/systems/character.rs:344-352` (the cast), `:353` (`probe_found_support = true`), `:400` (the OR into `grounded`); compare `crates/physics/src/world.rs:854-870` (unfiltered) vs `:880-899` (walkable-filtered)
- **Trigger Conditions**: `PlayerMode::Character`, `controller.is_grounded` already true, not swimming, no jump this frame, and a collider whose contact normal satisfies `|normal_y| < cos(max_slope_climb_deg)` within 36 BU below the capsule centre. Exterior rock faces and terrain on Skyrim/FNV/FO3 routinely exceed 50°; interiors mostly do not, which is why it is not visible in the FO4 session #3799 was filed from.
- **Description**: `world.rs` exposes three downward capsule casts; the walkable variant exists specifically to reject hits failing `min_walkable_normal_y` (#2193). Every other production floor probe uses the filtered form — the cold-start spawn ladder, the door-arrival ladder, `phys.census`. **The character controller's per-frame probe is the sole production caller of the unfiltered `cast_capsule_down`** (verified: every other call site is inside `world.rs`'s `#[cfg(test)]` module), and nothing says the omission is deliberate. Before `b9df7c46` that only affected a clamped motion correction; now `resolve_ground_contact` is `kcc_grounded || probe_found_support`, so the same unscreened hit sets the frame's `is_grounded`, which gates jump input and the probe's own next-frame execution.
- **Impact**: deliberately narrow. On a surface steeper than `max_slope_climb_deg`, `is_grounded` reads true on 100% of frames instead of ~50%, so jump is always available on a slope the spawn ladder would refuse (`plan_character_spawn` demotes the whole boot to FlyCam rather than stand the player there). It does **not** produce a "glued to a cliff" pin — on a slope the vertical gap is `offset / cos θ > offset`, so the correction is a real downward request and `handle_slopes` still deflects it. The larger cost is latent: `is_grounded` is currently consumed only by the controller, `save_io.rs`, `commands/view.rs` and a log — the moment the fall-damage / footstep / locomotion-state consumers #3799's own issue anticipates are written, they inherit a ground-contact signal with no walkability discipline and no comment warning them.
- **Related**: #2193, #2874, #2857, #3799.
- **Suggested Fix**: this needs a *decision*, not a mechanical filter swap — rapier's own `result.grounded` accepts anything within 89.94° of up, so applying `min_walkable_normal_y` here would make the probe stricter than the KCC it is ORed with. Either (a) switch to `cast_capsule_down_surface_and_normal` and gate only the `probe_found_support` half on the walkable normal, leaving `correction` on the raw hit, or (b) keep the behaviour and add one comment recording that the unfiltered form is intentional and why. The current silence is what makes this a drift risk rather than a design.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
