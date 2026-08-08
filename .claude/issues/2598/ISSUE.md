# FO4-D4-02: data_size stride override can turn recoverable mismatch into hard parse failure

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2598
**Finding ID**: FO4-D4-02

**Severity**: MEDIUM
**Dimension**: 4 (NIF Parser)
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:336-368` (stride override), `:1085-1096` (consumed guard)
**Status**: NEW

## Description
The `#621` `data_size`-derived stride override applies unconditionally in
both padding directions (understated and overstated). When the declared
stride is understated but still evenly divisible into `data_size`, the
override can turn what would otherwise be a recoverable stride mismatch into
a hard whole-mesh parse failure, because the resulting per-vertex `consumed`
byte count then exceeds `vertex_size_bytes` and trips the guard at
`:1085-1096`.

## Evidence
`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:336-368` computes and
applies the override without checking whether the override direction would
push `consumed` past `vertex_size_bytes`; the guard at `:1085-1096` then
fails the whole shape rather than falling back to the declared (pre-override)
stride.

## Impact
Not observed on any vanilla FO4 content in this audit pass (100% clean parse
across the corpus) — this is a mod-content-only exposure. A mod-authored
mesh with an understated-but-divisible stride could go from "recoverable
with the declared stride" to "hard parse failure" purely because of this
override.

## Suggested Fix
Before applying the `data_size`-derived override, check whether it would
push `consumed` past `vertex_size_bytes`; if so, skip the override and fall
back to the declared stride rather than hard-failing the shape.

## Completeness Checks
- [ ] **SIBLING**: Check other stride-override sites in the same file for the same asymmetry
- [ ] **TESTS**: A regression test with a synthetic understated-but-divisible stride fixture pins the fallback behavior
