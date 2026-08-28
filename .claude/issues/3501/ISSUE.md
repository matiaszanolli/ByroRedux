# #3501: SCR-D3-2026-08-27-01 (regression of #3019): the #3019 fix replaced a stale decompiler pass order with a wrong one — decompile/mod.rs now lists the boolean pass last, contradicting decompile_body and docs/feature-matrix.md

**Labels**: low, scripting, documentation, doc-rot
**Filed**: 2026-08-27 (`/audit-publish` of `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`)

- **Severity**: LOW
- **Dimension**: Decompiler — Control-Flow / Boolean / Lower (Dimension 3)
- **Untrusted-Input**: No
- **Location**: `crates/pex/src/decompile/mod.rs:7-18`
- **Regression of**: #3019 (CLOSED 2026-08-26 by `149e9c03`) — the fix landed but states a different wrong order; nothing open tracks it
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

#3019 filed `decompile/mod.rs`'s module docstring as first-commit-era ("phase 1 — this commit", phases 2–4 "(next)"). The fix rewrote it to name five phases — but ordered them `cfg → lift → control_flow → lower → boolean`, putting the short-circuit boolean pass **last**.

The actual pipeline runs the boolean pass **third**, before control-flow reconstruction, and this ordering is load-bearing: the boolean pre-pass collapses `&&`/`||` chains into one conditional so the control-flow pass sees a clean diamond, and `control_flow.rs`'s conditional-predecessor branch fails closed (#1732) precisely because well-formed input should never reach it *after* the boolean pass has run.

The commit message states the wrong order as fact and records that `docs/engine/scripting.md` and `m47-2-design.md` were checked — but `docs/feature-matrix.md:174`, corrected in the *same* commit under #2542, now carries the **right** order, so the two docs contradict each other.

## Evidence

```rust
// crates/pex/src/decompile/lower.rs:226-237 — the real pipeline (decompile_body)
fn decompile_body(
    object: &Object,
    func: &PexFunction,
) -> Result<Vec<Spanned<Stmt>>, DecompileError> {
    let mut cfg = build_cfg(func)?;
    let mut scopes = lift_function(object, func, &cfg)?;
    // Collapse `&&`/`||` short-circuits before control-flow reconstruction
    // so compound conditions surface as one expression, not nested ifs.
    rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
    let nodes = reconstruct(cfg, scopes, &func.name)?;
    Ok(lower_body(&nodes))
}
```

```rust
// crates/pex/src/decompile/mod.rs:13-18 — the docstring that landed under #3019
//! 3. [`control_flow`] — control-flow reconstruction (if/else, loops) over
//!    the CFG.
//! 4. [`lower`] — lowers the node tree → `byroredux_papyrus::ast::Script`,
//!    with a fidelity gate.
//! 5. [`boolean`] — short-circuit boolean-operator reconstruction
//!    (`rebuild_boolean_operators`).
```

```
docs/feature-matrix.md:174: | Full Papyrus transpiler (M47.2) | ✓ `.pex` recognizer slice
  (CFG→lift→short-circuit→control-flow→lower); full transpiler deferred |
```

All three re-verified against current `main` on 2026-08-27.

## Impact

Doc-rot only, but on the one ordering fact the domain's own skill file calls load-bearing, in the module docstring a reader reaches first. #2542 and #3019 were filed as the *same* defect in two files; one was fixed correctly and one was not, and both are now closed — so no open issue tracks the surviving half.

## Related

#3019 (CLOSED — this is its regression), #2542 (CLOSED, fixed correctly in `docs/feature-matrix.md`), #1732 (`control_flow.rs`'s fail-closed conditional-predecessor branch, whose rationale depends on the boolean pass running first).

## Suggested Fix

Swap phases 3–5 in `crates/pex/src/decompile/mod.rs` to `boolean → control_flow → lower`, and add a one-line pointer to `lower.rs::decompile_body` as the authority so the next rewrite has a source to check against.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`docs/engine/scripting.md`, `docs/engine/m47-2-design.md`, and the `/audit-scripting` skill file — every doc that restates the pass order must match `decompile_body`)
- [ ] **TESTS**: A regression test pins this specific fix
