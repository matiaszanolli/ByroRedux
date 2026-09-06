# PHYS-D7-2026-09-06-04: `awake_counts().1` is not an awake count — Rapier never drains `active_kinematic_set`, and the fast path knows it while the accessor and both render sites do not

Issue: #3975 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Queries & Diagnostics · **Status**: NEW
- **Location**: `crates/physics/src/world.rs:278-285` (the accessor + doc); render sites `byroredux/src/commands/physics.rs:164` + `:169-174` and `crates/physics/src/sync.rs:170-182`; the contradicting in-repo note at `crates/physics/src/world.rs:521-528`
- **Trigger Conditions**: any read of the second tuple element — `phys.stats` over `byro-dbg`, or `BYRO_PROFILE=1`'s per-frame `awake dyn={} kin={}` line — in a cell containing authored-keyframed clutter, FO4+/Starfield packed-Havok proxies (registered `Keyframed`), or a live actor's keyframed ragdoll bones.
- **Description**: the first element is honest — `IslandManager::update_active_set_and_energy` drains `active_dynamic_set` every step and re-pushes only bodies that failed the sleep test. The second is not: `active_kinematic_set` is **never drained**. A body is pushed in when it becomes kinematic or when its position/colliders change, and leaves only when removed or type-changed; the step loop merely iterates it and `continue`s past every body with zero velocity. So `active_kinematic_bodies().len()` is the count of **all live kinematic bodies**, awake or not.
- **Evidence**: the repo already established this — 240 lines below the accessor, in the fast path's own rationale: *"NOTE: we deliberately do NOT gate on `active_kinematic_bodies()`. Rapier keeps every kinematic body in that set structurally for its whole life … so it's never empty in a cell with authored-keyframed clutter."* The knowledge landed where acting on it mattered and was not carried to the accessor that publishes the same number to an operator. `phys.stats` consequently prints `awake: dynamic=0 kinematic=487 pending_wake=false` and then, from the next branch, `→ quiesced: step is taking the static-scene fast path` — a self-contradiction on consecutive lines.
- **Impact**: diagnostic-only and bounded — no production decision reads the second element (all consumers use `.0` or display only), and as a *leak* signal it is well-behaved, since it only falls when bodies are genuinely removed. The cost is a misdirected investigation: an operator or a future audit reading `kinematic=487` as churn will hunt a wake-discipline bug that does not exist. `body_count()` by contrast is correct (`RigidBodySet::len()` is the live arena count).
- **Related**: #2890 / #1698 (the fast path whose comment carries the correct statement). No prior audit flagged it — every earlier mention in `docs/audits/` reads `.0`.
- **Suggested Fix**: rename to `active_island_counts` (or return a named struct) and rewrite the doc to say the second element is *"every live kinematic body — Rapier keeps them in the active set structurally, see the fast-path note"*. Relabel both render sites `awake dyn=N · kinematic bodies=M`. A source-text assertion in the style of `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre` keeps the two statements from diverging again.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
