# #3782: SF-D3-02: material.rs asserts 'Starfield ships no .mat sidecar files' — true for vanilla, false on any install with Creation content (20 JSON .mat files measured)

**Labels**: documentation, import-pipeline, low, legacy-compat, game:starfield, doc-rot
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-30.md` — SF-D3-02 (LOW)
**Dimension**: 3 — CDB material database
**Location**: `byroredux/src/asset_provider/material.rs:1127` (the `.mat` short-circuit comment)

## Description

The comment states as fact that *"Starfield ships no `.mat` sidecar files"*, and that fact is what justifies never writing a JSON `.mat` resolver.

## Evidence

Measured across all 129 installed Starfield archives:

- **20 `.mat` files exist** — all from third-party Creation / mod archives, all JSON material-editor exports.
- **Zero `.bgsm` and zero `.bgem` anywhere** — that half of the premise is fully confirmed.

## Impact

The *conclusion* still holds today: there is no JSON `.mat` resolver, so running the dispatch on those 20 files would gain nothing. But the premise is the load-bearing part — it is what tells a future maintainer the case can never arise. It can, on any install with Creation content, and the comment says otherwise.

**Fix the comment, not the code.**

## Suggested Fix

Rewrite the comment to say what is measured: vanilla Starfield ships no `.mat`/`.bgsm`/`.bgem` sidecars; installed Creation/mod archives do ship JSON `.mat` exports (20 observed across 129 archives), and the short-circuit is retained because no JSON `.mat` resolver exists — not because the files cannot exist. Pin the count and date the way the surrounding measured comments do.

## Related

- #3398 — Starfield CDB Phase 2

## Completeness Checks
- [ ] **SIBLING**: Same "Starfield ships no .mat" phrasing checked at the other doc sites in `material.rs`
- [ ] **CANONICAL-BOUNDARY**: If the short-circuit is ever removed, the `.mat` decode belongs at the parser→`Material` boundary, not at render time. See `/audit-nifal`.
- [ ] **TESTS**: N/A — documentation-only
