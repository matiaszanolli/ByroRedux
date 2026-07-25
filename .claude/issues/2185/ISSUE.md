# SCR-D4-NEW4-01: Unterminated State/Struct/Group hangs the Papyrus parser forever

**Issue**: #2185
**Labels**: high, bug
**Dimension**: Papyrus Lexer & Parser (Dimension 4)
**Untrusted-Input**: Yes — any truncated or hand-malformed `.psc` file (network transfer cutoff, disk corruption, a mod author's file missing one closing keyword) triggers this
**Location**: `crates/papyrus/src/parser/script.rs` — `parse_state` (loop ~509-579), `parse_struct` (~584-618), `parse_group` (~621-679)
**Status**: NEW — regression introduced by the #2125 fix (commit `cacc9935`), independently found by an orphaned sub-agent from an earlier attempt at this audit and re-confirmed empirically in this pass

## Description

The #2125 fix correctly gave `parse_state`/`parse_struct`/`parse_group` a per-child recovery loop instead of a bare `?` — but none of the three loops check `self.at_eof()` before dispatching into their catch-all `_ =>` arm, unlike `parse_script`'s own top-level loop (`script.rs:74`, `if self.at_eof() { break; }`).

At genuine EOF, `peek()` returns `None`, which falls into the loop's `_` arm. That arm calls `parse_type()` (or `parse_variable_body()`), which at EOF fails **without consuming any token** — confirmed directly: `parse_base_type` (`crates/papyrus/src/parser/mod.rs:319-323`) returns `Err(ParseError::unexpected_eof(...))` as soon as `self.pos >= self.tokens.len()`, without advancing `self.pos`. The error handler then calls `skip_to_next_line()` (`script.rs:691-697`), which also consumes nothing at EOF — it loops on `advance_raw()`, which returns `None` immediately when `self.pos >= self.tokens.len()` (`parser/mod.rs:111-120`). Control falls through `continue` back to the top of the loop with `self.pos` unchanged — an infinite loop, 100% CPU, no progress, ever.

Confirmed present in all three loops:
- `parse_state`'s `_ =>` arm (`script.rs:556-576`)
- `parse_struct`'s `_ =>` arm (`script.rs:597-616`)
- `parse_group`'s `_ =>` arm (`script.rs:653-675`)

None of the three check `at_eof()` before dispatching, and no regression test exists pinning this shape.

## Evidence

Independently re-confirmed via a throwaway test spawning `parse_script` on a thread with a 3-second `mpsc::recv_timeout`. Input `"ScriptName Foo\n\nState MyState\n"` (missing `EndState`) hung past the timeout. The same shape was independently confirmed for `Struct`/`EndStruct` and `Group`/`EndGroup` by the original finder.

Notably, **pre-#2125-fix this did not hang**: the old bare-`?` shape propagated the EOF error immediately and returned `Err`, unwinding cleanly. The recovery loop introduced by the fix removed that early exit without replacing the termination condition it depended on.

## Impact

A straightforward denial-of-service on any code path that parses untrusted/imported `.psc` source (mod installation, community content import) — the parser hangs the calling thread indefinitely on a very ordinary corruption/truncation shape, not an exotic adversarial input.

## Suggested Fix

Add `if self.at_eof() { push_error(ParseError::unexpected_eof(...)); break; }` as the first check inside each of the three loops' `_ =>` arm (or before dispatching into it), mirroring `parse_script`'s own `at_eof()` guard. A single shared helper (e.g. a `container_body_loop` combinator) would prevent this class of drift the next time a fourth container is added — see the sibling MEDIUM finding SCR-D4-NEW4-02 (#2188), which is the same bug shape in a fourth container the #2125 fix didn't reach at all.

## Completeness Checks
- [ ] **TESTS**: A regression test with a bounded-time harness (thread + timeout, or an iteration cap) pins that an unterminated `State`/`Struct`/`Group` returns `Err`/recovers instead of hanging
- [ ] **SIBLING**: `parse_property_accessors` has the same missing-`at_eof()` gap (SCR-D4-NEW4-02, #2188) — fix both in the same pass if convenient, ideally via a shared recovery-loop helper
