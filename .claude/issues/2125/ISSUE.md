# SCR-D4-NEW3-01: A parser-level error in one function inside a State/Group/Struct discards the entire container

**Issue**: #2125
**Labels**: medium, bug
**Dimension**: Papyrus Lexer & Pratt Parser
**Untrusted-Input**: Yes (latent — no live `.psc`/SCTX caller today; will matter once a real frontend feeds this parser)
**Location**: `crates/papyrus/src/parser/script.rs:509-551` (`parse_state`), `:556-576` (`parse_struct`), `:579-619` (`parse_group`) — each parses its children with a bare `?` and no per-item catch; recovery only happens one level up, at `parse_script`'s top-level loop (`script.rs:77-85`)
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 4)

## Description

`parse_script`'s top-level loop is the only place that catches a parser `Err` and recovers (`push_error` + `skip_to_next_line`). `parse_state`/`parse_group`/`parse_struct` parse their own children (functions/events, properties, members) with a bare `?`, so a syntax error in, say, the third function of a `State` block propagates all the way up and the **entire `ScriptItem::State`** — including functions before the error that parsed perfectly — is discarded, not returned as a partial `State` with just the bad function dropped.

This is the same bug shape as the just-fixed #2025/SCR-D4-NEW2-01 (whole-file failure on one bad token), one container level deeper — the lex-level fix doesn't cover this parser-level gap.

## Evidence

Built a standalone scratch crate depending on `byroredux-papyrus` and ran `parse_script` on a 3-function `State` block where only the middle function has a genuine parser error (`int x = )`):
```papyrus
ScriptName Test extends ObjectReference
State MyState
    Function FunctionA()
        int a = 1
    EndFunction
    Function FunctionB()
        int x = )
    EndFunction
    Function FunctionC()
        int c = 3
    EndFunction
EndState
```
Result:
```
OK. errors=3
  err: expected expression, found ')' @ ...
  err: expected type, found 'EndFunction' @ ...
  err: expected type, found 'EndState' @ ...
total top-level body items: 1
item: Function(FunctionC)
```
`ScriptItem::State("MyState")` never appears in the AST at all — `FunctionA` (zero errors) is gone entirely, and `FunctionC` survives only by accident (line-by-line resync happens to land on its `Function` keyword, re-parsing it as a **top-level** function outside the state, structurally wrong).

Contrast with the lex-level fix (#2025), verified in the same session to isolate damage correctly even for a nested-`If`-inside-`Function` shape.

## Impact

For any script using `State` blocks (an idiomatic, common Papyrus pattern — the project's own `parse_full_rumble_on_activate_translation` fixture has three), one error in one state's function silently drops every other function in that state, with only cascading "unexpected token" noise pointing at it — no explicit "State X was dropped" diagnostic. The one function that resyncs onto a top-level position is also re-parented outside its `State`, which would corrupt a downstream state-membership-keyed recognizer. `parse_group`/`parse_struct` share the identical code shape (bare `?`, no catch) and are presumed to have the same gap.

## Suggested Fix

Give `parse_state`/`parse_group`/`parse_struct` their own per-child recovery loop mirroring the top-level one in `parse_script` — same fix shape as #1734/SCR-D4-02, one level deeper. Add a regression test, e.g. `parser_error_in_one_state_function_does_not_drop_sibling_functions_or_the_state`.
