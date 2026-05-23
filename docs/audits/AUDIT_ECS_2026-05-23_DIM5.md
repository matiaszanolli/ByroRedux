# ECS Audit — Dimension 5: System & Scheduler — 2026-05-23

**Scope:** Targeted single-dimension audit per `/audit-ecs 5`.

**Predecessors:**
- [AUDIT_ECS_2026-05-16.md](AUDIT_ECS_2026-05-16.md) (last full sweep — dim 5 steady-state, no findings).
- M27 Phase 1+2 ([a9810d40](../../)) and Phase 3 ([05fe2bac](../../)) landed
  the entire declared-access surface for parallel-stage systems plus the
  structural conflict resolutions (`fly_camera_system` /
  `character_controller_system` merged into a dispatcher; `spin_system`
  and `audio_system` re-staged as exclusive). Audit window covers the
  scheduler's behaviour against that delta.

---

## Executive Summary

The scheduler core ([`crates/core/src/ecs/scheduler.rs`](../../crates/core/src/ecs/scheduler.rs))
is in steady-state. The four bullets the dimension lists are all
covered by the existing 20-test suite — `cargo test -p byroredux-core
--lib scheduler` runs **20/20 green**.

Findings concentrated at the **declared-access API surface** added under
M27 (R7). One MEDIUM and two LOW. None are regressions; all are
asymmetries that were latent before M27 landed and have become visible
now that exclusive systems carry semantic weight.

### Severity rollup

| Severity | Count | Where |
|----------|-------|-------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 1 | ECS-D5-NEW-01 (`add_exclusive_with_access` missing) |
| LOW      | 2 | ECS-D5-NEW-02 (`undeclared_count` scope), ECS-D5-NEW-03 (test coverage gaps) |

### Recommended next step

