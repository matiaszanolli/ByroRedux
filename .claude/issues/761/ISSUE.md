# #761: SF-D3-06: `material_reference_stub` hard-codes `texture_clamp_mode = 3` (WRAP_S_WRAP_T)

URL: https://github.com/matiaszanolli/ByroRedux/issues/761
Labels: documentation, nif-parser, low

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 3, SF-D3-06)
**Severity**: LOW
**Status**: NEW

## Description

`crates/nif/src/blocks/shader.rs:708-742` (`material_reference_stub`) hard-codes `texture_clamp_mode: 3`. For a Starfield `.mat` reference the actual clamp mode lives in the .mat file; the merge step is supposed to overwrite this. With no .mat parser today (SF-D3-03 / SF-D6-03), the default of 3 is what every Starfield surface sees.

## Impact

WRAP/WRAP is the most common case so the visible artifact is rare, but a Starfield material that authors a CLAMP texture has it silently rendered as WRAP. Acceptable as a default — needs a comment documenting the intent so a future auditor doesn't conclude this is a bug.

## Suggested Fix

Add a doc-comment above the `texture_clamp_mode: 3` literal explaining: (a) WRAP/WRAP is the most common Starfield default, (b) the actual value comes from the `.mat` parser when that lands, (c) cross-link to SF-D3-03 / SF-D6-03.

## Completeness Checks

- [ ] **TESTS**: n/a (doc-only).
- [ ] **SIBLING**: When the `.mat` parser lands, ensure the merge step overwrites `texture_clamp_mode` and add a regression test.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.
