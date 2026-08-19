# #2593: FO4-D1-01: stale exterior-absorption-dormant comment

**Severity**: LOW
**Dimension**: 1 (ESM/Plugin)
**Location**: `crates/plugin/src/esm/cell/wrld.rs:493-503`
**Status**: NEW
**Labels**: documentation, low, legacy-compat

## Description
A comment block describes FO4 exterior worldspace absorption as "dormant" /
not yet wired up. That has been stale since #2063/#2376 landed — exterior
absorption is live wiring today, not a documented gap.

## Evidence
`crates/plugin/src/esm/cell/wrld.rs:493-503` still carries the pre-#2063
"dormant" framing even though the exterior absorption path it describes has
been active since #2063 (and refined in #2376).

## Impact
Doc-only. No functional effect — but the comment actively misleads anyone
reading the file into thinking exterior absorption isn't wired.

## Suggested Fix
Update the comment to describe current (live) behavior, cross-referencing
#2063/#2376 instead of describing the pre-fix state.

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change

---

# #2596: FO4-DIM3-01: BTDX v8 doc mislabels it mesh-only, vanilla ships v8 texture archives too

**Severity**: LOW
**Dimension**: 3 (Archives)
**Location**: `crates/bsa/src/ba2.rs:20-26`, doc comment on `BA2_V_FO4_NEXT_GEN_MESH` (`:88-90`)
**Status**: NEW
**Labels**: documentation, import-pipeline, low

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
