# SF-D3-02: ComponentDatabaseFile::parse peaks at 9.19 GB RSS on the vanilla CDB

**Issue**: #3055
**Severity**: MEDIUM
**Labels**: `medium,memory,performance,bug`
**Source report**: `docs/audits/AUDIT_STARFIELD_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-16.md` (Dimension 3 — CDB material provider).

**Location**: `crates/sfmaterial/src/reader.rs`:30

## Description

`ComponentDatabaseFile::parse` peaks at **9.19 GB RSS** on the vanilla CDB, with no ceiling.

## Evidence

Measured during this audit against the shipped `materialsbeta.cdb`. This is **legitimate vanilla input**, not a malformed-input case — the parse simply allocates without bound as it walks the database.

## Impact

9.19 GB peak RSS is more than the entire project VRAM+RAM budget, and more than many target machines have. On a 16 GB system it risks swap or OOM during a routine Starfield load; the dev machine's headroom is what makes it survivable locally.

Distinct from #2614, which is about a *corrupt* CDB aborting the process via an unvalidated chunk count — this is the vanilla path.

## Suggested Fix

Profile where the peak comes from (likely intermediate collections retained across the walk rather than the final data) and stream or drop as you go. A peak substantially above the 233 MB decompressed size suggests transient duplication rather than a fundamental requirement.

Measure before optimising — the 9.19 GB figure is the symptom, not the diagnosis.

## Related

- #2614 (SF-D3-01, 2026-06 — the corrupt-CDB abort; same parser, different input class)
- #3054 (SF-D3-01 this sweep — the cache holding the *result*)

## Completeness Checks
- [ ] **MEASURED**: The peak's source is profiled before any change
- [ ] **CEILING**: The parse has a bounded working set, or a documented requirement
- [ ] **VANILLA-PATH**: Verified on the shipped CDB, not a synthetic fixture
- [ ] **TESTS**: A regression test or bench pins peak RSS so it cannot silently regrow

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3055 --json state` when live state is needed.*
