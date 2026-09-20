# PEX-D2-2026-09-19-02: is opcode decompiles to an object-typed cast — third Champollion departure needs bookkeeping

- **ID**: D2-02
- **Labels**: low,scripting,bug
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4477

**Severity**: LOW · **Dimension**: Decompiler CFG & Lift (observed at `lift.rs:231`; rewrite in `lower.rs:110-118`) · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D2-2026-09-19-02) · **Location**: `crates/pex/src/decompile/lower.rs:110-118` (rewrite), `crates/pex/src/decompile/lift.rs:231` (producer)

**Description**
`a is T` (opcode 36, Bool result) lowers to `Expr::Cast { expr: a, target_type: T }` — an object-typed `as` expression — because the shared Papyrus AST has no `is` operator. In an `if` condition the rewritten AST is ill-typed Papyrus (`as` does not yield Bool). Documented in-code as deliberate and "recognizer-irrelevant"; recognizers key on names/calls/condition shape and ≈0 vanilla scripts contain opcode 36 (no `.psc` construct emits it), so no current consumer diverges. This is a third in-code Champollion departure alongside the two documented in the scripting baseline — it should be findable there too.

**Evidence**
`lower.rs:110-118` comment "lower to the structurally closest `a as T`. Rare and recognizer-irrelevant."; the 09-11→09-14 corpus runs show no `is`-related errors.

**Impact**
A future consumer that renders the lower-only AST to source text or evaluates it strictly would emit/reject ill-typed conditions for FO4+/Starfield scripts containing opcode 36. Silent wrong AST with no current divergence.

**Related**: departure bookkeeping adjacent to #4115

**Suggested Fix**
Either add an `Is`-capable expression (or a condition-only `TypeTest` variant) to the shared AST, or record the departure next to the two documented Champollion departures in the scripting audit baseline so it is findable.

## Completeness Checks
- [ ] **SIBLING**: If the AST gains an `Is` operator, check the `.psc` parser never produces it spuriously
- [ ] **TESTS**: Whatever route is chosen, a pin documents the opcode-36 lowering
