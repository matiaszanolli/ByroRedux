# OBL-2026-08-27-04

Issue: #3519 — https://github.com/matiaszanolli/ByroRedux/issues/3519
Filed: 2026-08-27 by /audit-publish from docs/audits/AUDIT_OBLIVION_2026-08-27.md

Source: `docs/audits/AUDIT_OBLIVION_2026-08-27.md` — finding `OBL-2026-08-27-04`

- **Severity**: LOW
- **Dimension**: 7 — Blocker Chain / doc accuracy
- **Location**: `ROADMAP.md:1102` (contradicting `ROADMAP.md:1100`)

## Description

`ROADMAP.md:1100` correctly records that the Oblivion interior-spawn grounding issue was **"Closed 2026-08-04 (`195fbb28`) … live-verified on `ICMarketDistrictTheGildedCarafe` grounded from frame 0 through a 120-frame run."**

The very next paragraph, `:1102`, still ends:

> The Oblivion inverted-normal residue remains separately open as #2193 pending a real-data retest; the Skyrim result does not by itself close that game-specific path.

`#2193` is **CLOSED** (`gh issue view 2193` → `CLOSED`, "OBL-2026-07-25-01: is_grounded stays false at Oblivion interior spawn").

## Evidence

Both lines are in the same "Known Issues" block of the same file; `:1100` names the closing commit and the live verification, `:1102` asks for a "real-data retest" that `:1100` already reports as done. The 2026-08-26 runtime baseline (`.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`) is that retest.

Re-verified at publish time: `gh issue view 2193 --json state` → `CLOSED`; the `:1102` sentence is still present in `ROADMAP.md`.

## Impact

Doc-rot only, but of the actively-misleading kind: `ROADMAP.md` is declared by `CLAUDE.md` to be an authoritative source, and this line is a standing invitation for a future Oblivion audit to reopen a fixed physics-grounding investigation. It is the same failure mode the memory note *tes_grounding_zero_mass_dynamic_fix* exists to prevent.

## Related

- `#2193` (closed)
- `#2419` (the previous ROADMAP-staleness item in the same neighbourhood)

## Suggested Fix

Replace the trailing sentence of `ROADMAP.md:1102` with a pointer to `:1100`'s closure — e.g. "The Oblivion residue this paragraph once tracked (`#2193`) closed independently on 2026-08-04; see the row above."

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other stale `#2193` / grounding references in ROADMAP / HISTORY / audit skills)
