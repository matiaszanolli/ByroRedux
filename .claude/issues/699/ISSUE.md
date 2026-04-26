# Issue #699: O6-N-02 + O6-N-03: BSA v103 'decompression NOT WORKING' claim is stale across audit-oblivion.md and ROADMAP.md (6 sites)

**Severity**: MEDIUM (doc-truth — drives every future audit on a false premise)
**Files**:
- `.claude/commands/audit-oblivion.md:19, 22`
- `ROADMAP.md:63-64, 71, 102, 314`

**Dimension**: Blockers / Doc Surface

The slash-command file states:
- Line 19: `BSA format | v103 — archive opens, **decompression NOT WORKING** (open blocker)`
- Line 22: `Cell loading | Deferred until BSA v103 decompression lands`

ROADMAP.md repeats the false claim in 4 places:
- Line 63-64: "Oblivion needs BSA v103 decompression before its cells load."
- Line 71 (compatibility matrix): "Exterior blocked on BSA v103 decompression."
- Line 102 (M32.5 row): "Oblivion exterior still blocked on BSA v103 decompression."
- Line 314 (Known Issues): "[ ] BSA v103 (Oblivion) decompression not working — blocks Oblivion exterior cell loading"

**Refuted by**: 2026-04-17 audit Dim 2 + 2026-04-25 audit Dim 2 — empirical extraction sweep at 147,629 / 147,629 (100%) across all 17 vanilla Oblivion BSAs.

**Real blocker**: cell loader not wired to TES4 worldspace + LAND records (same shape as FO3 was).

**Fix**:
- Strike "BSA v103 decompression" from all 6 sites.
- Replace with "Oblivion exterior blocked on TES4 worldspace + LAND wiring (same shape as FO3 was)."
- The "Known Issues" line 314 should be replaced with a real blocker (e.g. NPC spawning M41) or removed.
- Update slash-command file's Game Context table.

Bundles previously-separate findings O6-N-02 (slash-command file) and O6-N-03 (ROADMAP) — same false claim across multiple files; one PR closes both.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
