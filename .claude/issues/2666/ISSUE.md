# #2666: SCR-D2-NEW11-01: rebuild_expression's producer-drop guard is a debug_assert!, compiled out in release

**Severity**: LOW
**Dimension**: Decompiler CFG & Lift (Dimension 2)
**Untrusted-Input**: Yes (latent)
**Location**: `crates/pex/src/decompile/lift.rs:403`
**Status**: NEW

## Description

Copy-propagation splits its work across two independently maintained traversals: `count_constant_id` walks `Node::child_nodes()`, `replace_constant_id` walks `Node::child_nodes_mut()`. The `>1` arm correctly fails closed with `ExpressionRebuildFailed`, but the "verified single match must be consumed" postcondition is only a `debug_assert!`.

They agree today -- diffed arm-for-arm across all 17 `NodeKind` variants -- but nothing pins the parity, and `node.rs` has **zero tests**.

## Evidence

`crates/pex/src/decompile/lift.rs:403`:

```rust
debug_assert!(slot.is_none(), "verified single match must be consumed");
```

A future divergence between the two child enumerations (one arm added to `child_nodes_mut` but not `child_nodes`, or vice versa) would take the success path in release with `slot` still `Some`, silently deleting the producer statement while the consumer keeps a dangling `::tempN` reference -- a **wrong AST** rather than a fail-closed `Err`.

## Impact

Latent, not live -- the two traversals are currently in agreement, verified by direct arm-for-arm diff this pass. LOW on that basis.

The concern is the failure *mode* rather than the current state: this is the one place in the copy-propagation pass where a maintenance slip degrades to silent AST corruption instead of an error, and the parity it depends on is unpinned by any test.

## Related

#2024 (the O(n^2) fix that rewrote this function); SCR-D3-NEW11-01 (the other wrong-AST-without-error path in the decompiler)

## Suggested Fix

Return `ExpressionRebuildFailed` on an unconsumed slot, exactly as the `>1` arm already does -- one line, converts a release-mode silent corruption into a clean decline. Add a `child_nodes` / `child_nodes_mut` parity test over all `NodeKind` variants.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
