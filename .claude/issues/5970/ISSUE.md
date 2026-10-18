# SCR-D1-2026-10-17-02: Quest-script inlining has no total-effect cap - multiplicative expansion of a mod `.pex` can exhaust memory at cell load

Issue: #5970
Labels: high, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-02: Quest-script inlining has no total-effect cap - multiplicative expansion of a mod `.pex` can exhaust memory at cell load

- **Severity**: HIGH
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/inline.rs:45-199` (`inline_quest_script_call`), `lower.rs:375-494`, `fragment/populate.rs:437,670,777`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: A call to a quest-script function is lowered by re-lowering the callee body at every call site and splicing the result in. The only bounds are `MAX_INLINE_DEPTH = 4` and the recursion guard `inline_stack.contains(&frame)` (`:80`), which stops the same function re-entering itself but not distinct functions. There is no memoisation and no cap on the number of effects a fragment may expand to, so a script whose functions each contain N calls to the next function lowers to ~N^4 effects (the fragment calls F1; F1 calls F2 N times; F2 calls F3 N times; F3 calls F4 N times; F4 holds N leaf effects). N is bounded only by the pex size / the per-function instruction limit.
- **Evidence**: code path above; `lower_statements` -> `ExprStmt` -> `classify_effect_with_providers` miss -> `inline_quest_script_call` (`lower.rs:461`) -> `lower_statements(&function.body, ..)` (`inline.rs:182`) with `effects.extend(inlined)`. Callee `Self.G()` calls inline again because `callee_scope` binds `self` as a quest local (`inline.rs:125-126`). No `MAX_*` effect or work budget exists in `translate/effects/**` or `fragment/populate.rs`. (Not exercised - the repo is read-only for this audit; the multiplication follows directly from the code.) `prune_unread_locals`, `locals_never_suspend` and `quest_property_names_with_library` also re-walk the full expanded tree / library per call.
- **Impact**: Fragment population runs on the cell-load path for every quest/scene/INFO with a quest script; N=100 gives 10^8 `Effect` values (tens of GB) - an out-of-memory abort or minutes-long stall from a modded quest script. Skill escalation: unbounded allocation on mod-supplied bytes.
- **Related**: #3279 (depth cap precedent for `lower_statements`), #5801 (split moved this verbatim).
- **Suggested Fix**: Thread a shared work budget (e.g. max lowered statements / effects per fragment, counted across `inline_quest_script_call` and `lower_statements`) that declines the fragment when exceeded; optionally memoise per `(script, function)` within one lowering.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
