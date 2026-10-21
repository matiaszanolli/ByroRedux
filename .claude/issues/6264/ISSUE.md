# #6264: PEX-D4-2026-10-21-02: doc rot left by #5887 and #5884, plus a stale `Expr` table

**Labels**: low,scripting,documentation,doc-rot

**Source**: docs/audits/AUDIT_PAPYRUS_2026-10-21.md · **Severity**: LOW · **Dimension**: Papyrus Lexer & Parser · **Location**: `docs/engine/papyrus-parser.md:152,160`; `crates/papyrus/tests/r5_round_trip.rs:2`

**Audit status**: NEW (the first line is from `3081d23f2`; the `#5890` sweep in `d148cd27b` missed the other two)

**Untrusted-Input**: No

## Description

- `docs/engine/papyrus-parser.md:152`: the `Event` row says "a remote handler's `name` is `Sender.EventName`". Since #5887 (`3081d23f2`) the name is `Sender_EventName`, and the same file's `:133-136` already says so, so the file contradicts itself.
- `docs/engine/papyrus-parser.md:160`: the `Expr` row lists no `Is` (#5322; since #5443 the `.pex` side lowers to it too) and no `NewStruct` (FO4 `new S`). Both are variants in `crates/papyrus/src/ast.rs:276,285`.
- `crates/papyrus/tests/r5_round_trip.rs:2`: the module doc says the pipeline is "lex → preprocess → parse_script". `preprocess` was deleted by `af4a68ad7` (#5884).

## Impact

Documentation only. The `Event` row is the one a consumer writing a name matcher would read.

## Related

#5890, #5887, #5884 (all closed)

## Suggested Fix

Change `:152` to `Sender_EventName`, add `Is` and `NewStruct` to the `Expr` row, and drop "preprocess" from the round-trip doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other rows of the papyrus-parser.md AST tables vs crates/papyrus/src/ast.rs)
