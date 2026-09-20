# PEX-D4-2026-09-19-01: six newline-skipping peek/check sites outside the Pratt loop glue adjacent lines with zero errors

- **ID**: D4-01
- **Labels**: medium,scripting,bug
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4472

**Severity**: MEDIUM · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D4-2026-09-19-01) · **Location**: `crates/papyrus/src/parser/mod.rs:290` (colon-qualified ident), `mod.rs:369` (`[` type suffix), `stmt.rs:262` (assign-op), `stmt.rs:206` (VarDecl disambiguation), `script.rs:203-214` (top-level type→item peek), `script.rs:112` (header flag loop)

**Description**
#4321 made the Pratt loop newline-terminating via `peek_raw()`, but the six sites above still decide on newline-skipping `peek()`/`check()`. All six are probe-confirmed to glue the next line into the construct with **zero errors**:
- `foo` ⏎ `:bar()` → one `Call(Ident("foo:bar"))`
- `x` ⏎ `= 5` → one `Assign`
- `Foo` ⏎ `bar = 1` → one `VarDecl`
- `Actor` ⏎ `[] props` → Array-typed Variable
- `Int` ⏎ `Property P = 5` / `Int` ⏎ `Function F()` glue at top level
- the header flag loop consumes a following line's `Native`/`Const`/`Hidden`/`DebugOnly` into `ScriptFlags` — probe: a function's own `Native` silently promoted to a script flag with `Function.flags` left empty

Unlike #4321's valid-source shape, every one of these requires at least one invalid line — hence MEDIUM, not HIGH. This is the residue class SCR-D4-2026-09-14-02 named ("audit the other same-line peek/check decisions"); #2656 was fixed only at `parse_property_flags`.

**Evidence**
All six shapes probe-confirmed zero-error (dim-4 probe table, AUDIT_PAPYRUS_2026-09-19). Most consequential is the header-flag swallow (`script.rs:112-131`): it corrupts a flag bitfield silently rather than merely re-shaping already-broken source.

**Impact**
Malformed mod source is silently accepted as a plausible-but-wrong AST instead of erroring; `extender_preflight` under/over-reports flags and calls on such source. No valid-source shape found in the probe set.

**Related**: #4321 (fixed — the valid-source instance), #2656 (fixed at one site)

**Suggested Fix**
Apply the same `peek_raw()` discipline (or a same-line span check: candidate token's `span.start` must not exceed the previous consumed token's line end) at the six sites, and pin each with a zero-error-glue regression test like `a_line_opening_with_a_paren_is_not_a_call_on_the_previous_line`.

## Completeness Checks
- [ ] **SIBLING**: Re-sweep `parser/{mod,stmt,script,expr}.rs` for any remaining newline-skipping decision on a statement/item boundary
- [ ] **TESTS**: One zero-error-glue regression test per site
