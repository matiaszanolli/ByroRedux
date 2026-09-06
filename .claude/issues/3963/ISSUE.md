# PHYS-D6-2026-09-06-02: `AUDIT_PHYSICS_2026-09-04.md` records a finding as FIXED that its own cited commit did not touch

Issue: #3963 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy (audit ground truth)
- **Location**: `docs/audits/AUDIT_PHYSICS_2026-09-04.md:54` (the state table) · `:62-72` (the checklist row) · `:104-115` (the Verification Log entry for #3490)
- **Status**: NEW
- **Trigger Conditions**: any future Dimension-6 pass that reads the 09-04 report's clean verdict — which is what the report exists to provide.
- **Description**: the state table reads `| #3490 … | OPEN, verified true (PHYS-D6-2026-08-30-01 extended it) | **FIXED** — 0fd72cb6 |`. `0fd72cb6` fixed #3490's own premise; it did not address the extension. The report names the extension by ID **in the same cell** and then marks the row fixed. Its Verification Log entry is accurate about what it checked (*"the diff hoists one shared `aabb_y` fetch above both branches"*) but never re-opens the extension it cited, and the checklist row generalises that Y-only check into *"both branches now share one `compute_aabb()` call"* — true of the **fetch**, false of the **containment predicate** the row is about.
- **Evidence**: `git show 0fd72cb6 -- crates/physics/src/water.rs` removes and re-adds the union prefilter line **verbatim** (renamed `current_flow` → `aabb_y`); no XZ predicate line is touched. `.claude/issues/3490/ISSUE.md:33-38` records the sibling search as scoped to *"other body-origin-vs-collider-centre **Y** reads"* — the XZ half was never in the fixer's search space. `grep -rn PHYS-D6-2026-08-30-01` across the repo returns exactly two hits: the report that filed it and the report that (incorrectly) closed it. It has no GitHub issue, because its own recommendation was "fold into #3490 rather than tracking separately", and #3490 closed without folding.
- **Impact**: the defect in PHYS-D6-2026-09-06-01 is now recorded as fixed in the only place it is recorded at all. A pass that trusts the 09-04 report — the normal and intended use of a clean report — will not re-check it, and #3490's own sibling-search note steers anyone who does look toward the Y axis only.
- **Related**: PHYS-D6-2026-09-06-01, #3490, PHYS-D6-2026-08-30-01.
- **Suggested Fix**: amend the 09-04 report's row to "FIXED (Y axis); XZ extension still open" and file the extension under its own number so it stops depending on a closed issue's scope. **Process rule worth adopting**: an audit that closes finding A *by absorbing* finding B must verify B's predicate independently rather than inheriting A's commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
