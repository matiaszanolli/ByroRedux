# SCR-D6-NEW3-05: quest_fragment_dispatch_system doc comment claims MoveTo still declines at lowering — it doesn't anymore

**Issue**: #2129
**Labels**: low, tech-debt, documentation
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: No
**Location**: `crates/scripting/src/fragment.rs:432-435`
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 6 — introduced by this session's own feature commits, postdates the #2029 fix)

## Description

The doc comment claims object-targeting effects "still decline at the lowering stage," naming `MoveTo` as an example — but `translate/effects.rs` now lowers both `AddItem` and `MoveTo` call shapes into real `Effect` variants, and `apply_effect` applies both directly against the live ECS world (added in this same session, `97bc3b94`).

## Impact

Cosmetic — a maintainer reading only this comment would incorrectly believe `MoveTo` fragments are inert, when they mutate live `Transform` state.

## Suggested Fix

Narrow the sentence to the effects that are actually still gapped (e.g. `Enable`/`Disable`) and drop `MoveTo`/`AddItem` from the "still decline" list.
