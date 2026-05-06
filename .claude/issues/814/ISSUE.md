# #814 — FO4-D4-NEW-02: TXST DNAM flag bits dropped on 100% of vanilla records (382/382)

**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/cell/support.rs:230-258`, `crates/plugin/src/esm/cell/mod.rs:514-530`
**Source**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md`
**Created**: 2026-05-04
**Sibling**: #813 (DODT, same parser path) — bundle into one PR.

## Summary

`parse_txst_group` has no `b"DNAM"` arm. Every vanilla `Fallout4.esm`
TXST (382/382) carries a DNAM (u16 on FO4, u8 on Skyrim). Renderer
consequence: `HasModelSpaceNormals` flips normal-map decoding —
face TXSTs read garbage normals today.

## How to fix

```
/fix-issue 814
```
