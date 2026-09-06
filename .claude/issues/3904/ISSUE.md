# #3904: Two stale NIFAL facts in the shared audit skill files: _audit-common says 18 roles (live 22), audit-renderer says translate_material has two callers (live three)

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3904 --json state`).*

---

**Audit**: found **independently by three audits** in the `texture-roles-deep` suite — `AUDIT_NIFAL` (D8-03), `AUDIT_STARFIELD` (D8-03), `AUDIT_RENDERER` (D6-03). Filed once.
**Severity**: LOW

## Description

Two stale facts in the shared audit skill files — the documents every audit skill loads as its layout authority, so an error here propagates into every future audit.

### 1. `.claude/commands/_audit-common.md:97` — role count

> `MaterialTextureSet<T>` … replaces per-game texture slot numbers with **18** named source-agnostic roles + `decals: [T; 4]`

Live struct (`crates/nif/src/import/types.rs`) has **22** named roles + `decals`.

#3465 added a parity test that `include_str!`s exactly two prose copies of this fact. This third copy — in the file every audit skill reads first — is the one it does not scan.

### 2. `.claude/commands/audit-renderer/SKILL.md:159` — caller count

> `translate_material` has **exactly two callers** — `byroredux/src/scene/nif_loader.rs` (loose NIF) and `byroredux/src/cell_loader/spawn.rs` (REFR placement). A third `Material {…}` literal downstream is a translation leak.

Live: **three** production callers. `byroredux/src/cornell.rs:2073` is the third, and it is legitimate (the Cornell RT test harness), not a leak.

## Impact

Documentation drifted from code, in the two files audits treat as ground truth. The renderer one is the more harmful: it instructs a future auditor that a third caller *is a boundary violation*, so a correct call site reads as a finding. That is a false-positive generator aimed directly at the NIFAL single-boundary rule.

## Suggested Fix

Update both numbers. Extend #3465's parity test to also scan `_audit-common.md` so the role count cannot drift a third time, and reword the renderer line to name the three legitimate callers (or to state the rule as "no `Material {…}` literal outside these call sites" rather than pinning a count that grows).

## Completeness Checks
- [ ] **SIBLING**: Any other prose copy of either fact found and corrected
- [ ] **TESTS**: #3465's parity test extended to cover `_audit-common.md`

## Related
- #3465 (the parity test that scans two of the three prose copies)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite — merges NIFAL-2026-09-05-D8-03, SF-2026-09-05-D8-03, REN-2026-09-05-D6-03.
