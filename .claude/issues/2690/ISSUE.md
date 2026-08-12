# CONC-DOC: concurrency documentation & skill-text drift (2 sites)

**Issue**: #2690
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

Consolidated documentation / skill-text drift from the `/audit-concurrency` half of `/audit-suite renderer-deep` (2026-08-12). Filed as one issue rather than one-per-site because they share a root cause: **verification artefacts drifting out of sync with the code they certify, while continuing to look authoritative.**

**LEAD ITEM — CONC-D1-NEW-03 is a correction to the audit tooling and should be fixed first.** The `/audit-concurrency` Dimension-1 checklist instructs auditors to confirm the queue Mutex guard is *not* held across `queue_submit`. The shipped code **deliberately does hold it**, per `VUID-vkQueueSubmit-queue-00893` (refined by #1713), with a live regression test. Acting on the skill text as written would re-open two correct fixes. This is an XS edit that prevents a future audit run from causing a regression.

---

## CONC-D1-NEW-03

> Also treated as the lead item of section 5 (Documentation & Skill-Text Drift), because acting
> on the skill text as written would **re-open two correct fixes**.

- **Severity**: LOW (documentation)
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [SKILL.md](.claude/commands/audit-concurrency/SKILL.md) (Dimension 1, "Queue
  submission is single-Mutex" bullet); contradicted code at
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:3158-3196` + `:3243-3258` and
  [texture.rs](crates/renderer/src/vulkan/texture.rs)`:787-802`
- **Status**: NEW
- **Description**: The checklist states that because `vk::Queue` is `Copy`, "the canonical
  pattern is lock → copy the handle out → drop the guard → `queue_submit`: confirm the guard is
  **not** held across `queue_submit`/`queue_present`". The live code deliberately does the
  opposite, and says so in-line: `draw.rs:3160-3165` binds the `MutexGuard` specifically so it
  spans the call, citing `VUID-vkQueueSubmit-queue-00893` and the audit finding CONC-D2-NEW-01
  (2026-05-16) that introduced it; `texture.rs:793-798` repeats the reasoning and refines it
  under CONC-D1-01 (#1713) — guard held across the *submit*, released before the *fence wait*.
  There is a live regression test pinning this: `one_time_lock_scope_tests::…` in
  `texture.rs:846-880` asserts the lock → submit → wait ordering. Following the checklist
  literally would re-open two closed, correct fixes and re-introduce a genuine
  external-synchronisation violation.
- **Evidence**: `draw.rs:3166-3169` binds `let queue = self.graphics_queue.lock()…;` then calls
  `queue_submit(*queue, …)` and only `drop(queue)` afterwards (lines 3181 / 3195).
  `texture.rs:799-802` scopes `let q = queue.lock()…; device.queue_submit(*q, …)` to the submit
  only, with the fence wait outside.
- **Impact**: Audit-process defect. Not a runtime bug; it manufactures false findings and, if
  acted on, would produce a real CRITICAL-class data race on the queue.
- **Trigger Conditions**: n/a (documentation).
- **Verification Path**: `cargo test -p byroredux-renderer one_time_lock_scope_tests` already
  pins the correct behaviour.
- **Related**: CONC-D2-NEW-01 (audit 2026-05-16), CONC-D1-01 / #1713.
- **Suggested Fix**: Reword the bullet to "confirm the guard **is** held across `queue_submit` /
  `queue_present` (`VUID-vkQueueSubmit-queue-00893`) and released before any subsequent
  `wait_for_fences`".

---


---

## CONC-D3-NEW-03

- **Severity**: LOW
- **Dimension**: 3 — ECS Lock Ordering & Deadlock
- **Location**: [boot.rs](byroredux/src/boot.rs) — `install_runtime_registries`
  (`debug_assert_eq!(report_snapshot.known_conflict_count(), 0, …)`);
  [access.rs](crates/core/src/ecs/access.rs) — `analyze_pair`.
- **Status**: NEW
- **Description**: `analyze_pair` treats `WriteRead`, `ReadWrite` *and* `WriteWrite` overlaps as
  conflicts. Any cross-thread ABBA between two parallel systems needs, on each of the two shared
  locks, at least one side holding or requesting it in a blocking (write) mode — which is exactly
  an `analyze_pair` conflict. Therefore `known_conflict_count() == 0` over a stage's parallel
  batch is a *proof* that no ABBA exists between any two of its members, and it is the
  load-bearing reason this dimension has no reachable finding today. Nothing says so: the
  assert's own message frames it as a throughput/correctness nag ("make one side exclusive or
  split the access (see sys.accesses)"), `lock_tracker`'s module doc presents the runtime graph
  as the cross-thread guard without mentioning the static one, and
  [contributing.md](docs/contributing.md)'s `lock-order-check` row likewise.
- **Evidence**: `analyze_pair` runs six `collect_overlap` calls — write×read, read×write,
  write×write for components and again for resources — and returns `AccessConflict::Conflict` if
  any pair is non-empty. Paired with `install_runtime_registries`'s three `debug_assert_eq!`s
  (`undeclared_parallel_count`, `known_conflict_count`, `unknown_pair_count` all 0), the parallel
  batches are provably lock-disjoint. Confirmed by enumerating the two multi-member batches:
  `Stage::Early` = {`player_controller_system`, `weather_system`, `timer_tick_system`} and
  `Stage::Late` = {`camera_follow_system`, `reverb_zone_system`, `log_stats_system`,
  `metrics_sample_system`}; the only lock any two share is `TotalTime`, read-only on both sides.
- **Impact**: The invariant is easy to weaken by accident because nobody knows it is a deadlock
  guarantee. Two concrete ways it silently degrades: (1) the guard is `debug_assert_eq!`, so a
  release-only schedule divergence ships unchecked; (2) it is only as strong as the declarations,
  which is what CONC-D3-NEW-02 and #2389 erode. A reviewer told "this is just the parallelism
  report" will accept an incomplete declaration; a reviewer told "this is the deadlock proof"
  will not.
- **Trigger Conditions**: N/A — documentation/robustness gap, no timing window.
- **Verification Path**: `cargo test` / code review only. Nothing to observe at runtime; the
  check is a build-time property of the registration list.
- **Related**: #2393 (invariant near-vacuous — only 9 of ~53 systems ever paired; the two
  findings compound: a vacuous proof that nobody knows is a proof), #2391
  (`add_exclusive_with_access` has zero call sites, so the 43 exclusives get no such proof at
  all — they rely on `Scheduler::run`'s parallel-then-exclusive sequencing instead).
- **Suggested Fix**: One comment block at the `known_conflict_count` assert in `boot.rs` naming
  the property ("zero declared conflicts among a stage's parallel batch ⇒ the batch is
  lock-disjoint ⇒ no cross-thread ABBA is possible between its members; this, not the runtime
  graph, is the primary guard") and a cross-reference from `lock_tracker`'s module doc. Consider
  promoting the assert from `debug_assert_eq!` to a plain `assert!` — it runs once at
  construction, so the release cost is a single comparison.

---


---
*Filed from [`docs/audits/AUDIT_CONCURRENCY_2026-08-12.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-12.md).*

## Completeness Checks
- [ ] **SIBLING**: Every listed site corrected, not just the lead item
- [ ] **SKILL**: The `.claude/commands/audit-concurrency/SKILL.md` edit is verified against the shipped code, not against the old text
- [ ] **TESTS**: Where a doc pins an invariant, a test asserts it
