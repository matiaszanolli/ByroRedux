# #2656: SCR-D4-NEW11-01: parse_property_flags reaches across the newline and swallows the Auto of a following Auto State, silently demoting the script's auto-state

**Severity**: MEDIUM
**Dimension**: Papyrus Lexer & Pratt Parser (Dimension 4)
**Untrusted-Input**: Yes
**Location**: `crates/papyrus/src/parser/script.rs:421-453` (`parse_property_flags`), interacting with `crates/papyrus/src/parser/mod.rs:77-87` (`peek_with_span`) and `crates/papyrus/src/parser/script.rs:551-557` (`parse_state`)
**Status**: NEW

## Description

`parse_property_flags` is a `loop { match self.peek() { ... } }`, and `Parser::peek` deliberately **skips `Token::Newline`**. The flag loop therefore does not stop at the end of the property's declaration line -- it keeps scanning into subsequent lines looking for more flags.

`Auto` is both a property flag *and* the leading token of a top-level `Auto State` item. So when a **short-form property declaration is the last thing before an `Auto State`**, the flag loop consumes the state's `Auto`. `parse_state` then peeks, does not find `KwAuto`, and builds the state with `is_auto: false`. No diagnostic is emitted -- `parse_script` returns `Ok((script, []))`.

All six flag loops were checked; this is the only vulnerable one, because `Auto` is the only flag that is also a legal item-starter:

| Flag loop | Can an item start with one of its flags? | Verdict |
|---|---|---|
| `parse_property_flags` | **Yes -- `Auto State`** | **VULNERABLE** |
| `parse_script_header` | No | safe |
| `parse_function_flags` | No | safe |
| `parse_variable_flags` | No | safe (verified empirically) |
| `parse_group` flags | No | safe |

## Evidence

Reproduced with a throwaway integration test (since removed). Five cases, all parsing with **zero** reported errors:

```
A: Auto property + Auto State   -> STATE Waiting is_auto=false   <-- WRONG
B: control, plain `State`       -> STATE Waiting is_auto=false   (identical AST to A)
C: a Function separates them    -> STATE Waiting is_auto=true    (correct)
D: full-form property           -> STATE Waiting is_auto=true    (correct)
E: top-level variable           -> STATE Waiting is_auto=true    (correct)
```

A vs B is the load-bearing pair: a source file that says `Auto State Waiting` and one that says `State Waiting` produce **byte-identical ASTs**. C isolates the cause to adjacency. D is safe because the full-form path's `EndProperty` terminates `parse_property_accessors` first.

The crate's one guarding assertion (`crates/papyrus/tests/r5_round_trip.rs:96`, asserting `active.is_auto` on the real `defaultRumbleOnActivate.psc`) passes **only by accident**: every property in that fixture has a trailing `{ doc comment }`, and `peek()` skips `Newline` but not `DocComment`. Removing the doc comment makes the same script parse wrong.

## Impact

Bounded today, which is what keeps this out of HIGH: `is_auto` has no runtime consumer, the `.pex` path hardcodes it `false`, and `parse_script` has **no production caller** anywhere in the workspace (the engine's live scripting path consumes `.pex`, not `.psc`).

The exposure is forward-looking: the moment `.psc` gains a production consumer, or `is_auto` gains one, a script's auto-state is silently demoted with no diagnostic -- and the one test that would catch it only passes because of an incidental doc comment in the fixture.

## Related

#2185 (the sibling `skip_to_next_line` EOF-hang in the same file, fixed)

## Suggested Fix

Have `parse_property_flags` stop at a `Newline` -- either peek raw (non-newline-skipping) inside the flag loop, or record the property declaration's line and break when the next flag token's span crosses it. Add cases A-E above as regression tests, and make the `r5_round_trip` fixture assertion independent of the trailing doc comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
