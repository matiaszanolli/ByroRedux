# PHYS-D4-2026-09-06-02: the PHYSAL spec still describes a two-CInfo constraint seam that #3330 and #3792 made a four-type one, and `ecs.md`'s #3655 edit asserts a non-overlap property the site it names does not have

Issue: #3970 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Ragdoll Articulation · **Status**: NEW
- **Location**: `docs/engine/physal.md:130-154` (§3) · `crates/nif/src/blocks/collision/constraints.rs:16-21`, `:44-46`, `:503-510`, `:521-525` · `docs/engine/ecs.md:682-689`
- **Trigger Conditions**: none at runtime. Surfaces when a maintainer plans the next constraint-decode slice, or adds a new `PhysicsWorld` consumer.
- **Description**: two doc-vs-code divergences, both introduced by the commits this pass verified. (1) At HEAD the seam decodes **four** wire types (`bhkRagdoll`, `bhkLimitedHinge`, `bhkHinge` — #3330, `bhkPrismatic` — #3792) into **three** CInfo structs and **three** importer functions, across three wrappers. `physal.md` still says *"the typed decode of **two** constraint CInfos"*, names only two importer functions, says *"One `RagdollCInfo` / `LimitedHingeCInfo` therefore feeds every game"*, lists only two byte layouts in its per-game table, and cites only two nif.xml structs. The parser's own docstrings match — *"the two joints a humanoid ragdoll uses"*, *"Every other type stays a `type_name`-only stub"* — all false at HEAD. Neither `1ccf1abe` nor `13fdb48e` touched `physal.md`. (2) `9ce9a9e6` added `ragdoll_writeback_system` to a list whose enclosing sentence reads *"every site … collects what it needs from the component queries, **drops them**, and only then takes the resource … the guards **do not overlap at all**."* Every other named site does exactly that; `ragdoll_writeback_system` holds **eight** storage guards across the entire `PhysicsWorld` guard. The *ordering* invariant the hoist was for is satisfied (`PhysicsWorld` last, nothing taken under it — it is a sink with no outgoing edges, so no cycle is recordable); the stated *non-overlap* property is not.
- **Impact**: documentation only. It matters because both documents are the ones a future change consults — `physal.md` §3 is where someone deciding whether `bhkBallAndSocket` is worth decoding will look and find a table that omits the two kinds already done.
- **Related**: #3330, #3792, #3655, #2883 (the earlier `physal.md` seam-count correction — same paragraph, same failure mode).
- **Suggested Fix**: in `physal.md` §3 replace "two constraint CInfos" with the live four-type / three-CInfo inventory, add `prismatic_joint` to the importer list, add Hinge and Prismatic rows to the *Constraint layout* column, add the two nif.xml structs to the sources-of-truth list, and note that prismatic `friction` and the motors are captured-and-unused; refresh the four `constraints.rs` docstrings in the same edit. In `ecs.md:686-689`, split `ragdoll_writeback_system` into its own clause: it satisfies *`PhysicsWorld` last with nothing under it* while holding its component guards — the weaker but sufficient form of the rule.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
