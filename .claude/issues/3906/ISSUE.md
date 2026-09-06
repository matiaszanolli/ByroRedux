# #3906: SF-2026-09-05-D3-01: the Mat texture provenance is structurally unreachable — mat.dump advertises a src=mat label no code path can produce

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3906 --json state`).*

---

**Audit**: `docs/audits/AUDIT_STARFIELD_2026-09-05.md` · **Severity**: LOW · **Dimension**: 3

## Description

`ImportedTextureSource::Mat` is **structurally unreachable**. No code path can produce it, so the `mat.dump` console command advertises a `src=mat` provenance label that can never appear in its output.

## Impact

Diagnostics only. A developer using `mat.dump` to determine whether a texture came from a Starfield CDB `.mat` sidecar gets no such label and may conclude the sidecar was not consulted, when the real answer is that the provenance enum arm is never constructed.

## Suggested Fix

Either construct `ImportedTextureSource::Mat` at the CDB `.mat` resolution site so the label means what it says, or remove the arm and the `src=mat` documentation until the Phase-2 CDB path lands. Either resolves the mismatch; leaving an advertised-but-unreachable label is the thing to avoid.

## Completeness Checks
- [ ] **SIBLING**: Other `ImportedTextureSource` arms checked for reachability
- [ ] **TESTS**: If the arm is kept, a test pins that the CDB path produces it

## Related
- #3398 (Starfield CDB Phase 1)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
