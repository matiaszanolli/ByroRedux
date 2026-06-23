# SCR-D5-01: quest_stage_gate guarded shape emits a component while silently dropping sibling statements

Filed as: matiaszanolli/ByroRedux#1719
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:166-218` (`extract_stage_gate`, shape 1/2)
- **Labels**: high, legacy-compat, bug

## Description
Shape 3 (`single_set_stage`) requires the body be EXACTLY one statement. The guarded shape does NOT — it iterates `for stmt in body` and returns on the first recognizable `If`, ignoring sibling statements. `If guard / SetStage / EndIf` + e.g. `Self.Disable()` matches and silently drops the sibling, violating the docstring's promise to decline a body that carries logic beyond the advance.

## Impact
Silent partial lowering of a vanilla scripted REFR — quest advance fires but co-located effects vanish, no fallback. Decline-invariant leak (NIFAL-`Material`-wrong class).

## Suggested Fix
After `peel_player_gate`, require the post-peel body be exactly one statement (the guarded `If`), mirroring shape 3. Add `declines_guarded_with_extra_statements`.
