# SF-D6-05: BSVER band 168-171 has no Starfield handling while STARFIELD doc comment claims retail starts at 168

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2639
**Finding ID**: SF-D6-05

**Severity**: LOW
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/version.rs:413-415`
**Status**: NEW

## Description
BSVER band 168–171 has no Starfield handling, while `STARFIELD = 172`'s own
doc comment claims retail starts at 168. Every Starfield-vs-FO76 branch
keys off `172`; content at 168–171 would take the full FO76 path and skip
tail capture. Observed bsver distribution across 87,994 retail NIFs is
`{172,173,174,175}` only — latent, not live, but the doc comment invites a
future "fix" that would silently re-break the era split.

## Evidence
Corpus-wide bsver histogram over 87,994 retail NIFs: only `{172,173,174,175}`
observed, contradicting the doc comment's claim that retail starts at 168.

## Impact
Latent — no live defect, but a doc/code mismatch that could mislead a
future edit.

## Suggested Fix
Correct the doc-comment to the observed retail range (172–175); note
168–171 is unattested.

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
