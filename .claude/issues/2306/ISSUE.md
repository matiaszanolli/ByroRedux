# NIFAL-D8-02: nifal.md still cites deleted ShaderFlags<'a> typed view removed by #1897

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Shader-flags · **Tier Violated**: (doc, half-fixed)
**Location**: `docs/engine/nifal.md:253`
**Status**: NEW

## Description

`docs/engine/nifal.md:253` still cites the deleted `ShaderFlags<'a>` typed
view (removed by `#1897`) and calls the bit-collision guards "compile-time
equivalence asserts" when they are `#[test]` runtime asserts.
`.claude/commands/audit-nifal/SKILL.md` and `audit-nif/SKILL.md` were already
corrected — only the authoritative spec doc itself lags.

## Evidence

`grep -rn "ShaderFlags<'a>\|struct ShaderFlags" crates/nif/src crates/renderer/src`
returns no hits — the type no longer exists in the codebase — while
`nifal.md:253` still describes it as the current design ("the `ShaderFlags<'a>`
typed view + compile-time equivalence asserts").

## Impact

Doc-only — no behavior impact, but a stale architectural claim in the
authoritative spec that two sibling SKILL docs already caught and fixed.

## Suggested Fix

Update `nifal.md:253` to match the corrected description already in the
SKILL docs: namespaced constants per game (`shader_flags.rs`), dispatched by
block type, with `#[test]`-gated runtime equivalence asserts guarding bit
collisions — no `ShaderFlags<'a>` typed view.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix — no behavior change to pin)

## Filed as

GitHub issue #2306, labels: low, nif-parser, documentation.
