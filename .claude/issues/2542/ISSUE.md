# SCR-D3-NEW10-01: feature-matrix.md's M47.2 row states an incorrect decompiler pass order

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2542
**Finding ID**: SCR-D3-NEW10-01

**Severity**: LOW (doc-only; the correct ordering lives in `lower.rs`'s and `control_flow.rs`'s own module docs, so an engineer reading source would not be misled — only a reader of the feature matrix alone would be)
**Dimension**: Decompiler Control-Flow/Boolean/Lower
**Untrusted-Input**: No — documentation only
**Location**: `docs/feature-matrix.md:157`
**Status**: NEW (not previously filed; confirmed absent from the 94 open issues checked)

## Description
The parenthetical lists the decompiler pipeline as `CFG→lift→control-flow→lower→short-circuit`. The real order, verified against `decompile_body` in `lower.rs`, is `cfg → lift → rebuild_boolean_operators (short-circuit) → reconstruct (control-flow) → lower_body`. Two swaps: short-circuit collapse is third, not last; control-flow reconstruction is fourth, not third.

## Evidence
Confirmed directly at `docs/feature-matrix.md:157`: "CFG→lift→control-flow→lower→short-circuit". `crates/pex/src/decompile/lower.rs:230-236`:
```rust
let mut cfg = build_cfg(func)?;
let mut scopes = lift_function(object, func, &cfg)?;
// Collapse `&&`/`||` short-circuits before control-flow reconstruction
// so compound conditions surface as one expression, not nested ifs.
rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
let nodes = reconstruct(cfg, scopes, &func.name)?;
Ok(lower_body(&nodes))
```

## Impact
Cosmetic only. A reader relying solely on the feature matrix could form an incorrect mental model of pipeline structure.

## Suggested Fix
Update `docs/feature-matrix.md:157` to read `CFG→lift→short-circuit→control-flow→lower` (matching module names).

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
