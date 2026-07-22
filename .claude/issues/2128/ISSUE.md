# SCR-D6-NEW3-04: quest_stages.rs module header still describes fragment dispatch as future work

**Issue**: #2128
**Labels**: low, tech-debt, documentation
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: No
**Location**: `crates/scripting/src/quest_stages.rs:20-36`
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 6 — same rot pattern as #2029, introduced in a sibling file that fix didn't touch)

## Description

The "What's deliberately NOT here yet" doc section still describes stage-fragment dispatch as "future work" whose loop "stays future work," even though `quest_fragment_dispatch_system` has shipped and is live-scheduled — `fragment.rs`'s own header was correctly updated in the #2029 fix, but this sibling module's header was not.

## Impact

Cosmetic only. A maintainer skimming `quest_stages.rs` first (a plausible, more-foundational entry point) would incorrectly believe fragment dispatch doesn't exist.

## Suggested Fix

Update the bullet to point at the now-shipped `fragment::quest_fragment_dispatch_system`, mirroring the language already fixed in `fragment.rs`'s header.
