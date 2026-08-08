# SAFE-2026-08-07-06: audit-safety SKILL's Dimension-7 text misdescribes the #789 glass-passthrough guard as texture-equality; it's now materialKind == MATERIAL_KIND_GLASS

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2546
**Finding ID**: SAFE-2026-08-07-06

**Severity**: LOW
**Dimension**: 7 (RT IOR-Refraction Safety) / meta (skill doc-rot)
**Location**: `.claude/commands/audit-safety/SKILL.md:225-227`
**Status**: NEW (no open issue covers Dimension 7 of this skill; #2274 covers only Dimension 3)

## Description
The skill's Dimension-7 checklist states the passthrough guard is "the texture-equality identity check." That describes #789's *original* 2026-05 fix (`b38d16bc`). The mechanism was replaced on 2026-07-19 (`a09d2b76`, "Enhance alpha blending logic for glass materials"): the check is now keyed on `materials[hInst.materialId].materialKind == MATERIAL_KIND_GLASS`, not texture-index equality — texture-equality misfired whenever glass shared a texture with opaque geometry, letting the refraction ray skip through solid walls. This exact staleness was already caught once in a sibling report (`docs/audits/AUDIT_RENDERER_2026-06-09.md`), but `audit-safety`'s own Dimension-7 prose was never updated, so every subsequent `/audit-safety` run re-inherits the wrong mechanism description.

## Evidence
Confirmed directly: `SKILL.md:225-227` still says "texture-equality identity check"; current mechanism at `crates/renderer/shaders/triangle.frag:1711` — `(materials[hInst.materialId].materialKind == MATERIAL_KIND_GLASS)`.

## Impact
Documentation-only. The safety property (no unbounded recursion) does not depend on which identity check is used — it is structurally bounded by `REFRACT_PASSTHRU_BUDGET = 2` (`triangle.frag:1659,1680`, a fixed loop-iteration cap independent of the identity check). Risk is to future auditors/engineers who trust the skill text and either go looking for a check that no longer exists, or "fix" the current `materialKind` check back toward texture-equality believing it's a regression.

## Related
#789 (original bug), `docs/audits/AUDIT_RENDERER_2026-06-09.md` Dim 9 (independently caught the same drift), #2274 (sibling doc-rot issue, same root-cause pattern, different dimension).

## Suggested Fix
Update the skill's Dimension-7 bullet to read "keyed on `materialKind == MATERIAL_KIND_GLASS`," and note the loop's actual unbounded-recursion guard is the fixed `REFRACT_PASSTHRU_BUDGET = 2` cap, independent of which identity check gates continuation.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
