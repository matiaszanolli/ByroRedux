# #1766: SCR-D5-NEW-01: guarded If inner body drops sibling statements (incomplete fix of #1719)

Filed from `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` on 2026-06-27. Snapshot as-filed (GitHub is authoritative for live state).

**Severity**: HIGH · **Dimension**: Recognizer-Chain Soundness · **Untrusted-Input**: Yes
**Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:244` (`match_guarded_if`) + `find_set_stage` (`:311-321`)
**Status**: NEW — incomplete fix of #1719
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` (SCR-D5-NEW-01)

## Description
#1719 enforced exactly-one-statement at the **handler-body** level (`extract_stage_gate:178` `if let [only] = body`), so a sibling *next to* the guarded `If` now declines. But `match_guarded_if` resolves the target stage with `find_set_stage(body)?` (`:244`), which `find_map`s the guarded `If`'s **inner** body and returns the **first** `SetStage`, ignoring any other statement in that inner body. Shape 3 (`single_set_stage:299`) does NOT have this hole — it requires the body be exactly `[SetStage]`. The two shapes are asymmetric, so the decline-on-unmodeled invariant still leaks one nesting level deeper than #1719 closed.

## Evidence
Probe against the live `translate_script` boundary (instance binds `MyQuest`):

```
after-sibling   If GetStageDone(10)==1 / SetStage(20) / Self.Disable() / EndIf   -> EMITTED  (LEAK: Self.Disable() dropped)
before-sibling  If GetStageDone(10)==1 / Self.Disable() / SetStage(20) / EndIf   -> EMITTED  (LEAK: unmodeled stmt BEFORE the advance dropped)
player-gated    If player / If GetStageDone(10)==1 / SetStage(20) / Self.Disable() / EndIf / EndIf -> EMITTED (LEAK)
clean           If GetStageDone(10)==1 / SetStage(20) / EndIf                    -> EMITTED  (correct, no false-decline)
```

The `before-sibling` case is the worse half: `find_set_stage` scans the whole inner body, so an unmodeled statement placed *ahead* of the SetStage is dropped too — precisely the failure mode #1719's commit message cites (`Self.Disable()` "silently dropped"), just nested inside the `If`.

## Impact
A guarded quest-door/activator whose authored intent is "advance the stage **and** disable/move/enable/notify" lowers to a bare quest advance with the second-or-later effect silently discarded — a false-positive lowering that corrupts game logic with no fallback (the same class the decline invariant exists to prevent). It also defeats the quest-disagreement guard (`:248`): a guarded body with `MyQuest.SetStage(20)` + `OtherQuest.SetStage(5)` only checks the first, dropping the second quest's advance.

## Suggested Fix
In `match_guarded_if`, replace `find_set_stage(body)?` with the exactly-one-statement form — require the guarded `If` body be a single `ExprStmt` that is a `SetStage` (reuse/generalize `single_set_stage`), decline otherwise. (A future extension that *models* the extra statements would route the guarded body through `effects::lower_fragment`; until then the conservative single-statement gate is the fix.)

## Completeness Checks
- [ ] **SIBLING**: confirm shape 3 (`single_set_stage`) and any other recognizer body-walk has no analogous first-of-many `find_*` that ignores trailing statements
- [ ] **TESTS**: add `declines_guarded_inner_sibling_{after,before}` + `declines_player_gated_inner_sibling` next to `declines_guarded_with_extra_statements`
