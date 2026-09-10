# #4088 — INC-2026-09-09-03

`TRUNCATING_TEST_MODULES` is hand-maintained with no completeness gate, and the `SOURCES` doc overstates the ordering invariant that actually holds

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4088 --json state`).

---

- **Severity**: LOW
- **Dimension**: concurrency / scheduler source-shape coverage (`/audit-concurrency` Dim 4), test-gap, doc-rot
- **Location**: [`byroredux/src/boot/mod.rs:42-52`](../../byroredux/src/boot/mod.rs) (the doc), [`byroredux/src/boot/mod.rs:512-568`](../../byroredux/src/boot/mod.rs) (the list and its pin)
- **Changed in**: `byroredux/src/boot/mod.rs` (commit `8c5e02aa`)
- **Status**: NEW. Dedup: no issue or prior report covers it; file is one day old.
  Verified against code.
- **Description**: two related gaps in the same convention.
  1. The pin `every_production_file_precedes_the_first_test_module` proves the ordering
     property only for the three module names listed in `TRUNCATING_TEST_MODULES`.
     Nothing checks that the list is *complete*. A fourth source-shape module that
     truncates `SOURCES` at its own `mod` declaration — the convention this file
     explicitly invites — that is not added to the list would truncate silently and
     produce exactly the "scans silently assert on nothing" outcome. This is the same
     hazard the sibling gate `the_concat_list_covers_every_file_in_the_boot_directory`
     was written to close for *files*, left open for *modules*.
  2. The `SOURCES` doc states the enabling condition too broadly: *"That convention only
     holds while every production registration appears **before the first test module's
     text**"*. That is not true of the current ordering, and the concat is nevertheless
     correct — because the three truncation sentinels all live in `schedule/mod.rs`,
     which is deliberately last. `world.rs` is second in `SOURCES` and carries
     `#[cfg(test)] mod ai_storage_registration_tests` at line 388, ahead of all five
     stage files' registrations; `cli.rs` carries two more test modules at lines 85 and
     456. The pin's own doc (`mod.rs:535-539`) states the real invariant precisely —
     "before the *earliest such declaration*". The reader most likely to be misled is
     whoever adds truncation module #4 and reasons from the wrong rule.
- **Evidence**:
  ```rust
  // byroredux/src/boot/mod.rs:512  — hand-maintained, no completeness check
  const TRUNCATING_TEST_MODULES: &[&str] = &[
      "fragment_activation_order_tests",
      "scheduler_timings_gate_tests",
      "system_access_declaration_tests",
  ];
  ```
  ```
  $ grep -n '^#\[cfg(test)\]' byroredux/src/boot/world.rs
  388:#[cfg(test)]
  $ grep -n '^#\[cfg(test)\]' byroredux/src/boot/cli.rs
  85:#[cfg(test)]
  456:#[cfg(test)]
  ```
  and `SOURCES` orders `mod.rs, world.rs, schedule/early.rs, …, cli.rs, schedule/mod.rs`.
- **Impact**: coverage loss only. Same blast radius as INC-02.
- **Related**: INC-2026-09-09-02, #3855.
- **Suggested Fix**: derive `TRUNCATING_TEST_MODULES` from the source instead of
  restating it — scan `SOURCES` for `.split("mod <name>")` call sites, or add an
  assertion that every `mod *_tests {` declaration found in a file *other than*
  `schedule/mod.rs` is not used as a truncation sentinel anywhere. Separately, reword
  `SOURCES`'s doc to match the pin's own precise phrasing ("the earliest truncation
  sentinel", not "the first test module").

---
