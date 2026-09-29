# #5068 — SCR-D1-2026-09-29-01: The canonical effect table ignores CallArg::name — a named argument binds positionally

**Labels**: low, bug, scripting

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-29.md` — finding `SCR-D1-2026-09-29-01`

**Severity**: LOW

**Dimension**: Recognizer Chain

**Untrusted-Input**: No (unreachable from `.pex`)

**Location**:
`crates/scripting/src/translate/compose.rs:81-100` (`method_call`, `int_arg`),
`crates/scripting/src/translate/effects.rs:1738-1743` (`bool_arg`), and every `prim_*`.

**Status in report**: NEW

## Description

no helper in `translate/` inspects `CallArg::name`. The provider seam does
(`papyrus_provider/lower_call.rs:357`, `lower_program.rs:531/550/678`). As a result,
`SetEnemy(PlayerFaction, abOtherIsNeutralToSelf = true)` would lower as `self_neutral: true`, the inverted direction.
This accepts a wrong AST.

## Impact

none on the production path today. `crates/pex/src/decompile/lower.rs:166-169` always emits `name: None`.
Only hand-authored `.psc` reaches this code, and every current `parse_script` caller is a test, including the `.psc` half
of the `recognizes_da10…` fidelity gate. This is a latent hole in the decline invariant.

## Suggested Fix

decline in `method_call` (or a shared argument accessor) when any argument carries a name, as the
provider seam does.

Validated at HEAD 9fcfdc3fc: `method_call` / `int_arg` (`crates/scripting/src/translate/compose.rs`) and `bool_arg` (`translate/effects.rs`) index `args` positionally and never inspect `CallArg::name`; `crates/pex/src/decompile/lower.rs` emits `name: None`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `prim_*` argument accessor)
- [ ] **TESTS**: A regression test pins this specific fix (a named-argument call declines)
