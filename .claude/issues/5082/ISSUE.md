# NIF-D5-2026-09-29-01: BSEffectShaderProperty stub-gate comment gives the FO76 band as 152..171; the code gates bsver >= FO76 (155)

**Labels**: low,documentation,doc-rot,nif-parser,nif,game:fo76

**Source**: `docs/audits/AUDIT_NIF_2026-09-29.md`
**Severity**: LOW (doc-rot)
**Dimension**: Collision/Shader Parsing
**Game Affected**: Fallout 76 (bsver 155..=167) and Starfield
**Location**: `crates/nif/src/blocks/shader/effect.rs` — stub-gate comment in `BSEffectShaderProperty` parse ("FO76 (152..171) keeps the suffix-aware test")

## Description
The comment says "FO76 (152..171)". The gate is `if bsver >= crate::version::bsver::FO76` (155), with `>= STARFIELD` (172) taking `!name.is_empty()`. 152 is `FO76_SF2_CRCS`, the unrelated shader-flag-array threshold that #4161 separated from `FO76`. The lockstep sibling in `lighting.rs` carries no range. `.claude/commands/audit-nif/SKILL.md` Dim 5 notes the comment is wrong, but no issue tracks it; #4439 (e8ec0a608) rewrapped this very comment without fixing it.

## Impact
A reader aligning the two parsers, or an auditor checking band membership, gets an off-by-three lower bound that contradicts `version.rs`'s own doc.

## Related
#4161, #4439, #1510, #749, #4630 (open skill-drift bundle for audit-nif).

## Suggested Fix
Change the comment to "FO76 (`FO76`..`STARFIELD`, i.e. 155..171)", and drop the "known wrong" note from audit-nif SKILL.md in the same change.

Validated at HEAD 9fcfdc3fc: `effect.rs` comment reads "FO76 (152..171)" directly above `if bsver >= crate::version::bsver::FO76`.

## Completeness Checks
- [ ] **SIBLING**: `lighting.rs` `parse_fo76_plus` comment kept in lockstep
