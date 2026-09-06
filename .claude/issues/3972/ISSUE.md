# PHYS-D5-2026-09-06-02: #3799's entire safety argument lives in an untested three-clause gate — all four new tests feed `probe_found_support` as a literal

Issue: #3972 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Character Controller · **Status**: NEW
- **Location**: `byroredux/src/systems/character.rs:341` (the gate), `:1734-1795` (the three `resolve_ground_contact` tests), `:1182-1195` (the function under test)
- **Trigger Conditions**: any future edit to `character.rs:341` or to the `jump_fired` / `swim` derivations above it.
- **Description**: `b9df7c46`'s commit message states the correctness argument in one sentence — *"The probe is suppressed while airborne, swimming, and on the frame a jump fires, so this can neither keep a falling character grounded nor re-ground a launch."* All three suppressions are clauses of a single `if`: `if swim.is_none() && controller.is_grounded && !jump_fired`. **Nothing pins any of them.** The three tests added for #3799 all call the pure `resolve_ground_contact` with `probe_found_support` supplied as a hand-written `bool` — they verify the OR, never the thing that computes the operand. `character_controller_system` has no test caller anywhere in the repo. Deleting `!jump_fired` would re-ground a jump on its launch frame; deleting `swim.is_none()` would let a swimmer's probe assert ground contact off the lake bed. Both leave the suite green.
- **Evidence**: `stationary_capsule_stays_grounded_instead_of_flickering` models the production loop with `let probe_found_support = grounded; let kcc_grounded = !grounded;` — a hand-written restatement of the gate's expected behaviour, in the same file as the gate and free to disagree with it silently.
- **Impact**: no present defect. It is the structural version of this pass's theme: the fix is pinned where it is easy (a pure function) and unpinned where it is load-bearing (the impure gate). The `!jump_fired` clause is one edit away from the double-jump / hover class.
- **Related**: #3799; same shape as #2857's closing note that *"`PhysicsWorld::move_character` currently has zero unit tests, which is why this is invisible to `cargo test`"* — still open for the controller system as a whole.
- **Suggested Fix**: extract the gate to a pure `fn support_probe_enabled(swimming: bool, was_grounded: bool, jump_fired: bool) -> bool` and pin its truth table next to `resolve_ground_contact`'s; or add a source-text pin in the style of `scheduler_access_tests.rs` asserting the literal still contains all three clauses.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
