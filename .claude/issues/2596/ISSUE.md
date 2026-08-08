# FO4-DIM3-01: BTDX v8 doc mislabels it mesh-only, vanilla ships v8 texture archives too

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2596
**Finding ID**: FO4-DIM3-01

**Severity**: LOW
**Dimension**: 3 (Archives)
**Location**: `crates/bsa/src/ba2.rs:20-26`, doc comment on `BA2_V_FO4_NEXT_GEN_MESH` (`:88-90`)
**Status**: NEW

## Description
The module doc and the `BA2_V_FO4_NEXT_GEN_MESH` constant's doc comment
label BTDX version 8 as "mesh-only", but vanilla FO4 (Next-Gen Update) ships
a v8 DX10 **texture** archive too (`TexturesPatch.ba2`). The code itself is
unaffected — version dispatch doesn't branch on the "mesh-only" assumption —
this is doc-only drift.

## Evidence
`crates/bsa/src/ba2.rs:20-26` and the `BA2_V_FO4_NEXT_GEN_MESH` doc
(`:88-90`) describe v8 as mesh-only; `TexturesPatch.ba2` (vanilla FO4
Next-Gen Update content) is a v8 DX10 texture archive, contradicting the
comment.

## Impact
Doc-only — misleading comment, no functional defect. (Pre-existing open
issues #2360 and #1761 were re-observed unchanged during this pass and are
not re-filed here — they cover related-but-distinct BA2 version gaps.)

## Suggested Fix
Correct the doc comment to note that v8 covers both GNRL (texture) and
DX10-tagged (mesh) BTDX archives, not mesh-only.

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
