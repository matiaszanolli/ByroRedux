# SCR-2026-10-27-03: an `as Bool` cast is transparent in the fragment lowering, so a Bool-cast numeric reading compared with a literal lowers as numeric equality

Labels: low, scripting, bug
Source: docs/audits/AUDIT_SCRIPTING_2026-10-27.md

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-27.md` finding `SCR-2026-10-27-03` (LOW)
**Location (re-mapped to current tree)**: `crates/scripting/src/translate/effects/guards.rs` (`guard_reading`: non-Int cast returns the inner reading, ~:404-418); `classify_value` (strips non-Int casts); `crates/scripting/src/translate/compose.rs` (`as_num` strips casts off literals, ~:138-150)

**Severity**: LOW (decline-invariant hole; no vanilla occurrence found). Dimension: Recognizer Chain. Untrusted input: Yes (`.pex`-derived AST). Residual of #6215, which made only `as Int` exact.

## Description
`(G.GetValue() as Bool) == True` classifies as `(Global G, Eq, 1.0)`, i.e. `G == 1`. Papyrus means `G != 0`. `(A.GetValue() as Bool) == (B.GetValue() as Bool)` becomes `Compare A == B`. Both diverge for any value other than 0 or 1. The bare atom `If G.GetValue() as Bool` is correct (becomes `Ne 0`). A literal cast like `GetStage() >= (15.5 as Int)` is likewise compared untruncated (`as_num` unwraps any `Cast`).

## Evidence
`guard_reading` (verified at HEAD): `if !matches!(target_type.node, Type::Int) { return Some(inner); }`; `as_num`: `Expr::Cast { expr, .. } => as_num(&expr.node)`. The FO4 `Scripts/Source` tree has 0 matches for `as bool) ==`; Skyrim source is not on disk so it could not be checked.

## Impact
None on vanilla data checked. A modded `.pex` with that shape would lower to a wrong guard instead of declining to the VM (violates decline-on-unmodeled).

## Related
#6215, #6216.

## Suggested Fix
In `classify_comparison` / `classify_value`, decline when a `Bool` (or `String`) cast wraps a numeric reading; alternatively fold it to `(reading, Ne, 0)` against the literal's truth. Have `as_num` refuse a cast literal whose cast changes the value.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

