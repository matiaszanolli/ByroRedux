# PHYS-D7-2026-09-06-02: `phys.census` disables its own three-way split on a false premise — `NifImportRegistry` is a live world resource that a sibling console command already reads

Issue: #3966 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `byroredux/src/commands/physics.rs:129-132`; consumer `crates/physics/src/sync.rs:719-744`; the resource `byroredux/src/cell_loader/nif_import_registry.rs:316` (`impl Resource` at `:641`), accessor `:445-463`; the boot arm that *does* pass it `byroredux/src/scene.rs:603-606`; live precedent `byroredux/src/commands/assets.rs:523`
- **Status**: NEW — partial close of #2874 (CLOSED)
- **Trigger Conditions**: any `phys.census` run whose column comes back with **zero** colliders — the exact case the three-way split exists to resolve. On a non-empty column it still costs the `classic=/new_physics=/phantom=` line that distinguishes an FO4+ packed-Havok cell from a classic-`bhk` one.
- **Description**: the *report function* does implement the split — `sync.rs:719-744` has four arms keyed on `probe.authoring`, and both the 08-27b and 08-30 passes verified it **there**. Neither followed the value back to the *console* caller, which hard-codes it off with the comment *"The live path has no NIF import cache to sum."* **That premise is false.** `NifImportRegistry` is documented as a process-lifetime cache *"promoted to a world-resource so cell-to-cell traversal re-uses every previously-parsed mesh"*, it implements `Resource`, the boot arm reads it in two lines (`scene.rs:603-606`), and `commands/assets.rs:523` — a sibling console command in the same module tree — does `world.try_resource::<crate::cell_loader::NifImportRegistry>()` live.
- **Evidence**: the console arm renders *"#2874 cell collision authoring unavailable (no NIF import cache) — 0 total below cannot be split into 'nothing authored' vs 'dropped in translation'."* — i.e. it prints the exact conflation #2874 was filed to remove, on the one route an operator can invoke. The boot arm on the identical world renders the discriminating text. `collision_authoring_totals`' own doc reasons that cache-scope is a superset of cell-scope and that *"zero totals do prove nothing colliding was ever authored, and that is the arm the census was mis-reporting"* — so the console arm is not avoiding a scoping hazard, it is discarding the discriminator its dependency was built to provide.
- **Impact**: `phys.census` cannot answer *"is there no collider here because nothing was authored, or because everything authored was dropped in translation?"* — the split that decides whether the bug lives in the ESM/REFR layer or in `bhk` decode/registration. #2876's own module doc claims the console command exists so *"the `phys.census` console command can render the identical text"*; it does not. Worst on FO4/FO76/Starfield, where `new_physics > 0` is the marker that the packed-Havok proxy was supposed to fire.
- **Related**: #2874 (this is its unclosed third), #2876 (introduced the gap). Same call site as PHYS-D7-2026-09-06-01.
- **Suggested Fix**: replace `authoring: None` with the two lines from `scene.rs:603-606` and delete the stale comment. Guard with a `commands_tests.rs` case asserting the output does **not** contain `"authoring unavailable"` on a world holding a `NifImportRegistry`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
