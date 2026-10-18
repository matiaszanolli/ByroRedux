# SCR-D7-2026-10-17-09: Receiver-expression chains and saved Wait values are not covered by the nesting / wait caps (defense in depth)

Issue: #6025
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-09: Receiver-expression chains and saved Wait values are not covered by the nesting / wait caps (defense in depth)

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `lower_call.rs:481` (`lower_provider_value(&object.node, .., 1)` restarts depth at 1 for each receiver link), `execute.rs:1037, 715, 682` (`materialize_provider_arguments(.., 0)` restarts depth per call), `execute.rs:503-507` (`Wait` validation)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: (a) `MAX_PROVIDER_HANDLER_NESTING` does not bound a `a.f().f().f()...` receiver chain: lowering, `validate_provider_call` and `materialize_provider_arguments` all restart their depth counter at each call boundary, so the bound is really the front-ends' `MAX_EXPR_DEPTH` = 256 (`papyrus/src/parser/expr.rs:24`, `pex/decompile/lift.rs:380`) -> at most ~128 recursion levels, small frames, so not exploitable today; but the cap this layer advertises is not what enforces it. (b) `validate_provider_statements` re-validates a saved continuation's `Wait` only as finite and >= 0, not `<= MAX_PROVIDER_WAIT_SECONDS`; a continuation saved before #5657 (or a hand-edited save) with `3e38` s therefore survives and, because overflow drops the newest tails, can starve the 4 096-slot queue.
- **Impact**: Hardening only.
- **Related**: #5657, #5656.
- **Suggested Fix**: Thread `depth` through the receiver-expression calls; add the `<= MAX_PROVIDER_WAIT_SECONDS` check to the `Wait` arm of `validate_provider_statements`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
