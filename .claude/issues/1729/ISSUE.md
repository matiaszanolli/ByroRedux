# SAFE-2026-06-23-02: .pex control-flow reconstructor recurses on untrusted input with no depth cap

**Issue**: #1729
**Severity**: LOW
**Labels**: low, safety, bug
**Source audit**: `docs/audits/AUDIT_SAFETY_2026-06-23.md`
**Dimension**: 2 — Memory Corruption / UB (stack-overflow facet)
**Location**: `crates/pex/src/decompile/control_flow.rs::rebuild` (recursive self-calls at ~:146, ~:154, ~:163, ~:164)

## Description
The M47.2 `.pex` decompiler's control-flow reconstruction (`Reconstructor::rebuild`) recurses to nest if/while regions. A `.pex` is untrusted on-disk input — the same threat class that earned the NIF walkers `MAX_NIF_NODE_DEPTH` (#1269) and the Papyrus expr parser `MAX_EXPR_DEPTH = 256` (#1270). `rebuild` has **no equivalent depth bound**. Normally terminating (strictly-shrinking block-index sub-ranges + `end < start`/`fail()` guards), but a pathologically deep yet structurally valid nested-control-flow `.pex` could overflow the native stack → abort.

## Evidence
- `control_flow.rs:146` `let body = self.rebuild(body_start, body_end)?;`
- lines 154, 163, 164 — three more self-recursive `rebuild(...)` calls.
- No `depth` parameter / `MAX_*` constant / counter in the file.
- Contrast: `crates/nif/src/import/walk/mod.rs:186` threads `depth: u32` (#1269); `crates/papyrus/src/parser/expr.rs:36` gates on `MAX_EXPR_DEPTH` (#1270).

## Impact
A crafted/corrupt `.pex` could crash the decompiler via stack overflow. LOW because: (a) valid files have recursion bounded by script size — needs a deliberately adversarial file; (b) outcome is a clean abort, not memory corruption. Direct analogue of two already-fixed issues.

## Related
#1269, #1270

## Suggested Fix
Thread a `depth: u32` through `rebuild` and return `self.fail()` past a `MAX_PEX_REGION_DEPTH` constant (mirror #1269/#1270). Add a synthetic deeply-nested CFG test asserting graceful error.

## Completeness Checks
- [ ] **SIBLING**: Cap pattern matches the #1269/#1270 walkers
- [ ] **TESTS**: A synthetic deeply-nested CFG test asserts graceful `Err` rather than stack overflow

## Validation
CONFIRMED against current code (HEAD 2d4c350d): control_flow.rs rebuild() recurses at 146/154/163/164 with no depth parameter or MAX_* constant.
