# SK-D5-BSA-NEW-01: Stale 256 MB cap comments — actual enforced limit is 1 GB

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 5)
**GitHub issue**: #2326

**Severity**: LOW
**Location**: `crates/bsa/src/archive/extract.rs:97,118,166`, `crates/bsa/src/ba2.rs:491`

## Description

Commit `4a2b8200` bumped `MAX_CHUNK_BYTES` from 256 MB to 1 GB (to fit FO76
content) without updating four call-site comments that still describe the
old 256 MB figure.

## Evidence

All four stale comments confirmed still present at HEAD (1ae86f62);
`MAX_CHUNK_BYTES` itself is correctly 1 GB everywhere in code.

## Impact

Cosmetic only — the code correctly uses the constant everywhere; a future
reader trusting the comment over the constant could misjudge the actual
safety margin.

## Suggested Fix

Update the four comments to say "1 GB" or reference `MAX_CHUNK_BYTES` by
name instead of hardcoding the number in prose.

## Completeness Checks
- [ ] **SIBLING**: Grep the rest of `crates/bsa/` for any other stale `256 MB` / `MAX_CHUNK_BYTES`-adjacent prose
