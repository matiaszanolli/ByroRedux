# #1611 — NIF-D1-02: NetImmerse v3.3-v4.2 marker_* files truncate (out-of-span)

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: LOW (handled by the #698 truncate-not-Err recovery; the kept head still imports; tiny UI markers below the stated compat span) · **Dimension**: Stream Position · **Status**: NEW
**Source**: AUDIT_NIF_2026-06-14 (NIF-D1-02)
**Game Affected**: Oblivion data dir, but the files are pre-Gamebryo NetImmerse v3.3–v4.2 — *below* the documented v10.0.0.3 floor of the supported span.

**Location**: recovery path [lib.rs](crates/nif/src/lib.rs) (#698 inline-type-name truncate); the v3/v4 block layouts have no dedicated parsers.

## Description
6 `marker_*` UI meshes are NetImmerse v3.3–v4.2, predating the Gamebryo block layout the parser targets. They truncate gracefully via the #698 inline-type-name recovery rather than hard-`Err`. The prior audit grouped `marker_*` under NIF-NEW-03, but these are NetImmerse v3/v4 — a *distinct* root cause from the v10.1.0.x interpolator drift. All 6 are already in `oblivion_truncations.tsv`, so the no-new-truncation gate tracks them.

## Evidence
`sweep_oblivion.tsv` / `oblivion_truncations.tsv` rows; file-version 3.3.0.13 … 4.2.2.0.

## Impact
6 tiny editor-marker meshes (not gameplay content) keep their head and drop the tail. None are rendered in normal play.

## Related
#698; 06-13 NIF-NEW-03 (mis-grouped there).

## Suggested Fix
None required — document as out-of-span. If desired, exclude pre-v10.0.0.3 NetImmerse files from the Oblivion clean-rate denominator so they stop counting against the figure.

## Completeness Checks
- [ ] **SIBLING**: If a NetImmerse v3/v4 exclusion is added to the clean-rate denominator, apply it consistently across every sweep TSV / ROADMAP figure
- [ ] **TESTS**: If excluded, a test pins the denominator filter so the figure stays stable
