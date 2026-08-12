# #2668: SCR-D4-NEW11-02: OffsetMap::to_original is an unindexed linear scan over an already-sorted vec, giving O(N*E) error remapping

**Severity**: LOW
**Dimension**: Papyrus Lexer & Pratt Parser (Dimension 4)
**Untrusted-Input**: Yes
**Location**: `crates/papyrus/src/lexer.rs:66-81` (`OffsetMap::to_original`)
**Status**: NEW

## Description

`to_original` walks the whole entry vec linearly on every call, once per reported parse error, over a vec that is already sorted by construction (`OffsetMap::push` appends in increasing preprocessed-offset order).

The result is O(N*E) in line-continuations x errors.

## Evidence

Measured clean quadratic scaling -- 4x time per 2x input:

```
 4k continuations + errors ->   5 ms
32k continuations + errors -> 305 ms
```

The two axes in isolation are each linear; only the product is quadratic. `crates/papyrus/src/lexer.rs:66` is a plain `for` over `self.entries` with no bisection.

## Impact

Only reachable with a `.psc` carrying both many line continuations and many parse errors, and `.psc` has no production consumer today (the engine's live path consumes `.pex`; every `parse_script` call site is a test or an example).

Real but bounded: a hardening / quality item rather than a live performance problem.

## Related

SCR-D4-NEW11-01 (same file, same pass); the dead `removed` accumulator in `preprocess` is a free cleanup to fold in here

## Suggested Fix

Replace the linear scan with `partition_point` (the vec is already sorted, so bisection is a drop-in). Optionally drop the unused `removed` accumulator in `preprocess` in the same change.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
