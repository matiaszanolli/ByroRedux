# #4077 — ESM-2026-09-09-D5-02

the `6 if current_cell.is_none()` arm in `parse_wrld_children_inner` rests on a premise the shipped data refutes

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4077 --json state`).

---

- **Severity**: LOW
- **Dimension**: CELL / WRLD Walkers
- **Record / Sub-record**: `WRLD` → GRUP 1 (World Children) → GRUP 6
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:277-291`
- **Status**: NEW
- **Description**: The arm's comment asserts *"Skyrim wraps the worldspace persistent CELL
  in an outer type-6 group (labelled with that CELL's FormID), then places the CELL record
  and its type-8 actor children inside it."* No shipped plugin does that. In Skyrim SE (and
  every other supported title) the worldspace-persistent CELL record is a **direct child of
  the type-1 World Children group**, and its type-6 children group follows it as a sibling —
  so `current_cell` is already `Some(None)` by the time the type-6 group is read, the
  guarded arm never fires, and the persistent cell is captured by `force_persistent = true`
  passed down from `parse_wrld_group` (`wrld.rs:41`) instead.
- **Evidence**: CELL-record parent-group-type census over every `.esm`/`.esl` under all seven
  game Data dirs (`cell_under6.py`, includes Bethesda DLC/CC content and third-party masters):
  `grep -E "\b6:"` returns **nothing** — parents are exclusively `1`, `3`, `5`. Per-master
  detail for the two titles the comment names:
  `Skyrim.esm` → `{3: 590, 1: 36, 5: 16942}`; `Fallout4.esm` → `{3: 1195, 1: 5, 5: 38965}`.
  The type-1 → type-6 edge that does exist (`Skyrim.esm` 36, `FalloutNV.esm` 13,
  `Starfield.esm` 431) is the persistent CELL's *children* group, read after the CELL record.
- **Impact**: None observed — the arm is guarded and unreachable on real data, so it costs
  nothing at runtime. It is a false premise sitting in a load-bearing walker: a reader who
  trusts it will mis-model the WRLD child topology (this is the same class the memory note
  "Audit Finding Hygiene" and #3755 were paid for). A crafted/odd plugin that opened a type-6
  group before any CELL at the type-1 level would also silently promote whatever CELL it
  contains to worldspace-persistent.
- **Related**: #3755 (an identical falsified-comment class on the ACRE arm), `wrld.rs:36-41`
  (the `force_persistent` entry that actually does the work).
- **Suggested Fix**: Either delete the arm (the `_ => skip_group` fall-through plus the
  existing `force_persistent` entry already covers vanilla), or keep it and rewrite the
  comment to say it is a defensive shape not observed in any shipped master, citing the
  census above.

---
