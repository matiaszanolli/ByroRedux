# SAVE-D6-2026-08-20-02: none of the four new save/load entry points (F5, F9, pause menu, --load) has a single test

**Issue**: #3168 — https://github.com/matiaszanolli/ByroRedux/issues/3168
**Finding ID**: `SAVE-D6-2026-08-20-02`
**Severity**: LOW
**Dimension**: 6 — M45.1 Live Load-Apply
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: low, tech-debt, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D6-2026-08-20-02`
**Severity**: LOW
**Dimension**: 6 — M45.1 Live Load-Apply
**Data-Loss Class**: none (test gap)

## Location

- `byroredux/src/save_io.rs:618` — `quicksave`
- `byroredux/src/save_io.rs:822` — `queue_load_slot`
- `byroredux/src/save_io.rs:827` — `quickload_latest`
- `crates/save/src/disk.rs:95-107` — `latest_slot`
- `byroredux/src/save_io/command_queue_tests.rs` and `byroredux/src/save_io/live_reload_tests.rs`
  — both **zero diff** this cycle

## Description

`git diff 85b77371..HEAD` over `byroredux/src/save_io/command_queue_tests.rs`,
`live_reload_tests.rs`, `validation_gate_tests.rs` and `crates/save/tests/round_trip.rs` is
**empty**.

The delta added four public entry points and a launch flag. The only new assertion anywhere is a
single `assert_eq!(latest_slot(&dir), Some(0))` appended to `disk.rs`'s pre-existing
`parse_slot_names` test. Nothing exercises `quicksave`, `quickload_latest`, `queue_load_slot`, or
the `--load` boot queue; nothing pins that F5 and the `save` console command produce identical
results; nothing covers `latest_slot` on an empty directory, on a directory holding only a
`.tmp`, or on an mtime tie.

## Evidence

As above, plus `grep -rn "quicksave\|quickload_latest\|queue_load_slot" byroredux/src crates/save`
returning only production call sites (`app_events.rs:295`, `main.rs:752`, `main.rs:388`) and the
definitions themselves.

## Impact

**The one surface a player touches is the least-guarded part of the subsystem.** It is also, per
`SAVE-D4-2026-08-20-01`, the surface whose failures are invisible at runtime — so tests are the
*only* place a regression could surface at all.

## Related

- **#3026** — CLOSED; the issue that added the surface.
- `SAVE-D4-2026-08-20-01` — why the runtime channel cannot be relied on here.

## Suggested Fix

Three tests in `command_queue_tests.rs`, all achievable with the existing in-memory fixtures:

1. `quicksave` and `SaveCommand.execute(world, "")` produce byte-identical output on the same world.
2. `quickload_latest` on an empty save dir returns the `"no save slots available"` error rather
   than panicking.
3. `latest_slot` ignores a `.tmp` sibling that is newer than every real slot.

## Completeness Checks
- [ ] **SIBLING**: the `--load` boot queue (`main.rs:385-393`) is covered too, not only F5/F9
- [ ] **TESTS**: all three named tests land, and `latest_slot`'s empty-dir / mtime-tie cases are pinned
