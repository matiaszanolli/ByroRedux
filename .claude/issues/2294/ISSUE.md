# SAVE-D1-11: Scene/Dialogue/Package mid-playback progress omitted from registry without #1696-style documented rationale

**Filed from**: `docs/audits/AUDIT_SAVE_2026-08-03.md`
**Labels**: low, ecs, bug

**Severity**: LOW
**Dimension**: Snapshot Completeness & Determinism
**Data-Loss Class**: silent-drop (likely self-healing, but undocumented)
**Location**: `crates/scripting/src/scene.rs:142-178` (`ScenePlayer`), `crates/scripting/src/dialogue.rs:85-92` (`DialoguePlayback`), `crates/scripting/src/package.rs:131-138` (`ScenePackagePlayback`)

## Description
`ScenePlayer` tracks which phase a Bethesda `SCEN` scene has reached and which numbered actions have completed. This is meaningful mid-progress state, structurally similar to `AnimationPlayer`/`AnimationStack`, deliberately excluded from the live-overlay path (`#1696`) with an explicit code comment explaining why. `ScenePlayer`/`DialoguePlayback`/`ScenePackagePlayback` have no equivalent comment, issue reference, or registry entry — the omission may be the same intentional call, but nothing states the decision was made rather than simply not yet made.

## Impact
Low today — likely self-correcting via quest-stage-driven scene re-entry. But an undocumented omission is indistinguishable from an oversight on the next read.

## Suggested Fix
Either register all three (low cost) or add a one-line comment at each definition site citing this finding and stating explicitly why a cell reload safely reconstructs the equivalent state.

Classification at filing time: NEW, CONFIRMED against current HEAD — all three struct definitions verified live; `grep` for `#1696`-style citations near them returns nothing.