1. **ECS-D5-NEW-01** — add `add_exclusive_with_access` + sibling
   `try_add_exclusive_with_access` so exclusive systems can declare
   access in the same idiom as parallel ones. Two-method extension
   mirroring [`scheduler.rs:146-220`](../../crates/core/src/ecs/scheduler.rs#L146-L220).
2. **ECS-D5-NEW-02** — once #01 lands, decide whether `undeclared_count`
   should keep walking all systems or scope to parallel. Today neither
   semantic is internally consistent with M27 Phase 3's "0 undeclared"
   commit-message claim.

Suggested publish: `/audit-publish docs/audits/AUDIT_ECS_2026-05-23_DIM5.md`

---

## Verified-Clean Checks

All four explicit dimension bullets verified clean:

### Bullet 1 — Blanket impl for `Fn(&World, f32)` correct (no ownership issues)

[`system.rs:49-53`](../../crates/core/src/ecs/system.rs#L49-L53) bounds
the blanket on `FnMut(&World, f32) + Send + Sync`. `FnMut` (not `Fn`)
is correct: stateful closures (`move |_world, _dt| { counter += 1; }`)
need mutable self-capture, and the scheduler holds the entry as
`Box<dyn System>` calling `run(&mut self, …)`. Test:
[`scheduler.rs:closure_system`](../../crates/core/src/ecs/scheduler.rs#L518)
exercises the path end-to-end (no panic, mutation visible after `run`).

### Bullet 2 — Mutations from system N visible to system N+1 in same `run()`

Two cases:

- **Cross-stage** (deterministic): the `for (_stage, data) in &mut self.stages`
  iteration at [`scheduler.rs:316`](../../crates/core/src/ecs/scheduler.rs#L316)
  walks `BTreeMap<Stage, _>` keys in natural `Stage` ordinal order
  (`Early=0`, …, `Late=4`). Each stage's parallel batch completes
  before its exclusive batch, which completes before the next stage
  starts. Test:
  [`mutation_visible_across_stages`](../../crates/core/src/ecs/scheduler.rs#L599)
  pins this (Update mutates Position; PostUpdate reads it).
- **Within-stage parallel → exclusive boundary** (deterministic):
  exclusive systems run after the parallel batch in registration order.
  Test: [`exclusive_runs_after_parallel`](../../crates/core/src/ecs/scheduler.rs#L657).

Within the parallel batch itself, ordering is non-deterministic by
design (rayon `par_iter_mut().for_each`), and per-storage `RwLock`
serializes conflicting accesses. This is intentional — declared-access
M27 surfaces the analysis up-front so the operator can re-stage
contended pairs rather than relying on incidental order.

### Bullet 3 — Empty scheduler runs without panic

Test:
[`empty_scheduler_runs_cleanly`](../../crates/core/src/ecs/scheduler.rs#L704).
`Scheduler::new()` returns an empty `BTreeMap`; `run` iterates zero
stages → no-op. Sibling test
[`empty_stages_skipped`](../../crates/core/src/ecs/scheduler.rs#L713)
confirms intermediate-stage gaps don't trip the iteration.

### Bullet 4 — `system_names()` returns correct order

Test:
[`system_names_in_stage_order`](../../crates/core/src/ecs/scheduler.rs#L735).
The `BTreeMap` ordering plus `parallel.iter().chain(exclusive.iter())`
at [`scheduler.rs:78-81`](../../crates/core/src/ecs/scheduler.rs#L78-L81)
guarantees stage-order with parallel-before-exclusive within each
stage.

### Bonus checks (passed)

- **Re-entry safety (#868)**: Scheduler is `&mut self` on `run()`;
  there is no `impl Resource for Scheduler` anywhere
  (`grep -rn 'impl Resource for Scheduler' --include='*.rs'` → only
  `SchedulerAccessReport` wrapper, which is the report snapshot, not
  the scheduler itself). Re-entry from inside a system body remains
  structurally impossible.
- **Duplicate-name discipline**: `try_add_to` + `try_add_exclusive`
  reject duplicates at registration. The lax `add_to` /
  `add_exclusive` log a warning and proceed (intentional for closure
  ergonomics; closures share `type_name`).
- **System count + registration ordering**: 16 `add_exclusive` calls +
  10 `add_to_with_access` calls in `byroredux/src/main.rs` (count via
  `grep -c`); plus a handful of `add_to` plain registrations. All
  20/20 scheduler tests pass against the production wiring.

---

## Findings

### ECS-D5-NEW-01: `add_exclusive_with_access` missing — closure/function exclusives can't declare access

- **Severity**: MEDIUM
- **Dimension**: System & Scheduler — declared-access API surface (R7)
- **Location**: [`crates/core/src/ecs/scheduler.rs:227-270`](../../crates/core/src/ecs/scheduler.rs#L227-L270)
  (`add_exclusive` and `try_add_exclusive`; no `_with_access` siblings)
- **Status**: NEW
- **Description**:
  The parallel-stage registration API has four flavours:
  ```
  add_to                       add_to_with_access
  try_add_to                   try_add_to_with_access
  ```
  Each parallel slot has a closure-friendly entry point that takes an
  `Access` declaration via `declared_override`. The exclusive-stage
  surface has only two flavours:
  ```
  add_exclusive                <missing>
  try_add_exclusive            <missing>
  ```
  Closures and bare `fn(&World, f32)` items go through the blanket
  `impl<F: FnMut(&World, f32)> System for F` at
  [`system.rs:49`](../../crates/core/src/ecs/system.rs#L49), which
  inherits the default `fn access(&self) -> Option<Access> { None }`
  from the trait. So every exclusive system registered with these
  types — and that's *every* exclusive system in
  `byroredux/src/main.rs` today — sees `declared_access() == None`,
  classified as undeclared by `access_report`.

  Engine inventory:
  ```
  $ grep -c "add_exclusive\b" byroredux/src/main.rs
  16
  $ grep -c "add_to_with_access" byroredux/src/main.rs
  10
  ```

  The 16 exclusives cover the entire papyrus_demo dispatcher set
  (6 `add_exclusive` closures at
  [`main.rs:587-608`](../../byroredux/src/main.rs#L587-L608)), `spin_system`
  ([`main.rs:655`](../../byroredux/src/main.rs#L655)), the PostUpdate
  ordering chain (`footstep_system`, `particle_system`,
  `billboard_system`, `world_bound_propagation_system`,
  `submersion_system` at [`main.rs:677-696`](../../byroredux/src/main.rs#L677-L696)),
  the M27 Phase 3 re-staged `audio_system` at
  [`main.rs:760`](../../byroredux/src/main.rs#L760), and
  `event_cleanup_system` at
  [`main.rs:769`](../../byroredux/src/main.rs#L769). None of these have
  the option to declare access today without rewriting them as struct
  systems that `impl System` directly — which would force them to keep
  a `Self` carrier just for the declaration.

- **Evidence**:
  - The parallel path has the override at
    [`scheduler.rs:146-170`](../../crates/core/src/ecs/scheduler.rs#L146-L170):
    ```rust
    pub fn add_to_with_access<S: System + 'static>(
        &mut self,
        stage: Stage,
        system: S,
        access: Access,
    ) -> &mut Self { … declared_override: Some(access) … }
    ```
  - The exclusive path has no equivalent
    ([`scheduler.rs:227-270`](../../crates/core/src/ecs/scheduler.rs#L227-L270)):
    ```rust
    pub fn add_exclusive<S: System + 'static>(&mut self, stage: Stage, system: S) -> &mut Self {
        … declared_override: None, … // always None, no override entry point
    }
    ```
  - `SystemEntry::declared_access` at
    [`scheduler.rs:52-57`](../../crates/core/src/ecs/scheduler.rs#L52-L57)
    falls back to `system.access()` when `declared_override` is None;
    for the blanket-Fn systems that's `None` too.

- **Impact**:
  - The M27 Phase 3 commit message (`05fe2bac`) claims "0 undeclared"
    in `sys.accesses`. The actual count would be ~16 (every exclusive
    system shows as `(undeclared)` in the report — see the rendering
    at [`commands.rs:842-844`](../../byroredux/src/commands.rs#L842-L844)
    where `None` → `"(undeclared)"` string).
  - Operators reading the report can't tell apart "exclusive — can't
    declare access today" from "parallel — hasn't been migrated yet."
    Both show up identically. Future audits / migrations that target
    "undeclared systems" will dig through 16 exclusive entries before
    realizing none of them are migratable under the current API.
  - Forward-looking: any cross-stage conflict analysis (today the
    analyzer only walks parallel pairs within a stage) would need
    exclusive declarations as a prerequisite. The API gap is the
    structural block.

- **Related**:
  - M27 Phase 3 commit `05fe2bac` (the "0 undeclared" claim).
  - ECS-D5-NEW-02 below (paired metric-semantic gap that becomes
    decideable once this lands).

- **Suggested Fix**:
  Mirror `add_to_with_access` / `try_add_to_with_access` against the
  exclusive vector at [`scheduler.rs:227-270`](../../crates/core/src/ecs/scheduler.rs#L227-L270).
  Two new methods, both with `declared_override: Some(access)`:
  ```rust
  pub fn add_exclusive_with_access<S: System + 'static>(
      &mut self, stage: Stage, system: S, access: Access,
  ) -> &mut Self { … }

  pub fn try_add_exclusive_with_access<S: System + 'static>(
      &mut self, stage: Stage, system: S, access: Access,
  ) -> Result<&mut Self, String> { … }
  ```
  Pair with a regression test that asserts an `add_exclusive_with_access`
  registration's row in the report carries `declared = Some(_)`. Then
  migrate the 16 main.rs exclusives in a follow-up commit.

---

### ECS-D5-NEW-02: `undeclared_count` walks parallel + exclusive — metric scope contradicts M27 Phase 3 claim

- **Severity**: LOW
- **Dimension**: System & Scheduler — declared-access metric semantics (R7)
- **Location**: [`crates/core/src/ecs/scheduler.rs:452-458`](../../crates/core/src/ecs/scheduler.rs#L452-L458)
- **Status**: NEW
- **Description**:
  ```rust
  pub fn undeclared_count(&self) -> usize {
      self.stages
          .iter()
          .flat_map(|s| s.systems.iter())   // ← walks PARALLEL + EXCLUSIVE
          .filter(|row| row.declared.is_none())
          .count()
  }
  ```
  The metric counts every undeclared system regardless of phase. But
  exclusive systems contribute zero information to the conflict
  analyzer (which only walks parallel pairs at
  [`scheduler.rs:382-395`](../../crates/core/src/ecs/scheduler.rs#L382-L395)),
  and — per ECS-D5-NEW-01 — they have no API path to declare access
  in the first place. So the metric mixes two distinct populations:
  ```
  undeclared_count = (parallel systems not yet migrated)
                   + (exclusive systems that structurally can't declare today)
  ```
  M27 Phase 3's commit message claims this metric is `0`, which is
  only achievable for the first population.

- **Evidence**:
  - The conflict analyzer scope is parallel-only — `for i in 0..data.parallel.len()`
    at [`scheduler.rs:382`](../../crates/core/src/ecs/scheduler.rs#L382).
    Exclusive entries are listed in `systems` but never paired:
    [`scheduler.rs:exclusive_systems_are_listed_but_not_paired`](../../crates/core/src/ecs/scheduler.rs#L924).
  - The console renders `None` exclusives as `(undeclared)` at
    [`commands.rs:842-844`](../../byroredux/src/commands.rs#L842-L844),
    indistinguishable from a non-migrated parallel system.

- **Impact**:
  - **Today**: `sys.accesses` headline `"… {undeclared_count} undeclared …"`
    reads ~16 in production, contradicting the M27 Phase 3
    "0 undeclared" claim. Operators see a non-zero count that they
    can't drive to 0 with the current API.
  - **Forward-looking**: any KPI or CI gate that tracks "undeclared
    systems trending to zero" will plateau at the exclusive count
    rather than reaching zero.

- **Related**: ECS-D5-NEW-01 (the API fix that makes the "all
  declared" target achievable).

- **Suggested Fix**:
  Two options — pick one after ECS-D5-NEW-01 lands so the choice is
  informed by actual API symmetry:

  1. **Keep metric scope, document explicitly.** Rename / split into
     `undeclared_parallel_count()` + `undeclared_exclusive_count()`
     and have `sys.accesses` print both. Each becomes individually
     meaningful.
  2. **Scope metric to parallel only.** Change the iteration to
     `s.systems.iter().filter(|r| !r.is_exclusive)` and update the
     docstring. Matches M27 Phase 3's commit-message semantic and
     leaves exclusives out of the migration KPI entirely.

  Either fix is one-line plus a test. Defer the decision to the
  operator who lands ECS-D5-NEW-01.

---

### ECS-D5-NEW-03: Two scheduler test coverage gaps — all-five-stages chain + cross-feature parity

- **Severity**: LOW
- **Dimension**: System & Scheduler — test coverage
- **Location**: [`crates/core/src/ecs/scheduler.rs:570-594`](../../crates/core/src/ecs/scheduler.rs#L570-L594)
  (`stages_run_in_order` exercises 3 of 5 stages); whole-file
  ([`scheduler.rs:485-942`](../../crates/core/src/ecs/scheduler.rs#L485-L942))
  has no `#[cfg(feature = "parallel-scheduler")]` / `not(...)` parity test.
- **Status**: NEW
- **Description**:
  Two small coverage gaps surfaced while reading the test file:

  1. **All-stages-in-order**: `stages_run_in_order` exercises
     `Early → Update → PostUpdate` (3 of 5 stages). `Physics` and `Late`
     never appear in any ordering test. The `BTreeMap<Stage, _>`
     ordering by discriminant is robust against accidental reordering
     (the `Stage` enum is `#[derive(Ord)]` with explicit `= N`
     discriminants), but a test that pins all five in one sequence
     would document the contract better and catch a future drift if
     someone reorders the enum.
  2. **Parallel-scheduler feature parity**: The whole test module
     runs against whatever feature set `cargo test` resolves —
     default-on (`parallel-scheduler` enabled per
     [`Cargo.toml:default = ["parallel-scheduler"]`](../../crates/core/Cargo.toml#L?)),
     so `not(feature = "parallel-scheduler")` is only ever exercised
     when someone explicitly disables the default. The two `cfg`
     branches at [`scheduler.rs:318-329`](../../crates/core/src/ecs/scheduler.rs#L318-L329)
     diverge (rayon vs plain for-loop) and have different panic-
     propagation semantics documented at
     [`scheduler.rs:294-307`](../../crates/core/src/ecs/scheduler.rs#L294-L307).
     Neither path has a CI gate guaranteeing both build + run.

- **Evidence**:
  ```
  $ grep -n "Stage::Physics\|Stage::Late" crates/core/src/ecs/scheduler.rs | grep tests
  657:        scheduler.add_to(Stage::Late, |world: &World, _dt: f32| {
  673:        scheduler.add_exclusive(Stage::Late, |world: &World, _dt: f32| {
  723:        scheduler.add_to(Stage::Late, move |_: &World, _: f32| {
  737:        scheduler.add_to(Stage::Late, DamageOverTime { dps: 10.0 });
  929:        scheduler.add_to(Stage::Late, DamagePosition);
  ```
  No test reference to `Stage::Physics`.

- **Impact**:
  - **Coverage gap (LOW)**: a reorder of the `Stage` enum (e.g. adding
    a new stage between two existing ones) wouldn't be caught by the
    existing 3-of-5 ordering test. Today the discriminants are
    explicit, so accidental reorder is unlikely.
  - **Feature parity (LOW)**: a regression in the `not(parallel-scheduler)`
    branch (which is the build that ships when rayon needs to be
    omitted — embedded / WASM / debug-only configs) wouldn't surface
    until someone explicitly tested without the default. Not blocking
    today since the engine ships with rayon enabled.

- **Suggested Fix**:
  Two cheap tests:
  ```rust
  #[test]
  fn all_five_stages_run_in_order() {
      // Variant of stages_run_in_order with Early/Update/PostUpdate/Physics/Late
      // each incrementing the counter to expected ordinal.
  }

  // In CI: cargo test --no-default-features --features=ecs (or the
  // explicit non-parallel feature set) as a sibling job.
  ```
  Add the CI bit to `.github/workflows/` if the loss of rayon would
  otherwise silently drop a release variant.

---

## Steady-state checks (no findings)

- **Stage enum stability**: `Stage` discriminants pinned to 0..=4 with
  explicit `= N` syntax at [`scheduler.rs:24-35`](../../crates/core/src/ecs/scheduler.rs#L24-L35).
  No drift since 2026-05-16.
- **System trait surface**: 3 methods (`run`, `name`, `access`), all
  unchanged since R7 landed.
- **`run()` body shape**: `for stage in &mut self.stages → parallel batch
  → exclusive batch`. Unchanged structure; the M27 changes added access
  metadata around the registration site, not the run loop.
- **Duplicate-name discipline (#312)**: `try_add_to` /
  `try_add_to_with_access` / `try_add_exclusive` reject duplicates;
  lax variants log + accept. 3 tests pin the contract
  ([`scheduler.rs:750-803`](../../crates/core/src/ecs/scheduler.rs#L750-L803)).
- **R7 declared-access path (parallel side)**: 5 tests cover the
  declared/undeclared × conflict-shape matrix
  ([`scheduler.rs:830-921`](../../crates/core/src/ecs/scheduler.rs#L830-L921)).
  Override-beats-trait, multi-pair overlap, resource conflicts,
  Unknown-with-flags — all covered.
- **`access_report` lifecycle**: snapshotted once at engine init
  ([`main.rs:781-783`](../../byroredux/src/main.rs#L781-L783)), stored
  as `SchedulerAccessReport` resource. Read by `sys.accesses` console
  command only. No mid-run mutation path; report drift would require
  re-registering systems after startup (not exposed today).
- **Re-entry safety (#868)**: still structurally impossible —
  `Scheduler` is owned by `App`, not stored as a `Resource`.

---

## Summary

| Severity | Count | Disposition |
|----------|-------|-------------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 1 | ECS-D5-NEW-01 (`add_exclusive_with_access` missing) |
| LOW      | 2 | ECS-D5-NEW-02 (metric scope) + ECS-D5-NEW-03 (coverage) |

**Headline:** Scheduler core is in steady-state; all four dimension
bullets are covered by the live 20-test suite. The gaps cluster at the
R7 declared-access API surface — exclusive systems can't declare access,
which makes M27 Phase 3's "0 undeclared" KPI structurally
unreachable today. ECS-D5-NEW-01 is the cheapest unlock; ECS-D5-NEW-02
and -03 are follow-ups that become decideable once #01 lands.
