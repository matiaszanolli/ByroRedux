# SCR-D5-NEW2-01: SetObjective{Displayed,Completed,Failed} effect primitives collapse present-but-non-literal argument into absent-default true

**Labels**: high, bug

**Severity**: HIGH
**Dimension**: Recognizer-Chain Soundness
**Untrusted-Input**: Yes — reachable via any decompiled quest-fragment `.pex` through the now-wired `populate_quest_fragments_from_pex` → `lower_fragment` path.
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`crates/scripting/src/translate/effects.rs:227-259` (`prim_set_objective_displayed`/`_completed`/`_failed`), `bool_arg` helper at `:297-299`.

## Description
`bool_arg(args, idx)` returns `Some(as_num(...)? != 0.0)`, collapsing two distinct cases into one `None`: the argument slot is genuinely absent (Papyrus optional-arg omission — default should apply) vs. present but not a literal (a local bool variable, `Not(...)`, a copy-propagated temp — a term the primitive can't evaluate). All three call sites do `bool_arg(args, 1).unwrap_or(true)`, so a present-non-literal argument silently becomes `true` exactly as if omitted. This is the identical defect class `f63a701e`/#1909 fixed in `rumble.rs::float_prop`/`bool_prop` one file over — but that fix touched only the guard-side coercion, not this sibling effect-side table.

Verified current: `bool_arg` in `crates/scripting/src/translate/effects.rs` still has the single-`Option<bool>` signature; all three `SetObjective*` primitives still call `.unwrap_or(true)` directly on it.

## Evidence
`as_num` (`compose.rs:76-84`) only matches `IntLit`/`FloatLit`/`BoolLit`/`Cast`; any other `Expr` (notably `Expr::Ident`, the shape a local variable takes) returns `None`, which `bool_arg` also turns into `None` — indistinguishable from "argument doesn't exist." No test exercises a non-literal 2nd argument to any `SetObjective*` call.

## Impact
A fragment statement like `Self.SetObjectiveCompleted(20, bWasSuccessful)` — ordinary, unconstrained Papyrus, unlike auto-property initializers which the grammar restricts to literals (the reasoning that kept the rumble case at LOW does not transfer here) — is emitted as `completed: true` regardless of the real runtime value. `QuestObjectiveState` is live-dispatched by `quest_fragment_dispatch_system` and persisted in save data (`byroredux/src/save_io.rs`), so this silently and permanently corrupts quest-journal state. Reachable the moment a real quest fragment with a non-literal completion flag decompiles through the now fully-wired `--scripts-bsa` path.

## Related
Sibling defect to the just-fixed #1907 (same file) and #1909 (`rumble.rs`) — same "present-non-literal collapses into absent-default" shape, in the one table those fixes didn't touch.

## Suggested Fix
Give `bool_arg` the same `Option<Option<bool>>` contract `rumble.rs::bool_prop` now has: `None` when the index is out of range (genuinely absent), `Some(None)` when present but `as_num` fails (decline), `Some(Some(v))` when present and literal. Update the three call sites to `bool_arg(args, 1)?.unwrap_or(default)` and add a guard test asserting `lower_fragment` returns `None` for `Self.SetObjectiveCompleted(20, someVar)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `rumble.rs::float_prop`/`bool_prop` already fixed under #1909; this brings `effects.rs::bool_arg` to parity
- [ ] **TESTS**: A regression test pins this specific fix
