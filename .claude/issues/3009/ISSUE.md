# RT-2026-08-16-10: two of the four P2-slice modules have no runtime gate and no console surface

**Issue**: #3009
**Severity**: LOW
**Dimension**: P2 gameplay slice coverage
**Labels**: `low,gameplay,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (Dimension — P2 gameplay slice coverage).

**Location**: `byroredux/src/inventory.rs` · `byroredux/src/settings_io.rs`

## Description

`.claude/commands/_audit-common.md` scopes the un-owned gameplay slice as `combat.rs` + `inventory.rs` + `settings_io.rs` + the action half of `interaction.rs`, and names the three P0/P1/P2 scripts as its gates.

Measured against the scripts, the gates touch only `combat.rs` and the action half of `interaction.rs`. **`inventory.rs` (546 LOC) and `settings_io.rs` (334 LOC) — 880 LOC, a third of the slice — are exercised by no gate, and cannot be**: the engine exposes no `byro-dbg` command for inventory or settings state.

## Evidence

The complete command surface (`help` against a live engine) lists `interaction.status`, `combat.status`, `combat.approach`, `input.press`, `input.hold`, `input.look`, `player.status` — and **no inventory or settings command**.

`p2-melee-core.sh`:121 issues `entities Inventory`, but only to locate the Draugr by editor ID — it never inspects a single item, stack or equip slot. `settings_io.rs` has two `#[test]` functions and no runtime coverage.

Re-verified 2026-08-17: `grep -rn "inventory\.\|settings\." byroredux/src/commands/mod.rs` returns nothing.

## Impact

The slice's inventory/equipment half — which `docs/engine/playable-vertical-slice.md` gate 5 requires to survive a save/reload — has no runtime verification and no way to acquire one without new console commands.

LOW because it is a coverage gap rather than a defect, but it is the reason several other findings in this sweep had to be established by reading code rather than by observing the engine.

## Suggested Fix

Add `inventory.status` (items, stacks, equip slots) and `settings.status` console commands in `byroredux/src/commands/`, then extend `p2-melee-core.sh` (or add a P3 gate) to assert against them.

The command surface is the blocker — the gate cannot be written until it exists.

## Related

- #3000, #3008 (the P2 gate's other semantic gaps)
- `_audit-common.md`'s un-owned-subsystem table, which names this slice as the highest-value coverage gap

## Completeness Checks
- [ ] **COMMAND-FIRST**: The console surface lands before the gate that depends on it
- [ ] **SIBLING**: `settings_io.rs` covered too, not only `inventory.rs`
- [ ] **GATE-5**: The save/reload continuity requirement in `playable-vertical-slice.md` is actually assertable afterwards
- [ ] **TESTS**: A runtime gate exercises inventory state, not just its presence

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3009 --json state` when live state is needed.*
